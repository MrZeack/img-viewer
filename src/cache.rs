use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use egui::ColorImage;

pub type ImageCache = Arc<Mutex<LruCache<String, Arc<ColorImage>>>>;
pub type MetadataCache = Arc<Mutex<LruCache<String, crate::metadata_simple::ImageMetadata>>>;

pub fn create_image_cache(capacity: usize) -> ImageCache {
    Arc::new(Mutex::new(
        LruCache::new(NonZeroUsize::new(capacity).expect("Cache capacity must be non-zero"))
    ))
}

pub fn create_metadata_cache(capacity: usize) -> MetadataCache {
    Arc::new(Mutex::new(
        LruCache::new(NonZeroUsize::new(capacity).expect("Cache capacity must be non-zero"))
    ))
}

#[derive(Clone)]
pub struct CompositionCache {
    cache: Arc<Mutex<LruCache<String, Vec<CompositionLine>>>>
}

#[derive(Debug, Clone)]
pub struct CompositionLine {
    pub start: egui::Pos2,
    pub end: egui::Pos2,
    pub stroke: egui::Stroke,
}

impl CompositionCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(
                LruCache::new(NonZeroUsize::new(capacity).expect("Cache capacity must be non-zero"))
            ))
        }
    }

    pub fn get(&self, key: &str) -> Option<Vec<CompositionLine>> {
        self.cache.lock().unwrap().get(key).cloned()
    }

    pub fn put(&self, key: String, lines: Vec<CompositionLine>) {
        self.cache.lock().unwrap().put(key, lines);
    }

    pub fn clear(&self) {
        self.cache.lock().unwrap().clear();
    }
}
