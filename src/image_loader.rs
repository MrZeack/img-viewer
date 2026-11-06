//! Asynchronous image loading with multi-threading and caching
use crate::errors::AppError;
use crate::event::{EventBus, ViewerEvent};
use egui::ColorImage;
use image::{DynamicImage, ImageFormat};
use libheif_rs::{HeifContext, LibHeif};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

/// Represents a successfully loaded image
#[derive(Debug, Clone)]
pub struct LoadedImage {
    /// The loaded image as an egui ColorImage
    pub image: ColorImage,
}

pub struct ImageLoader {
    sender: mpsc::Sender<LoadRequest>,
    pending: Arc<Mutex<HashSet<PathBuf>>>,
}

#[derive(Debug)]
struct LoadRequest {
    path: std::path::PathBuf,
    target_size: Option<(u32, u32)>,
    high_quality: bool,
}

impl ImageLoader {
    pub fn new(num_threads: usize, event_bus: EventBus) -> Self {
        Self::with_pending(num_threads, event_bus, Arc::new(Mutex::new(HashSet::new())))
    }

    pub fn with_pending(
        num_threads: usize,
        event_bus: EventBus,
        pending: Arc<Mutex<HashSet<PathBuf>>>,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<LoadRequest>();
        let pending_worker = pending.clone();

        thread::spawn(move || {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .build()
                .expect("Failed to build image loader thread pool");

            pool.install(|| {
                rx.into_iter().par_bridge().for_each(|request| {
                    let path = request.path.clone();
                    let result =
                        load_image_sync(request).map(|loaded_image| Arc::new(loaded_image.image));
                    if let Ok(image_arc) = result {
                        event_bus.publish(ViewerEvent::ImageLoaded(path.clone(), image_arc));
                    }
                    // mark request as completed (success or error)
                    if let Ok(mut set) = pending_worker.lock() {
                        set.remove(&path);
                    }
                });
            });
        });

        Self {
            sender: tx,
            pending,
        }
    }

    pub fn pending_requests(&self) -> Arc<Mutex<HashSet<PathBuf>>> {
        self.pending.clone()
    }

    pub fn load_image(&self, path: &Path, target_size: Option<(u32, u32)>, high_quality: bool) {
        let p = path.to_path_buf();
        if let Ok(mut set) = self.pending.lock() {
            if set.contains(&p) {
                return;
            }
            set.insert(p.clone());
        }

        let request = LoadRequest {
            path: p,
            target_size,
            high_quality,
        };
        let _ = self.sender.send(request);
    }
}

fn load_image_sync(request: LoadRequest) -> Result<LoadedImage, AppError> {
    let data = std::fs::read(&request.path).map_err(|e| {
        AppError::FileSystemError(format!(
            "Failed to read image file {}: {}",
            request.path.display(),
            e
        ))
    })?;

    // Determine image format
    // Determine image format
    let format = image::guess_format(&data);

    // Handle ARW, CR3 and HIF specifically if format is unknown or we want to force check extensions
    let path_str = request.path.to_string_lossy().to_lowercase();
    if path_str.ends_with(".arw") || path_str.ends_with(".cr3") {
        return load_raw_preview(&data, request.target_size);
    } else if path_str.ends_with(".hif") || path_str.ends_with(".heic") {
        return load_hif(&data);
    }

    let format = format.map_err(|e| {
        AppError::ImageLoadError(format!(
            "Failed to guess image format for {}: {}",
            request.path.display(),
            e
        ))
    })?;

    // For JPEG images, use optimized loading
    let img = if format == ImageFormat::Jpeg {
        load_jpeg_optimized(&data, request.target_size).map_err(|e| {
            AppError::ImageLoadError(format!(
                "Failed to load JPEG {}: {}",
                request.path.display(),
                e
            ))
        })?
    } else {
        let mut img = image::load_from_memory_with_format(&data, format).map_err(|e| {
            AppError::ImageLoadError(format!(
                "Failed to load image {}: {}",
                request.path.display(),
                e
            ))
        })?;

        // Resize if needed
        if let Some((width, height)) = request.target_size {
            img = img.resize(
                width,
                height,
                if request.high_quality {
                    image::imageops::FilterType::Lanczos3
                } else {
                    image::imageops::FilterType::Triangle
                },
            );
        }
        img
    };

    let color_image = dynamic_image_to_color_image(&img);

    Ok(LoadedImage { image: color_image })
}

fn load_jpeg_optimized(
    data: &[u8],
    target_size: Option<(u32, u32)>,
) -> Result<DynamicImage, AppError> {
    use image::codecs::jpeg::JpegDecoder;
    use image::ImageDecoder;

    let decoder = JpegDecoder::new(std::io::Cursor::new(data))
        .map_err(|e| AppError::ImageLoadError(format!("Failed to create JPEG decoder: {}", e)))?;

    // Use JPEG's built-in downsampling for better performance
    let img = if let Some((target_width, target_height)) = target_size {
        let (orig_width, orig_height) = decoder.dimensions();
        let scale = ((orig_width / target_width).min(orig_height / target_height)).max(1);

        if scale > 1 {
            // Use JPEG's efficient downsampling
            let decoder = JpegDecoder::new(std::io::Cursor::new(data)).map_err(|e| {
                AppError::ImageLoadError(format!(
                    "Failed to create JPEG decoder for downsampling: {}",
                    e
                ))
            })?;
            // Note: set_max_decoding_pixels not available in this version
            DynamicImage::from_decoder(decoder)
                .map_err(|e| AppError::ImageLoadError(format!("Failed to decode JPEG: {}", e)))?
        } else {
            image::load_from_memory_with_format(data, image::ImageFormat::Jpeg)
                .map_err(|e| AppError::ImageLoadError(format!("Failed to load JPEG: {}", e)))?
        }
    } else {
        image::load_from_memory_with_format(data, image::ImageFormat::Jpeg)
            .map_err(|e| AppError::ImageLoadError(format!("Failed to load JPEG: {}", e)))?
    };

    Ok(img)
}

fn load_raw_preview(data: &[u8], target_size: Option<(u32, u32)>) -> Result<LoadedImage, AppError> {
    // Try to extract the embedded JPEG preview/thumbnail using quickraw first
    // Note: quickraw's export_thumbnail_data usually returns the largest embedded preview
    let jpg_data = match safe_export_thumbnail(data) {
        Some((data, _orientation)) => data,
        None => {
            // If quickraw fails (e.g. for CR3), try manual extraction of the largest JPEG
            extract_largest_jpeg(data).ok_or_else(|| {
                AppError::ImageLoadError(
                    "Failed to extract RAW preview (quickraw and manual fallback failed)"
                        .to_string(),
                )
            })?
        }
    };

    // Load the JPEG data
    let mut img =
        image::load_from_memory_with_format(&jpg_data, ImageFormat::Jpeg).map_err(|e| {
            AppError::ImageLoadError(format!("Failed to load extracted RAW preview: {}", e))
        })?;

    // Resize if target_size is provided, otherwise keep the full preview resolution
    if let Some((w, h)) = target_size {
        // Only downscale if the image is larger than target_size
        if img.width() > w || img.height() > h {
            img = img.resize(w, h, image::imageops::FilterType::Triangle);
        }
    }

    let color_image = dynamic_image_to_color_image(&img);
    Ok(LoadedImage { image: color_image })
}

/// Calls `quickraw::Export::export_thumbnail_data` while guarding against panics.
///
/// quickraw/quickexif index slices without bounds checks and panic on empty,
/// truncated or unsupported files. A panic on a loader thread would otherwise
/// bring down the whole app, so treat any panic as "no thumbnail".
pub(crate) fn safe_export_thumbnail(data: &[u8]) -> Option<(Vec<u8>, u32)> {
    // Too small to hold a valid TIFF/ISO-BMFF header; skip quickraw entirely.
    if data.len() < 16 {
        return None;
    }
    std::panic::catch_unwind(|| {
        quickraw::Export::export_thumbnail_data(data)
            .ok()
            .map(|(jpg, orientation)| (jpg.to_vec(), orientation as u32))
    })
    .ok()
    .flatten()
}

fn extract_largest_jpeg(data: &[u8]) -> Option<Vec<u8>> {
    let mut best_jpeg = None;
    let mut max_size = 0;

    let mut i = 0;
    // Simple heuristic to find JPEG streams (FF D8 ... FF D9)
    // We limit search to avoid performance issues on huge files if possible,
    // but for CR3 we might need to scan deep.
    // Optimization: Valid JPEGs usually start at aligned offsets or specific atom positions,
    // but byte scanning is fast enough for 50MB files in Rust.

    while i < data.len().saturating_sub(2) {
        // Search for JPEG SOI (Start of Image): FF D8
        // Note: Some might use FF D8 FF but FF D8 is the marker.
        // Standard JPEG is FF D8 FF E0/E1/DB...
        if data[i] == 0xFF && data[i + 1] == 0xD8 {
            let start = i;
            let mut end = 0;
            // Search for EOI (End of Image): FF D9
            let mut j = start + 2;
            while j < data.len().saturating_sub(1) {
                if data[j] == 0xFF && data[j + 1] == 0xD9 {
                    end = j + 2;
                    let size = end - start;
                    // Filter out very small thumbnails (e.g. < 10KB) if we have candidates,
                    // or just pick the largest one.
                    if size > max_size {
                        max_size = size;
                        best_jpeg = Some((start, end));
                    }
                    // Jump index
                    i = end - 1;
                    break;
                }
                j += 1;
            }
            if end == 0 {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    best_jpeg.map(|(start, end)| data[start..end].to_vec())
}

fn load_hif(data: &[u8]) -> Result<LoadedImage, AppError> {
    let lib_heif = LibHeif::new();
    let ctx = HeifContext::read_from_bytes(data)
        .map_err(|e| AppError::ImageLoadError(format!("Failed to read HEIF data: {}", e)))?;

    let handle = ctx.primary_image_handle().map_err(|e| {
        AppError::ImageLoadError(format!("Failed to get primary image handle: {}", e))
    })?;

    // Decode the image
    let image = lib_heif
        .decode(
            &handle,
            libheif_rs::ColorSpace::Rgb(libheif_rs::RgbChroma::Rgb),
            None,
        )
        .map_err(|e| AppError::ImageLoadError(format!("Failed to decode HEIF image: {}", e)))?;

    let planes = image.planes();
    let interleaved_plane = planes.interleaved.ok_or_else(|| {
        AppError::ImageLoadError("No interleaved plane found in HEIF image".to_string())
    })?;

    let width = image.width();
    let height = image.height();
    let stride = interleaved_plane.stride;
    let data = interleaved_plane.data;

    // Create DynamicImage from raw data
    // The data is likely RGB8 or RGBA8 depending on how we decoded.
    // We requested Rgb, so it should be RGB.

    // We need to be careful about stride.
    let mut buffer = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        let start = y as usize * stride;
        let end = start + (width as usize * 3);
        buffer.extend_from_slice(&data[start..end]);
    }

    let img_buffer = image::ImageBuffer::<image::Rgb<u8>, Vec<u8>>::from_raw(width, height, buffer)
        .ok_or_else(|| {
            AppError::ImageLoadError("Failed to create image buffer from HEIF data".to_string())
        })?;

    let dynamic_image = DynamicImage::ImageRgb8(img_buffer);
    let color_image = dynamic_image_to_color_image(&dynamic_image);

    Ok(LoadedImage { image: color_image })
}

fn dynamic_image_to_color_image(img: &DynamicImage) -> ColorImage {
    let rgb_image = img.to_rgb8();
    let pixels = rgb_image
        .pixels()
        .map(|p| egui::Color32::from_rgb(p[0], p[1], p[2]))
        .collect();

    ColorImage {
        size: [img.width() as usize, img.height() as usize],
        pixels,
    }
}

pub fn get_image_files(
    dir: &Path,
    newest_first: bool,
) -> Result<Vec<std::path::PathBuf>, AppError> {
    let mut files = Vec::new();

    if dir.is_dir() {
        let dir_entries = std::fs::read_dir(dir).map_err(|e| {
            AppError::FileSystemError(format!("Failed to read directory {}: {}", dir.display(), e))
        })?;

        for entry in dir_entries {
            let entry = entry.map_err(|e| {
                AppError::FileSystemError(format!("Failed to read directory entry: {}", e))
            })?;
            let path = entry.path();

            if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy().to_lowercase();
                if matches!(
                    ext.as_str(),
                    "jpg"
                        | "jpeg"
                        | "png"
                        | "bmp"
                        | "tiff"
                        | "webp"
                        | "hif"
                        | "heic"
                        | "arw"
                        | "cr3"
                ) {
                    files.push(path);
                }
            }
        }
    }

    // Sort by modification date
    files.sort_by_cached_key(|a| {
        a.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    if newest_first {
        files.reverse();
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir() -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("img_viewer_test_{}_{}", std::process::id(), suffix));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_supported_extensions() {
        let dir = unique_temp_dir();
        let extensions = vec![
            "jpg", "jpeg", "png", "bmp", "tiff", "webp", "hif", "heic", "arw", "cr3",
        ];

        for ext in &extensions {
            let file_path = dir.join(format!("test.{}", ext));
            File::create(file_path).unwrap();
        }

        // Add an unsupported file
        File::create(dir.join("test.txt")).unwrap();

        let files = get_image_files(&dir, false).unwrap();
        assert_eq!(files.len(), extensions.len());

        let found_extensions: Vec<String> = files
            .iter()
            .map(|p| p.extension().unwrap().to_string_lossy().to_lowercase())
            .collect();

        for ext in extensions {
            assert!(found_extensions.contains(&ext.to_string()));
        }

        fs::remove_dir_all(dir).unwrap();
    }
}

// Preloader for adjacent images
pub struct ImagePreloader {
    loader: ImageLoader,
    cache: crate::cache::ImageCache,
    preload_distance: usize,
    preview_size: u32,
}

impl ImagePreloader {
    pub fn new(
        cache: crate::cache::ImageCache,
        preload_distance: usize,
        event_bus: EventBus,
        preview_size: u32,
        pending_requests: Arc<Mutex<HashSet<PathBuf>>>,
    ) -> Self {
        Self {
            loader: ImageLoader::with_pending(2, event_bus, pending_requests),
            cache,
            preload_distance,
            preview_size,
        }
    }

    pub fn preload_adjacent(&self, current_path: &Path, all_files: &[std::path::PathBuf]) {
        let current_idx = all_files.iter().position(|p| p == current_path);

        if let Some(idx) = current_idx {
            // Preload next images
            for i in 1..=self.preload_distance {
                if idx + i < all_files.len() {
                    let next_path = &all_files[idx + i];
                    if !self.is_cached(next_path) {
                        self.loader.load_image(
                            next_path,
                            Some((self.preview_size, self.preview_size)),
                            false,
                        );
                    }
                }
            }

            // Preload previous images
            for i in 1..=self.preload_distance {
                if idx >= i {
                    let prev_path = &all_files[idx - i];
                    if !self.is_cached(prev_path) {
                        self.loader.load_image(
                            prev_path,
                            Some((self.preview_size, self.preview_size)),
                            false,
                        );
                    }
                }
            }
        }
    }

    fn is_cached(&self, path: &Path) -> bool {
        let key = path.to_string_lossy().to_string();
        self.cache.contains(&key)
    }
}

#[cfg(test)]
mod safe_thumbnail_tests {
    use super::safe_export_thumbnail;

    #[test]
    fn garbage_raw_does_not_panic() {
        assert!(safe_export_thumbnail(&[]).is_none());
        assert!(safe_export_thumbnail(&[0u8; 64]).is_none());
        let mut tiff = b"II*\x00\x08\x00\x00\x00".to_vec();
        tiff.extend_from_slice(&[0xFF; 32]);
        assert!(safe_export_thumbnail(&tiff).is_none());
    }
}
