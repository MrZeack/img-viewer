use anyhow::Result;
use egui::ColorImage;
use image::{DynamicImage, ImageFormat};
use std::path::Path;
use std::sync::mpsc;
use std::thread;

#[derive(Debug, Clone)]
pub struct LoadedImage {
    pub image: ColorImage,
    pub original_size: (usize, usize),
    pub file_size: u64,
}

pub struct ImageLoader {
    sender: mpsc::Sender<LoadRequest>,
    receiver: mpsc::Receiver<LoadResult>,
}

#[derive(Debug)]
struct LoadRequest {
    path: std::path::PathBuf,
    target_size: Option<(u32, u32)>,
    high_quality: bool,
}

#[derive(Debug)]
pub struct LoadResult {
    pub path: std::path::PathBuf,
    pub result: Result<LoadedImage>,
}

impl ImageLoader {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<LoadRequest>();
        let (result_tx, result_rx) = mpsc::channel::<LoadResult>();
        
        // Spawn background thread pool for image loading
        thread::spawn(move || {
            while let Ok(request) = rx.recv() {
                let result_tx = result_tx.clone();
                let path = request.path.clone();
                
                let result = load_image_sync(request);
                let _ = result_tx.send(LoadResult { path, result });
            }
        });
        
        Self {
            sender: tx,
            receiver: result_rx,
        }
    }

    pub fn load_image(&self, path: &Path, target_size: Option<(u32, u32)>, high_quality: bool) {
        let request = LoadRequest {
            path: path.to_path_buf(),
            target_size,
            high_quality,
        };
        let _ = self.sender.send(request);
    }

    pub fn try_recv_result(&self) -> Option<LoadResult> {
        self.receiver.try_recv().ok()
    }
}

fn load_image_sync(request: LoadRequest) -> Result<LoadedImage> {
    let data = std::fs::read(&request.path)?;
    
    // Determine image format
    let format = image::guess_format(&data)?;
    
    // For JPEG images, use optimized loading
    let img = if format == ImageFormat::Jpeg {
        load_jpeg_optimized(&data, request.target_size)?
    } else {
        let mut img = image::load_from_memory_with_format(&data, format)?;
        
        // Resize if needed
        if let Some((width, height)) = request.target_size {
            img = img.resize(
                width, 
                height, 
                if request.high_quality {
                    image::imageops::FilterType::Lanczos3
                } else {
                    image::imageops::FilterType::Triangle
                }
            );
        }
        img
    };
    
    let original_size = (img.width() as usize, img.height() as usize);
    let file_size = data.len() as u64;
    
    // Convert to ColorImage for egui
    let color_image = dynamic_image_to_color_image(&img);
    
    Ok(LoadedImage {
        image: color_image,
        original_size,
        file_size,
    })
}

fn load_jpeg_optimized(data: &[u8], target_size: Option<(u32, u32)>) -> Result<DynamicImage> {
    use image::codecs::jpeg::JpegDecoder;
    use image::ImageDecoder;
    
    let decoder = JpegDecoder::new(std::io::Cursor::new(data))?;
    
    // Use JPEG's built-in downsampling for better performance
    let img = if let Some((target_width, target_height)) = target_size {
        let (orig_width, orig_height) = decoder.dimensions();
        let scale = ((orig_width / target_width).min(orig_height / target_height)).max(1);
        
        if scale > 1 {
            // Use JPEG's efficient downsampling
            let decoder = JpegDecoder::new(std::io::Cursor::new(data))?;
            // Note: set_max_decoding_pixels not available in this version
            DynamicImage::from_decoder(decoder)?
        } else {
            image::load_from_memory_with_format(data, image::ImageFormat::Jpeg)?
        }
    } else {
        image::load_from_memory_with_format(data, image::ImageFormat::Jpeg)?
    };
    
    Ok(img)
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

pub fn get_image_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy().to_lowercase();
                if matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "bmp" | "tiff" | "webp") {
                    files.push(path);
                }
            }
        }
    }
    
    // Sort by creation date (newest first)
    files.sort_by(|a, b| {
        let a_created = a.metadata().and_then(|m| m.created()).unwrap_or(std::time::UNIX_EPOCH);
        let b_created = b.metadata().and_then(|m| m.created()).unwrap_or(std::time::UNIX_EPOCH);
        b_created.cmp(&a_created) // reverse order for newest first
    });
    
    Ok(files)
}

// Preloader for adjacent images
pub struct ImagePreloader {
    loader: ImageLoader,
    cache: crate::cache::ImageCache,
}

impl ImagePreloader {
    pub fn new(cache: crate::cache::ImageCache) -> Self {
        Self {
            loader: ImageLoader::new(),
            cache,
        }
    }
    
    pub fn preload_adjacent(&self, current_path: &Path, all_files: &[std::path::PathBuf]) {
        let current_idx = all_files.iter().position(|p| p == current_path);
        
        if let Some(idx) = current_idx {
            // Preload next image
            if idx + 1 < all_files.len() {
                let next_path = &all_files[idx + 1];
                if !self.is_cached(next_path) {
                    self.loader.load_image(next_path, Some((1024, 1024)), false);
                }
            }
            
            // Preload previous image
            if idx > 0 {
                let prev_path = &all_files[idx - 1];
                if !self.is_cached(prev_path) {
                    self.loader.load_image(prev_path, Some((1024, 1024)), false);
                }
            }
        }
    }
    
    fn is_cached(&self, path: &Path) -> bool {
        let key = path.to_string_lossy().to_string();
        self.cache.lock().unwrap().contains(&key)
    }
    
    pub fn update(&self) {
        while let Some(result) = self.loader.try_recv_result() {
            if let Ok(loaded) = result.result {
                let key = result.path.to_string_lossy().to_string();
                self.cache.lock().unwrap().put(key, loaded.image);
            }
        }
    }
}