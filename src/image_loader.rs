use rayon::prelude::*;
use anyhow::Result;
use egui::ColorImage;
use image::{DynamicImage, ImageFormat};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use crate::event::{EventBus, ViewerEvent};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct LoadedImage {
    pub image: ColorImage,
}

pub struct ImageLoader {
    sender: mpsc::Sender<LoadRequest>,
}

#[derive(Debug)]
struct LoadRequest {
    path: std::path::PathBuf,
    target_size: Option<(u32, u32)>,
    high_quality: bool,
}

impl ImageLoader {
    pub fn new(num_threads: usize, event_bus: EventBus) -> Self {
        let (tx, rx) = mpsc::channel::<LoadRequest>();
        
        thread::spawn(move || {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .build()
                .expect("Failed to build image loader thread pool");

            pool.install(|| {
                rx.into_iter().par_bridge().for_each(|request| {
                    let path = request.path.clone();
                    if let Ok(loaded_image) = load_image_sync(request) {
                        let image_arc = Arc::new(loaded_image.image);
                        event_bus.publish(ViewerEvent::ImageLoaded(path, image_arc));
                    }
                });
            });
        });
        
        Self {
            sender: tx,
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
    
    let color_image = dynamic_image_to_color_image(&img);
    
    Ok(LoadedImage {
        image: color_image,
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
    
    // Sort by modification date (oldest first)
    files.sort_by_key(|a| a.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH));
    
    Ok(files)
}

// Preloader for adjacent images
pub struct ImagePreloader {
    loader: ImageLoader,
    cache: crate::cache::ImageCache,
    preload_distance: usize,
}

impl ImagePreloader {
    pub fn new(cache: crate::cache::ImageCache, preload_distance: usize, event_bus: EventBus) -> Self {
        Self {
            loader: ImageLoader::new(2, event_bus), // Preloader uses a small number of threads
            cache,
            preload_distance,
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
                        self.loader.load_image(next_path, Some((1024, 1024)), false);
                    }
                }
            }
            
            // Preload previous images
            for i in 1..=self.preload_distance {
                if idx >= i {
                    let prev_path = &all_files[idx - i];
                    if !self.is_cached(prev_path) {
                        self.loader.load_image(prev_path, Some((1024, 1024)), false);
                    }
                }
            }
        }
    }
    
    fn is_cached(&self, path: &Path) -> bool {
        let key = path.to_string_lossy().to_string();
        self.cache.lock().unwrap().contains(&key)
    }
}