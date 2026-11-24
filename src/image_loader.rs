//! Asynchronous image loading with prioritized work queue, scaling and caching.
use crate::errors::AppError;
use crate::event::{EventBus, ViewerEvent};
use egui::ColorImage;
use image::{DynamicImage, ImageFormat};
use libheif_rs::{HeifContext, LibHeif};
use std::borrow::Cow;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

const MAX_PRELOAD_QUEUE: usize = 16;

/// Represents a successfully loaded image.
#[derive(Debug, Clone)]
pub struct LoadedImage {
    /// The loaded image as an egui ColorImage.
    pub image: ColorImage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoadPriority {
    Current,
    Preload,
}

#[derive(Debug, Clone)]
struct LoadRequest {
    path: PathBuf,
    target_size: Option<(u32, u32)>,
    high_quality: bool,
}

type RequestKey = (PathBuf, Option<(u32, u32)>);

#[derive(Default)]
struct QueueState {
    current: Vec<LoadRequest>,
    preload: VecDeque<LoadRequest>,
    in_flight: HashSet<RequestKey>,
}

impl QueueState {
    fn key(req: &LoadRequest) -> RequestKey {
        (req.path.clone(), req.target_size)
    }

    fn contains(&self, key: &RequestKey) -> bool {
        self.in_flight.contains(key)
            || self.current.iter().any(|r| Self::key(r) == *key)
            || self.preload.iter().any(|r| Self::key(r) == *key)
    }

    fn enqueue(&mut self, req: LoadRequest, priority: LoadPriority) -> bool {
        let key = Self::key(&req);
        if self.contains(&key) {
            return false;
        }
        match priority {
            LoadPriority::Current => {
                // Keep only current-path requests (e.g. normal + HQ upgrade) and drop obsolete images.
                self.current.retain(|r| r.path == req.path);
                self.current.push(req);
            }
            LoadPriority::Preload => {
                if self.preload.len() >= MAX_PRELOAD_QUEUE {
                    self.preload.pop_front();
                }
                self.preload.push_back(req);
            }
        }
        true
    }

    fn pop(&mut self) -> Option<LoadRequest> {
        let req = self.current.pop().or_else(|| self.preload.pop_front())?;
        self.in_flight.insert(Self::key(&req));
        Some(req)
    }

    /// Descarta las precargas en cola (no las que ya corren): pertenecen a una posición anterior.
    fn cancel_preloads(&mut self) {
        self.preload.clear();
    }

    fn finish(&mut self, key: &RequestKey) {
        self.in_flight.remove(key);
    }
}

struct LoaderShared {
    queue: Mutex<QueueState>,
    work: Condvar,
    stop: AtomicBool,
    event_bus: EventBus,
}

struct LoaderInner {
    shared: Arc<LoaderShared>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl Drop for LoaderInner {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.work.notify_all();
        let mut workers = lock(&self.workers);
        for handle in workers.drain(..) {
            let _ = handle.join();
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone)]
pub struct ImageLoader {
    inner: Arc<LoaderInner>,
}

impl ImageLoader {
    pub fn new(num_threads: usize, event_bus: EventBus) -> Self {
        let shared = Arc::new(LoaderShared {
            queue: Mutex::new(QueueState::default()),
            work: Condvar::new(),
            stop: AtomicBool::new(false),
            event_bus,
        });

        let threads = num_threads.max(1);
        let workers = (0..threads)
            .map(|n| {
                let shared = Arc::clone(&shared);
                thread::Builder::new()
                    .name(format!("img-load-{n}"))
                    .spawn(move || worker_loop(&shared))
                    .expect("Failed to spawn image loader worker")
            })
            .collect();

        Self {
            inner: Arc::new(LoaderInner {
                shared,
                workers: Mutex::new(workers),
            }),
        }
    }

    pub fn load_image(&self, path: &Path, target_size: Option<(u32, u32)>, high_quality: bool) {
        self.enqueue(path, target_size, high_quality, LoadPriority::Current);
    }

    /// Descarta las precargas pendientes para que las de la nueva posición vayan primero.
    pub fn cancel_preloads(&self) {
        lock(&self.inner.shared.queue).cancel_preloads();
    }

    pub fn preload_image(&self, path: &Path, target_size: Option<(u32, u32)>) {
        self.enqueue(path, target_size, false, LoadPriority::Preload);
    }

    fn enqueue(
        &self,
        path: &Path,
        target_size: Option<(u32, u32)>,
        high_quality: bool,
        priority: LoadPriority,
    ) {
        let req = LoadRequest {
            path: path.to_path_buf(),
            target_size,
            high_quality,
        };
        let inserted = lock(&self.inner.shared.queue).enqueue(req, priority);
        if inserted {
            self.inner.shared.work.notify_one();
        }
    }
}

fn worker_loop(shared: &LoaderShared) {
    loop {
        let request = {
            let mut queue = lock(&shared.queue);
            loop {
                if shared.stop.load(Ordering::SeqCst) {
                    return;
                }
                if let Some(req) = queue.pop() {
                    break req;
                }
                queue = shared.work.wait(queue).unwrap_or_else(|e| e.into_inner());
            }
        };

        let key = QueueState::key(&request);
        let path = request.path.clone();
        let target_dim = request.target_size.map(|(w, h)| w.max(h));
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| load_image_sync(request)))
                .unwrap_or_else(|_| {
                    Err(AppError::ImageLoadError("panic decoding image".to_string()))
                });

        match result {
            Ok(loaded) => {
                shared.event_bus.publish(ViewerEvent::ImageLoaded {
                    path,
                    image: Arc::new(loaded.image),
                    target_dim,
                });
            }
            Err(err) => {
                shared.event_bus.publish(ViewerEvent::ImageLoadFailed {
                    path,
                    error: err.to_string(),
                });
            }
        }

        lock(&shared.queue).finish(&key);
    }
}

fn load_image_sync(request: LoadRequest) -> Result<LoadedImage, AppError> {
    let ext = request
        .path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if ext == "arw" {
        return load_arw(&request.path, request.target_size, request.high_quality);
    }

    let data = std::fs::read(&request.path).map_err(|e| {
        AppError::FileSystemError(format!(
            "Failed to read image file {}: {}",
            request.path.display(),
            e
        ))
    })?;

    if ext == "cr3" {
        return load_raw_preview(&data, request.target_size, request.high_quality);
    } else if ext == "hif" || ext == "heic" {
        return load_hif(&data, request.target_size, request.high_quality);
    }

    let format = image::guess_format(&data).map_err(|e| {
        AppError::ImageLoadError(format!(
            "Failed to guess image format for {}: {}",
            request.path.display(),
            e
        ))
    })?;

    let img = if format == ImageFormat::Jpeg {
        load_jpeg_optimized(&data, request.target_size, request.high_quality).map_err(|e| {
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
        resize_to_fit(&mut img, request.target_size, request.high_quality);
        img
    };

    Ok(LoadedImage {
        image: dynamic_image_to_color_image(&img),
    })
}

fn resize_filter(high_quality: bool) -> image::imageops::FilterType {
    if high_quality {
        image::imageops::FilterType::Lanczos3
    } else {
        image::imageops::FilterType::Triangle
    }
}

fn resize_to_fit(img: &mut DynamicImage, target_size: Option<(u32, u32)>, high_quality: bool) {
    if let Some((w, h)) = target_size {
        if img.width() > w || img.height() > h {
            *img = img.resize(w, h, resize_filter(high_quality));
        }
    }
}

fn load_jpeg_optimized(
    data: &[u8],
    target_size: Option<(u32, u32)>,
    high_quality: bool,
) -> Result<DynamicImage, AppError> {
    if let Some((w, h)) = target_size {
        if let Ok(rgb) = crate::thumbnails::generate::decode_jpeg_scaled(data, w.max(h)) {
            let mut img = DynamicImage::ImageRgb8(rgb);
            resize_to_fit(&mut img, Some((w, h)), high_quality);
            return Ok(img);
        }
    }

    let mut img = image::load_from_memory_with_format(data, ImageFormat::Jpeg)
        .map_err(|e| AppError::ImageLoadError(format!("Failed to load JPEG: {}", e)))?;
    resize_to_fit(&mut img, target_size, high_quality);
    Ok(img)
}

/// ARW: demosaica los datos del sensor a resolución completa, sin curva ni estilo de la cámara.
/// Si quickraw no soporta el modelo, usa el JPEG embebido más grande.
fn load_arw(
    path: &Path,
    target_size: Option<(u32, u32)>,
    high_quality: bool,
) -> Result<LoadedImage, AppError> {
    if let Some(mut img) = decode_sensor_path(path) {
        resize_to_fit(&mut img, target_size, high_quality);
        return Ok(LoadedImage {
            image: dynamic_image_to_color_image(&img),
        });
    }

    let data = std::fs::read(path).map_err(|e| {
        AppError::FileSystemError(format!("Failed to read ARW file {}: {}", path.display(), e))
    })?;
    let largest = crate::thumbnails::generate::bounded_preview_jpeg(&data, data.len());
    match largest {
        Some(jpg) => decode_and_fit(jpg, target_size, high_quality),
        None => load_raw_preview(&data, target_size, high_quality),
    }
}

/// Demosaico lineal con quickraw leyendo directo desde ruta, sin clonar el buffer entero.
fn decode_sensor_path(path: &Path) -> Option<DynamicImage> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() < 16 {
        return None;
    }
    let path_str = path.to_str()?.to_owned();
    std::panic::catch_unwind(move || {
        use quickraw::{data, DemosaicingMethod, Export, Input, Output, OutputType};
        let output = Output::new(
            DemosaicingMethod::Linear,
            data::XYZ2SRGB,
            data::GAMMA_SRGB,
            OutputType::Raw8,
            true,
            false,
        );
        let export = Export::new(Input::ByFile(&path_str), output).ok()?;
        let (pixels, w, h) = export.export_8bit_image();
        image::RgbImage::from_raw(w as u32, h as u32, pixels).map(DynamicImage::ImageRgb8)
    })
    .ok()
    .flatten()
}

fn decode_and_fit(
    jpg: &[u8],
    target_size: Option<(u32, u32)>,
    high_quality: bool,
) -> Result<LoadedImage, AppError> {
    let img = load_jpeg_optimized(jpg, target_size, high_quality).map_err(|e| {
        AppError::ImageLoadError(format!("Failed to load extracted RAW preview: {}", e))
    })?;
    Ok(LoadedImage {
        image: dynamic_image_to_color_image(&img),
    })
}

fn load_raw_preview(
    data: &[u8],
    target_size: Option<(u32, u32)>,
    high_quality: bool,
) -> Result<LoadedImage, AppError> {
    let jpg_data = match crate::cr3::preview_jpeg(data) {
        Some(jpg) => Cow::Borrowed(jpg),
        None => match safe_export_thumbnail(data) {
            Some((data, _orientation)) => Cow::Owned(data),
            None => Cow::Owned(extract_largest_jpeg(data).ok_or_else(|| {
                AppError::ImageLoadError(
                    "Failed to extract RAW preview (quickraw and manual fallback failed)"
                        .to_string(),
                )
            })?),
        },
    };

    decode_and_fit(&jpg_data, target_size, high_quality)
}

/// Calls `quickraw::Export::export_thumbnail_data` while guarding against panics.
///
/// quickraw/quickexif index slices without bounds checks and panic on empty,
/// truncated or unsupported files. A panic on a loader thread would otherwise
/// bring down the whole app, so treat any panic as "no thumbnail".
pub(crate) fn safe_export_thumbnail(data: &[u8]) -> Option<(Vec<u8>, u32)> {
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
    crate::thumbnails::generate::bounded_preview_jpeg(data, data.len()).map(|s| s.to_vec())
}

fn load_hif(
    data: &[u8],
    target_size: Option<(u32, u32)>,
    high_quality: bool,
) -> Result<LoadedImage, AppError> {
    let lib_heif = LibHeif::new();
    let ctx = HeifContext::read_from_bytes(data)
        .map_err(|e| AppError::ImageLoadError(format!("Failed to read HEIF data: {}", e)))?;

    let handle = ctx.primary_image_handle().map_err(|e| {
        AppError::ImageLoadError(format!("Failed to get primary image handle: {}", e))
    })?;

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
    let raw = interleaved_plane.data;
    let row_bytes = width as usize * 3;

    let needs_resize = target_size.is_some_and(|(w, h)| width > w || height > h);
    if !needs_resize {
        let mut pixels = Vec::with_capacity((width as usize) * (height as usize));
        for y in 0..height as usize {
            let start = y * stride;
            let row = raw.get(start..start + row_bytes).ok_or_else(|| {
                AppError::ImageLoadError("Truncated HEIF plane buffer".to_string())
            })?;
            for &[r, g, b] in row.as_chunks::<3>().0 {
                pixels.push(egui::Color32::from_rgb(r, g, b));
            }
        }
        return Ok(LoadedImage {
            image: ColorImage {
                size: [width as usize, height as usize],
                pixels,
            },
        });
    }

    let mut buffer = Vec::with_capacity(row_bytes * height as usize);
    for y in 0..height as usize {
        let start = y * stride;
        let row = raw
            .get(start..start + row_bytes)
            .ok_or_else(|| AppError::ImageLoadError("Truncated HEIF plane buffer".to_string()))?;
        buffer.extend_from_slice(row);
    }

    let rgb = image::RgbImage::from_raw(width, height, buffer).ok_or_else(|| {
        AppError::ImageLoadError("Failed to create image buffer from HEIF data".to_string())
    })?;
    let mut dynamic_image = DynamicImage::ImageRgb8(rgb);
    resize_to_fit(&mut dynamic_image, target_size, high_quality);
    Ok(LoadedImage {
        image: dynamic_image_to_color_image(&dynamic_image),
    })
}

fn dynamic_image_to_color_image(img: &DynamicImage) -> ColorImage {
    match img {
        DynamicImage::ImageRgb8(rgb) => {
            ColorImage::from_rgb([rgb.width() as usize, rgb.height() as usize], rgb.as_raw())
        }
        DynamicImage::ImageRgba8(rgba) => ColorImage::from_rgba_unmultiplied(
            [rgba.width() as usize, rgba.height() as usize],
            rgba.as_raw(),
        ),
        _ => {
            let rgb = img.to_rgb8();
            ColorImage::from_rgb([rgb.width() as usize, rgb.height() as usize], rgb.as_raw())
        }
    }
}

/// Archivos de imagen de `dir` con su fecha de modificación, sin ordenar.
/// `found` cuenta los encontrados, para mostrar progreso mientras se escanea.
pub fn scan_image_files(
    dir: &Path,
    found: Option<&std::sync::atomic::AtomicUsize>,
) -> Result<Vec<(PathBuf, std::time::SystemTime)>, AppError> {
    let mut files = Vec::new();
    if !dir.is_dir() {
        return Ok(files);
    }
    let dir_entries = std::fs::read_dir(dir).map_err(|e| {
        AppError::FileSystemError(format!("Failed to read directory {}: {}", dir.display(), e))
    })?;
    for entry in dir_entries {
        let entry = entry.map_err(|e| {
            AppError::FileSystemError(format!("Failed to read directory entry: {}", e))
        })?;
        let path = entry.path();
        let supported = path.extension().is_some_and(|ext| {
            matches!(
                ext.to_string_lossy().to_lowercase().as_str(),
                "jpg"
                    | "jpeg"
                    | "png"
                    | "bmp"
                    | "tif"
                    | "tiff"
                    | "webp"
                    | "hif"
                    | "heic"
                    | "arw"
                    | "cr3"
            )
        });
        if supported {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            files.push((path, mtime));
            if let Some(found) = found {
                found.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
    Ok(files)
}

/// Archivos de imagen ordenados por fecha (`newest_first` invierte el orden). Para tests y benchmarks;
/// la app usa `scan_image_files` y ordena según `SortOrder`.
#[cfg(test)]
pub fn get_image_files(dir: &Path, newest_first: bool) -> Result<Vec<PathBuf>, AppError> {
    let mut files = scan_image_files(dir, None)?;
    files.sort_by_key(|(_, mtime)| *mtime);
    if newest_first {
        files.reverse();
    }
    Ok(files.into_iter().map(|(path, _)| path).collect())
}

// Preloader for adjacent images sharing the main ImageLoader priority pool.
pub struct ImagePreloader {
    loader: ImageLoader,
    cache: crate::cache::ImageCache,
    preload_distance: usize,
    preview_size: u32,
}

impl ImagePreloader {
    pub fn new(
        loader: ImageLoader,
        cache: crate::cache::ImageCache,
        preload_distance: usize,
        preview_size: u32,
    ) -> Self {
        Self {
            loader,
            cache,
            preload_distance,
            preview_size,
        }
    }

    pub fn preload_adjacent(&self, current_path: &Path, all_files: &[PathBuf]) {
        let Some(idx) = all_files.iter().position(|p| p == current_path) else {
            return;
        };

        self.loader.cancel_preloads();
        for i in 1..=self.preload_distance {
            if let Some(next_path) = all_files.get(idx + i) {
                self.maybe_preload(next_path);
            }
            if idx >= i {
                self.maybe_preload(&all_files[idx - i]);
            }
        }
    }

    fn maybe_preload(&self, path: &Path) {
        let key = path.to_string_lossy();
        let already_good = self
            .cache
            .get_entry(&key)
            .is_some_and(|e| e.satisfies(Some(self.preview_size)));
        if !already_good {
            self.loader
                .preload_image(path, Some((self.preview_size, self.preview_size)));
        }
    }
}

#[cfg(test)]
mod tests {
    fn req(name: &str) -> LoadRequest {
        LoadRequest {
            path: PathBuf::from(name),
            target_size: Some((100, 100)),
            high_quality: false,
        }
    }

    #[test]
    fn cancel_preloads_drops_queued_but_keeps_current_and_in_flight() {
        let mut q = QueueState::default();
        q.enqueue(req("/cur"), LoadPriority::Current);
        q.enqueue(req("/old1"), LoadPriority::Preload);
        q.enqueue(req("/old2"), LoadPriority::Preload);
        let running = q.pop().unwrap();
        assert_eq!(running.path, PathBuf::from("/cur"));
        q.cancel_preloads();
        assert!(q.pop().is_none(), "las precargas obsoletas se descartan");
        assert!(
            q.contains(&QueueState::key(&running)),
            "la que corre sigue en vuelo"
        );
    }

    use super::*;
    use std::fs::{self, File};
    use std::sync::mpsc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir() -> PathBuf {
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
            "jpg", "jpeg", "png", "bmp", "tif", "tiff", "webp", "hif", "heic", "arw", "cr3",
        ];

        for ext in &extensions {
            let file_path = dir.join(format!("test.{}", ext));
            File::create(file_path).unwrap();
        }

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

    #[test]
    fn scan_image_files_ignores_directories_and_accepts_tif() {
        let dir = unique_temp_dir();
        fs::create_dir_all(dir.join("folder.jpg")).unwrap();
        File::create(dir.join("photo.tif")).unwrap();
        File::create(dir.join("real.JPG")).unwrap();

        let files = get_image_files(&dir, false).unwrap();
        let mut names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(names, vec!["photo.tif", "real.JPG"]);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn jpeg_respects_target_size() {
        let dir = unique_temp_dir();
        let path = dir.join("big.jpg");
        let img = image::RgbImage::from_fn(1600, 1200, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 64])
        });
        let jpeg = crate::thumbnails::generate::encode_jpeg(&img).unwrap();
        fs::write(&path, jpeg).unwrap();

        let loaded = load_image_sync(LoadRequest {
            path,
            target_size: Some((400, 400)),
            high_quality: true,
        })
        .unwrap();
        assert!(
            loaded.image.size[0].max(loaded.image.size[1]) <= 400,
            "got {:?}",
            loaded.image.size
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hif_respects_target_size() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("photos")
            .join("IMG_8382.HIF");
        let loaded = load_image_sync(LoadRequest {
            path,
            target_size: Some((600, 600)),
            high_quality: false,
        })
        .unwrap();
        assert!(
            loaded.image.size[0].max(loaded.image.size[1]) <= 600,
            "got {:?}",
            loaded.image.size
        );
    }

    #[test]
    fn failed_load_emits_error_event() {
        let (tx, rx) = mpsc::channel();
        let bus = EventBus::new(tx, None);
        let loader = ImageLoader::new(1, bus);
        let missing = PathBuf::from("/definitely/missing/img_viewer_test.jpg");
        loader.load_image(&missing, Some((512, 512)), true);

        let event = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("expected failure event");
        match event {
            ViewerEvent::ImageLoadFailed { path, error } => {
                assert_eq!(path, missing);
                assert!(!error.is_empty());
            }
            other => panic!("expected ImageLoadFailed, got {other:?}"),
        }
    }

    #[test]
    fn queue_prioritizes_current_lifo_and_allows_hq_upgrade() {
        let mut q = QueueState::default();
        let a = PathBuf::from("/a.jpg");
        let b = PathBuf::from("/b.jpg");

        // Preload queued first, then current requests for a and b: b supersedes a and beats preload.
        assert!(q.enqueue(
            LoadRequest {
                path: a.clone(),
                target_size: Some((1024, 1024)),
                high_quality: false,
            },
            LoadPriority::Preload,
        ));
        assert!(q.enqueue(
            LoadRequest {
                path: a.clone(),
                target_size: Some((2048, 2048)),
                high_quality: true,
            },
            LoadPriority::Current,
        ));
        // Requesting HQ (None) for the same path while 2048 is queued is NOT deduplicated away.
        assert!(q.enqueue(
            LoadRequest {
                path: a.clone(),
                target_size: None,
                high_quality: true,
            },
            LoadPriority::Current,
        ));
        assert_eq!(q.pop().unwrap().target_size, None);

        // Moving to image b drops stale current requests for a.
        assert!(q.enqueue(
            LoadRequest {
                path: b.clone(),
                target_size: Some((2048, 2048)),
                high_quality: true,
            },
            LoadPriority::Current,
        ));
        assert_eq!(q.pop().unwrap().path, b);
        assert_eq!(q.pop().unwrap().path, a); // preload still served when idle
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

#[cfg(test)]
mod raw_tests {
    use super::*;

    fn sample(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("photos")
            .join(name)
    }

    fn load(path: PathBuf, target_size: Option<(u32, u32)>) -> ColorImage {
        load_image_sync(LoadRequest {
            path,
            target_size,
            high_quality: true,
        })
        .unwrap()
        .image
    }

    #[test]
    fn garbage_sensor_file_does_not_panic() {
        let dir = std::env::temp_dir().join(format!("ivt-sensor-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (empty, junk) = (dir.join("empty.arw"), dir.join("junk.arw"));
        std::fs::write(&empty, b"").unwrap();
        std::fs::write(&junk, vec![0xFF; 4096]).unwrap();
        assert!(decode_sensor_path(&empty).is_none());
        assert!(decode_sensor_path(&junk).is_none());
        assert!(decode_sensor_path(&dir.join("missing.arw")).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unsupported_arw_model_uses_largest_embedded_jpeg() {
        // ILCE-6700: quickraw no lo decodifica; trae JPEG de 1616×1080 y otro de 6192×4128.
        let img = load(sample("_SON2447.ARW"), None);
        assert_eq!(img.size[0].max(img.size[1]), 6192);
    }

    #[test]
    fn arw_respects_target_size() {
        let img = load(sample("_SON2447.ARW"), Some((1000, 1000)));
        assert!(img.size[0].max(img.size[1]) <= 1000);
    }

    /// ARW de un modelo soportado (p. ej. ILCE-6000, cuyo JPEG embebido es de 1616 px):
    /// `IMG_BENCH_DIR=<carpeta> cargo test --release arw_sensor_full_resolution -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn arw_sensor_full_resolution() {
        let dir = PathBuf::from(std::env::var("IMG_BENCH_DIR").expect("IMG_BENCH_DIR"));
        let path = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("arw")))
            .expect("sin ARW");
        let start = std::time::Instant::now();
        let img = load(path.clone(), None);
        println!(
            "{}: {:?} en {:?}",
            path.display(),
            img.size,
            start.elapsed()
        );
        assert!(img.size[0].max(img.size[1]) > 4000);
    }
}
