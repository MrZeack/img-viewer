use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::cache::{CompositionCache, ImageCache, MetadataCache};
use crate::composition::CompositionType;
use crate::image_loader::{ImageLoader, ImagePreloader};
use crate::metadata_loader::MetadataLoader;
use crate::metadata_simple::ImageMetadata;
use crate::event::ViewerEvent;
use std::sync::mpsc;

pub struct ImageViewerApp {
    // Core state
    pub(crate) current_dir: PathBuf,
    pub(crate) image_files: Vec<PathBuf>,
    pub(crate) current_index: usize,
    
    // Image display
    pub(crate) current_image: Option<egui::TextureHandle>,
    pub(crate) zoom_factor: f32,
    pub(crate) image_position: egui::Vec2,
    
    // Caches
    pub(crate) image_cache: ImageCache,
    pub(crate) metadata_cache: MetadataCache,
    pub(crate) composition_cache: CompositionCache,
    
    // Image loading
    pub(crate) image_loader: ImageLoader,
    pub(crate) preloader: ImagePreloader,
    pub(crate) loading_requests: HashMap<PathBuf, Instant>,
    
    // Metadata
    pub(crate) current_metadata: Option<ImageMetadata>,
    pub(crate) metadata_loader: MetadataLoader,
    
    // Composition lines
    pub(crate) show_composition: bool,
    pub(crate) composition_type: CompositionType,
    
    // UI state
    pub(crate) show_sidebar: bool,
    pub(crate) sidebar_width: f32,
    pub(crate) status_message: Option<(String, Instant)>,
    
    // Bookmarks (1-9 keys)
    pub(crate) bookmarks: HashMap<usize, BookmarkState>,
    
    // Image management
    pub(crate) last_moved: Option<(PathBuf, PathBuf, String, usize)>, // source, dest, type, previous index

    // Cache management
    pub(crate) last_cache_cleanup: Instant,
    pub(crate) last_distant_cleanup: Instant,

    // Event bus
    pub(crate) event_receiver: mpsc::Receiver<ViewerEvent>,

    // Lifecycle
    pub(crate) should_close: bool,

    // Preferences
    pub(crate) sort_newest_first: bool,

    // Performance tuning
    pub(crate) max_cached_dimension: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct BookmarkState {
    pub(crate) index: usize,
    pub(crate) zoom: f32,
    pub(crate) position: egui::Vec2,
    pub(crate) composition: bool,
    pub(crate) composition_type: CompositionType,
}

impl ImageViewerApp {
    pub(crate) fn clean_cache_for_moved_image(&mut self, moved_path: &PathBuf) {
        let path_str = moved_path.to_string_lossy().to_string();

        // Remove from all caches
        self.image_cache.remove(&path_str);
        self.metadata_cache.remove(&path_str);

        // Also remove any composition cache entries for this image
        // Since composition cache keys might include image dimensions,
        // we need to check for keys that start with the path
        let composition_keys: Vec<String> = {
            let cache_guard = self.composition_cache.cache.lock().unwrap();
            cache_guard.iter().filter_map(|(key, _)| {
                if key.starts_with(&path_str) {
                    Some(key.clone())
                } else {
                    None
                }
            }).collect()
        };

        for key in composition_keys {
            self.composition_cache.remove(&key);
        }
    }

    pub(crate) fn cleanup_old_cache_entries(&mut self) {
        // Cleanup entries older than 5 minutes for images not currently visible
        let max_age = Duration::from_secs(300); // 5 minutes

        self.image_cache.cleanup_old_entries(max_age);
        self.metadata_cache.cleanup_old_entries(max_age);
        self.composition_cache.cleanup_old_entries(max_age);
    }

    pub(crate) fn cleanup_cache_by_usage(&mut self) {
        // If caches are getting full, remove entries that haven't been accessed recently
        const IMAGE_CACHE_THRESHOLD: usize = 80; // cleanup when 80% full
        const METADATA_CACHE_THRESHOLD: usize = 150;
        const COMPOSITION_CACHE_THRESHOLD: usize = 25;

        if self.image_cache.len() > IMAGE_CACHE_THRESHOLD {
            self.image_cache.cleanup_old_entries(Duration::from_secs(120)); // 2 minutes
        }

        if self.metadata_cache.len() > METADATA_CACHE_THRESHOLD {
            self.metadata_cache.cleanup_old_entries(Duration::from_secs(180)); // 3 minutes
        }

        if self.composition_cache.len() > COMPOSITION_CACHE_THRESHOLD {
            self.composition_cache.cleanup_old_entries(Duration::from_secs(60)); // 1 minute
        }
    }

    pub(crate) fn cleanup_distant_cache_entries(&mut self) {
        // Remove cache entries for images that are far from current position
        // This helps keep memory usage low while preserving nearby images
        const CACHE_DISTANCE: usize = 20; // Keep images within ±20 positions

        if self.image_files.is_empty() {
            return;
        }

        let start_index = self.current_index.saturating_sub(CACHE_DISTANCE);
        let end_index = (self.current_index + CACHE_DISTANCE).min(self.image_files.len() - 1);

        // Collect paths of images that should be kept
        let keep_paths: std::collections::HashSet<String> = self.image_files[start_index..=end_index]
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();

        // Remove entries not in the keep list
        let image_keys: Vec<String> = {
            let cache_guard = self.image_cache.cache.lock().unwrap();
            cache_guard.iter().filter_map(|(key, _): (&String, _)| {
                if !keep_paths.contains(key) {
                    Some(key.clone())
                } else {
                    None
                }
            }).collect()
        };

        for key in image_keys {
            self.image_cache.remove(&key);
        }

        // Same for metadata cache
        let metadata_keys: Vec<String> = {
            let cache_guard = self.metadata_cache.cache.lock().unwrap();
            cache_guard.iter().filter_map(|(key, _): (&String, _)| {
                if !keep_paths.contains(key) {
                    Some(key.clone())
                } else {
                    None
                }
            }).collect()
        };

        for key in metadata_keys {
            self.metadata_cache.remove(&key);
        }
    }
}
