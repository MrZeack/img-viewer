//! Generic LRU cache with per-entry timestamps and optional memory budget.
use egui::ColorImage;
use lru::LruCache;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Default memory budget for cached full/preview images (512 MiB).
#[cfg(test)]
pub const DEFAULT_IMAGE_CACHE_MAX_BYTES: usize = 512 * 1024 * 1024;

struct CacheEntry<V> {
    value: V,
    last_access: Instant,
    bytes: usize,
}

struct CacheInner<V> {
    lru: LruCache<String, CacheEntry<V>>,
    total_bytes: usize,
    max_bytes: Option<usize>,
}

/// Thread-safe LRU cache with per-entry timestamps and optional byte budget.
pub struct TimestampedLruCache<V> {
    inner: Arc<Mutex<CacheInner<V>>>,
}

impl<V> Clone for TimestampedLruCache<V> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl<V> TimestampedLruCache<V> {
    pub fn new(capacity: usize) -> Self {
        Self::create(capacity, None)
    }

    pub fn with_max_bytes(capacity: usize, max_bytes: usize) -> Self {
        Self::create(capacity, Some(max_bytes.max(1)))
    }

    fn create(capacity: usize, max_bytes: Option<usize>) -> Self {
        let cap = NonZeroUsize::new(capacity.max(1)).expect("capacity clamped to >= 1");
        Self {
            inner: Arc::new(Mutex::new(CacheInner {
                lru: LruCache::new(cap),
                total_bytes: 0,
                max_bytes,
            })),
        }
    }

    pub fn get(&self, key: &str) -> Option<V>
    where
        V: Clone,
    {
        let mut inner = lock(&self.inner);
        let entry = inner.lru.get_mut(key)?;
        entry.last_access = Instant::now();
        Some(entry.value.clone())
    }

    pub fn put(&self, key: String, value: V) {
        self.put_weighted(key, value, 0);
    }

    pub fn put_weighted(&self, key: String, value: V, bytes: usize) {
        let mut inner = lock(&self.inner);
        if let Some(prev) = inner.lru.pop(&key) {
            inner.total_bytes = inner.total_bytes.saturating_sub(prev.bytes);
        }

        let cap = inner.lru.cap().get();
        while inner.lru.len() >= cap {
            if let Some((_, evicted)) = inner.lru.pop_lru() {
                inner.total_bytes = inner.total_bytes.saturating_sub(evicted.bytes);
            } else {
                break;
            }
        }

        if let Some(max_bytes) = inner.max_bytes {
            while !inner.lru.is_empty() && inner.total_bytes.saturating_add(bytes) > max_bytes {
                if let Some((_, evicted)) = inner.lru.pop_lru() {
                    inner.total_bytes = inner.total_bytes.saturating_sub(evicted.bytes);
                } else {
                    break;
                }
            }
        }

        inner.total_bytes = inner.total_bytes.saturating_add(bytes);
        inner.lru.put(
            key,
            CacheEntry {
                value,
                last_access: Instant::now(),
                bytes,
            },
        );
    }

    pub fn remove(&self, key: &str) -> Option<V> {
        let mut inner = lock(&self.inner);
        inner.lru.pop(key).map(|entry| {
            inner.total_bytes = inner.total_bytes.saturating_sub(entry.bytes);
            entry.value
        })
    }

    pub fn clear(&self) {
        let mut inner = lock(&self.inner);
        inner.lru.clear();
        inner.total_bytes = 0;
    }

    pub fn cleanup_old_entries(&self, max_age: Duration) {
        let now = Instant::now();
        let mut inner = lock(&self.inner);
        let expired: Vec<String> = inner
            .lru
            .iter()
            .filter(|(_, entry)| now.duration_since(entry.last_access) > max_age)
            .map(|(k, _)| k.clone())
            .collect();
        for key in expired {
            if let Some(entry) = inner.lru.pop(&key) {
                inner.total_bytes = inner.total_bytes.saturating_sub(entry.bytes);
            }
        }
    }

    pub fn retain_keys(&self, keep: &HashSet<String>) {
        let mut inner = lock(&self.inner);
        let drop_keys: Vec<String> = inner
            .lru
            .iter()
            .filter(|(k, _)| !keep.contains(*k))
            .map(|(k, _)| k.clone())
            .collect();
        for key in drop_keys {
            if let Some(entry) = inner.lru.pop(&key) {
                inner.total_bytes = inner.total_bytes.saturating_sub(entry.bytes);
            }
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn len(&self) -> usize {
        lock(&self.inner).lru.len()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn total_bytes(&self) -> usize {
        lock(&self.inner).total_bytes
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn contains(&self, key: &str) -> bool {
        lock(&self.inner).lru.contains(key)
    }
}

/// Cached decoded image together with the target resolution it was loaded for.
#[derive(Debug, Clone)]
pub struct CachedImage {
    pub image: Arc<ColorImage>,
    /// Maximum dimension requested (`None` = native full resolution).
    pub target_dim: Option<u32>,
}

impl CachedImage {
    pub fn max_side(&self) -> u32 {
        self.image.size[0].max(self.image.size[1]) as u32
    }

    /// True if this image is already at least as sharp as `requested_dim`.
    /// An image strictly smaller than its own `target_dim` was never downscaled and is already native.
    pub fn satisfies(&self, requested_dim: Option<u32>) -> bool {
        match (self.target_dim, requested_dim) {
            (None, _) => true,
            (Some(have), Some(want)) => have >= want || self.max_side() < have,
            (Some(have), None) => self.max_side() < have,
        }
    }
}

/// Image cache that tracks decoded pixel memory against a byte budget and avoids resolution downgrades.
#[derive(Clone)]
pub struct ImageCache {
    inner: TimestampedLruCache<CachedImage>,
}

impl ImageCache {
    #[cfg(test)]
    pub fn new(capacity: usize) -> Self {
        Self::with_max_bytes(capacity, DEFAULT_IMAGE_CACHE_MAX_BYTES)
    }

    pub fn with_max_bytes(capacity: usize, max_bytes: usize) -> Self {
        Self {
            inner: TimestampedLruCache::with_max_bytes(capacity, max_bytes),
        }
    }

    pub fn get(&self, key: &str) -> Option<Arc<ColorImage>> {
        self.inner.get(key).map(|entry| entry.image)
    }

    pub fn get_entry(&self, key: &str) -> Option<CachedImage> {
        self.inner.get(key)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn put(&self, key: String, value: Arc<ColorImage>) {
        self.put_if_better(key, value, None);
    }

    /// Inserts the image unless the cache already holds a strictly sharper version for `key`.
    pub fn put_if_better(
        &self,
        key: String,
        image: Arc<ColorImage>,
        target_dim: Option<u32>,
    ) -> bool {
        let incoming = CachedImage { image, target_dim };
        if let Some(existing) = self.inner.get(&key) {
            if existing.satisfies(target_dim) && !incoming.satisfies(existing.target_dim) {
                return false;
            }
        }
        let bytes = incoming.image.pixels.len() * std::mem::size_of::<egui::Color32>();
        self.inner.put_weighted(key, incoming, bytes);
        true
    }

    pub fn remove(&self, key: &str) {
        self.inner.remove(key);
    }

    pub fn cleanup_old_entries(&self, max_age: Duration) {
        self.inner.cleanup_old_entries(max_age);
    }

    pub fn retain_keys(&self, keep: &HashSet<String>) {
        self.inner.retain_keys(keep);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn contains(&self, key: &str) -> bool {
        self.inner.contains(key)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn total_bytes(&self) -> usize {
        self.inner.total_bytes()
    }
}

/// Cache for image metadata.
pub type MetadataCache = TimestampedLruCache<crate::metadata_simple::ImageMetadata>;

/// Cache for precomputed composition lines.
pub type CompositionCache = TimestampedLruCache<Vec<CompositionLine>>;

/// Represents a single line in a composition guide.
#[derive(Debug, Clone)]
pub struct CompositionLine {
    /// Starting position of the line
    pub start: egui::Pos2,
    /// Ending position of the line
    pub end: egui::Pos2,
    /// Stroke style for rendering the line
    pub stroke: egui::Stroke,
}

pub fn create_metadata_cache(capacity: usize) -> MetadataCache {
    MetadataCache::new(capacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_image(w: usize, h: usize) -> Arc<ColorImage> {
        Arc::new(ColorImage::new([w, h], egui::Color32::BLACK))
    }

    #[test]
    fn zero_capacity_clamps_to_one_without_panicking() {
        let cache: TimestampedLruCache<u32> = TimestampedLruCache::new(0);
        cache.put("a".into(), 1);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get("a"), Some(1));
        cache.put("b".into(), 2);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get("a"), None);
        assert_eq!(cache.get("b"), Some(2));
    }

    #[test]
    fn ttl_cleanup_removes_only_expired_entries() {
        let cache: TimestampedLruCache<u32> = TimestampedLruCache::new(4);
        cache.put("old".into(), 1);
        std::thread::sleep(Duration::from_millis(25));
        cache.put("fresh".into(), 2);

        cache.cleanup_old_entries(Duration::from_millis(15));
        assert_eq!(cache.get("old"), None);
        assert_eq!(cache.get("fresh"), Some(2));
    }

    #[test]
    fn image_cache_enforces_byte_budget_and_tracks_updates() {
        // Each 10x10 image = 400 bytes; budget 900 bytes fits 2 images.
        let cache = ImageCache::with_max_bytes(10, 900);
        cache.put("a".into(), dummy_image(10, 10));
        cache.put("b".into(), dummy_image(10, 10));
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.total_bytes(), 800);

        // Accessing "a" makes "b" LRU; inserting "c" should evict "b".
        assert!(cache.get("a").is_some());
        cache.put("c".into(), dummy_image(10, 10));
        assert_eq!(cache.len(), 2);
        assert!(cache.contains("a"));
        assert!(!cache.contains("b"));
        assert!(cache.contains("c"));
        assert_eq!(cache.total_bytes(), 800);

        // Replacing "c" with a larger single image (20x20 = 1600 bytes) evicts "a"
        // and still keeps "c" alone.
        cache.put("c".into(), dummy_image(20, 20));
        assert_eq!(cache.len(), 1);
        assert!(!cache.contains("a"));
        assert!(cache.contains("c"));
        assert_eq!(cache.total_bytes(), 1600);
    }

    #[test]
    fn retain_keys_evicts_unlisted_entries_and_updates_bytes() {
        let cache = ImageCache::with_max_bytes(10, 10_000);
        cache.put("a".into(), dummy_image(10, 10));
        cache.put("b".into(), dummy_image(10, 10));
        cache.put("c".into(), dummy_image(10, 10));
        let keep: HashSet<String> = ["b".into()].into_iter().collect();

        cache.retain_keys(&keep);
        assert_eq!(cache.len(), 1);
        assert!(cache.contains("b"));
        assert_eq!(cache.total_bytes(), 400);
    }

    #[test]
    fn cached_image_satisfies_checks_resolution_and_avoids_downgrade() {
        let preview = CachedImage {
            image: dummy_image(1024, 680),
            target_dim: Some(1024),
        };
        assert!(preview.satisfies(Some(1024)));
        assert!(!preview.satisfies(Some(2048)));
        assert!(!preview.satisfies(None));

        // Image smaller than requested target was not downscaled, so it satisfies full-res.
        let small = CachedImage {
            image: dummy_image(600, 400),
            target_dim: Some(1024),
        };
        assert!(small.satisfies(Some(2048)));
        assert!(small.satisfies(None));

        let cache = ImageCache::with_max_bytes(10, 100 * 1024 * 1024);
        cache.put_if_better("p".into(), dummy_image(2048, 1365), Some(2048));
        // A late 1024px preview must not overwrite the 2048px entry.
        cache.put_if_better("p".into(), dummy_image(1024, 680), Some(1024));
        let entry = cache.get_entry("p").unwrap();
        assert_eq!(entry.target_dim, Some(2048));
        assert_eq!(entry.image.size, [2048, 1365]);
    }
}
