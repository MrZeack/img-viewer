//! Caching implementation for images, metadata, and composition lines
use egui::ColorImage;
use lru::LruCache;
use std::collections::HashMap;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Cache for image data with timestamp tracking for cleanup
#[derive(Clone)]
pub struct TimestampedImageCache {
    /// The main LRU cache holding image data
    pub cache: Arc<Mutex<LruCache<String, Arc<ColorImage>>>>,
    /// Map of keys to timestamps for cleanup operations
    timestamps: Arc<Mutex<HashMap<String, Instant>>>,
}

/// Cache for image metadata with timestamp tracking for cleanup
#[derive(Clone)]
pub struct TimestampedMetadataCache {
    /// The main LRU cache holding metadata
    pub cache: Arc<Mutex<LruCache<String, crate::metadata_simple::ImageMetadata>>>,
    /// Map of keys to timestamps for cleanup operations
    timestamps: Arc<Mutex<HashMap<String, Instant>>>,
}

/// Type alias for the image cache
pub type ImageCache = TimestampedImageCache;
/// Type alias for the metadata cache
pub type MetadataCache = TimestampedMetadataCache;

impl TimestampedImageCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(capacity).expect("Cache capacity must be non-zero"),
            ))),
            timestamps: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get(&self, key: &str) -> Option<Arc<ColorImage>> {
        let value = self.cache.lock().unwrap().get(key).cloned();
        if value.is_some() {
            // refresh last-access timestamp on successful hit
            self.timestamps
                .lock()
                .unwrap()
                .insert(key.to_string(), Instant::now());
        }
        value
    }

    pub fn put(&self, key: String, value: Arc<ColorImage>) {
        {
            let mut cache = self.cache.lock().unwrap();
            cache.put(key.clone(), value);
        }
        {
            let mut ts = self.timestamps.lock().unwrap();
            ts.insert(key, Instant::now());
        }
        self.reconcile_timestamps_with_cache();
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

impl TimestampedImageCache {
    fn reconcile_timestamps_with_cache(&self) {
        let keys: HashSet<String> = {
            let cache = self.cache.lock().unwrap();
            cache.iter().map(|(k, _)| k.clone()).collect()
        };
        let mut ts = self.timestamps.lock().unwrap();
        ts.retain(|k, _| keys.contains(k));
    }
}

impl TimestampedMetadataCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(capacity).expect("Cache capacity must be non-zero"),
            ))),
            timestamps: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get(&self, key: &str) -> Option<crate::metadata_simple::ImageMetadata> {
        let value = self.cache.lock().unwrap().get(key).cloned();
        if value.is_some() {
            self.timestamps
                .lock()
                .unwrap()
                .insert(key.to_string(), Instant::now());
        }
        value
    }

    pub fn put(&self, key: String, value: crate::metadata_simple::ImageMetadata) {
        {
            let mut cache = self.cache.lock().unwrap();
            cache.put(key.clone(), value);
        }
        {
            let mut ts = self.timestamps.lock().unwrap();
            ts.insert(key, Instant::now());
        }
        self.reconcile_timestamps_with_cache();
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

impl TimestampedMetadataCache {
    fn reconcile_timestamps_with_cache(&self) {
        let keys: HashSet<String> = {
            let cache = self.cache.lock().unwrap();
            cache.iter().map(|(k, _)| k.clone()).collect()
        };
        let mut ts = self.timestamps.lock().unwrap();
        ts.retain(|k, _| keys.contains(k));
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

/// Represents a single line in a composition guide
#[derive(Debug, Clone)]
pub struct CompositionLine {
    /// Starting position of the line
    pub start: egui::Pos2,
    /// Ending position of the line
    pub end: egui::Pos2,
    /// Stroke style for rendering the line
    pub stroke: egui::Stroke,
}

impl CompositionCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(capacity).expect("Cache capacity must be non-zero"),
            ))),
            timestamps: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get(&self, key: &str) -> Option<Vec<CompositionLine>> {
        let value = self.cache.lock().unwrap().get(key).cloned();
        if value.is_some() {
            self.timestamps
                .lock()
                .unwrap()
                .insert(key.to_string(), Instant::now());
        }
        value
    }

    pub fn put(&self, key: String, lines: Vec<CompositionLine>) {
        {
            let mut cache = self.cache.lock().unwrap();
            cache.put(key.clone(), lines);
        }
        {
            let mut ts = self.timestamps.lock().unwrap();
            ts.insert(key, Instant::now());
        }
        self.reconcile_timestamps_with_cache();
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

impl CompositionCache {
    fn reconcile_timestamps_with_cache(&self) {
        let keys: HashSet<String> = {
            let cache = self.cache.lock().unwrap();
            cache.iter().map(|(k, _)| k.clone()).collect()
        };
        let mut ts = self.timestamps.lock().unwrap();
        ts.retain(|k, _| keys.contains(k));
    }
}
