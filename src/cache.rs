use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use egui::ColorImage;

#[derive(Clone)]
pub struct TimestampedImageCache {
    pub cache: Arc<Mutex<LruCache<String, Arc<ColorImage>>>>,
    timestamps: Arc<Mutex<HashMap<String, Instant>>>,
}

#[derive(Clone)]
pub struct TimestampedMetadataCache {
    pub cache: Arc<Mutex<LruCache<String, crate::metadata_simple::ImageMetadata>>>,
    timestamps: Arc<Mutex<HashMap<String, Instant>>>,
}

pub type ImageCache = TimestampedImageCache;
pub type MetadataCache = TimestampedMetadataCache;

impl TimestampedImageCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(
                LruCache::new(NonZeroUsize::new(capacity).expect("Cache capacity must be non-zero"))
            )),
            timestamps: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get(&self, key: &str) -> Option<Arc<ColorImage>> {
        self.cache.lock().unwrap().get(key).cloned()
    }

    pub fn put(&self, key: String, value: Arc<ColorImage>) {
        self.cache.lock().unwrap().put(key.clone(), value);
        self.timestamps.lock().unwrap().insert(key, Instant::now());
    }

    pub fn remove(&self, key: &str) {
        self.cache.lock().unwrap().pop(key);
        self.timestamps.lock().unwrap().remove(key);
    }

    pub fn clear(&self) {
        self.cache.lock().unwrap().clear();
        self.timestamps.lock().unwrap().clear();
    }

    pub fn cleanup_old_entries(&self, max_age: Duration) {
        let now = Instant::now();
        let mut keys_to_remove = Vec::new();

        {
            let timestamps = self.timestamps.lock().unwrap();
            for (key, timestamp) in timestamps.iter() {
                if now.duration_since(*timestamp) > max_age {
                    keys_to_remove.push(key.clone());
                }
            }
        }

        for key in keys_to_remove {
            self.remove(&key);
        }
    }

    pub fn len(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    pub fn contains(&self, key: &str) -> bool {
        self.cache.lock().unwrap().contains(key)
    }
}

impl TimestampedMetadataCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(
                LruCache::new(NonZeroUsize::new(capacity).expect("Cache capacity must be non-zero"))
            )),
            timestamps: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get(&self, key: &str) -> Option<crate::metadata_simple::ImageMetadata> {
        self.cache.lock().unwrap().get(key).cloned()
    }

    pub fn put(&self, key: String, value: crate::metadata_simple::ImageMetadata) {
        self.cache.lock().unwrap().put(key.clone(), value);
        self.timestamps.lock().unwrap().insert(key, Instant::now());
    }

    pub fn remove(&self, key: &str) {
        self.cache.lock().unwrap().pop(key);
        self.timestamps.lock().unwrap().remove(key);
    }

    pub fn clear(&self) {
        self.cache.lock().unwrap().clear();
        self.timestamps.lock().unwrap().clear();
    }

    pub fn cleanup_old_entries(&self, max_age: Duration) {
        let now = Instant::now();
        let mut keys_to_remove = Vec::new();

        {
            let timestamps = self.timestamps.lock().unwrap();
            for (key, timestamp) in timestamps.iter() {
                if now.duration_since(*timestamp) > max_age {
                    keys_to_remove.push(key.clone());
                }
            }
        }

        for key in keys_to_remove {
            self.remove(&key);
        }
    }

    pub fn len(&self) -> usize {
        self.cache.lock().unwrap().len()
    }
}

pub fn create_image_cache(capacity: usize) -> ImageCache {
    TimestampedImageCache::new(capacity)
}

pub fn create_metadata_cache(capacity: usize) -> MetadataCache {
    TimestampedMetadataCache::new(capacity)
}

#[derive(Clone)]
pub struct CompositionCache {
    pub cache: Arc<Mutex<LruCache<String, Vec<CompositionLine>>>>,
    timestamps: Arc<Mutex<HashMap<String, Instant>>>,
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
            )),
            timestamps: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get(&self, key: &str) -> Option<Vec<CompositionLine>> {
        self.cache.lock().unwrap().get(key).cloned()
    }

    pub fn put(&self, key: String, lines: Vec<CompositionLine>) {
        self.cache.lock().unwrap().put(key.clone(), lines);
        self.timestamps.lock().unwrap().insert(key, Instant::now());
    }

    pub fn remove(&self, key: &str) {
        self.cache.lock().unwrap().pop(key);
        self.timestamps.lock().unwrap().remove(key);
    }

    pub fn clear(&self) {
        self.cache.lock().unwrap().clear();
        self.timestamps.lock().unwrap().clear();
    }

    pub fn cleanup_old_entries(&self, max_age: Duration) {
        let now = Instant::now();
        let mut keys_to_remove = Vec::new();

        {
            let timestamps = self.timestamps.lock().unwrap();
            for (key, timestamp) in timestamps.iter() {
                if now.duration_since(*timestamp) > max_age {
                    keys_to_remove.push(key.clone());
                }
            }
        }

        for key in keys_to_remove {
            self.remove(&key);
        }
    }

    pub fn len(&self) -> usize {
        self.cache.lock().unwrap().len()
    }
}
