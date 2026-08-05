use eframe::egui;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::cache::{CompositionCache, ImageCache, MetadataCache};
use crate::composition::CompositionType;
use crate::event::ViewerEvent;
use crate::image_loader::{ImageLoader, ImagePreloader};
use crate::metadata_loader::MetadataLoader;
use crate::metadata_simple::ImageMetadata;
use std::sync::mpsc;

/// Main application state for the image viewer
pub struct ImageViewerApp {
    /// Current directory containing images
    pub(crate) current_dir: PathBuf,
    /// List of image files in the current directory
    pub(crate) image_files: Vec<PathBuf>,
    /// Index of the currently displayed image
    pub(crate) current_index: usize,

    /// Currently displayed image
    pub(crate) current_image: Option<egui::TextureHandle>,
    /// Current zoom factor
    pub(crate) zoom_factor: f32,
    /// Current position of the image view
    pub(crate) image_position: egui::Vec2,

    /// Cache for loaded images
    pub(crate) image_cache: ImageCache,
    /// Cache for image metadata
    pub(crate) metadata_cache: MetadataCache,
    /// Cache for composition lines
    pub(crate) composition_cache: CompositionCache,

    /// Loader for images
    pub(crate) image_loader: ImageLoader,
    /// Preloader for adjacent images
    pub(crate) preloader: ImagePreloader,
    /// Track pending image loading requests
    pub(crate) loading_requests: HashMap<PathBuf, Instant>,

    /// Metadata for the current image
    pub(crate) current_metadata: Option<ImageMetadata>,
    /// Loader for image metadata
    pub(crate) metadata_loader: MetadataLoader,

    /// Whether to show composition lines
    pub(crate) show_composition: bool,
    /// Current type of composition guide
    pub(crate) composition_type: CompositionType,

    /// Whether the sidebar is visible
    pub(crate) show_sidebar: bool,
    /// Width of the sidebar
    pub(crate) sidebar_width: f32,
    /// Status message with expiration time
    pub(crate) status_message: Option<(String, Instant)>,

    /// Bookmarks for quick navigation (keys 1-9)
    pub(crate) bookmarks: HashMap<usize, BookmarkState>,

    /// Information about the last moved image for undo functionality
    pub(crate) last_moved: Option<(PathBuf, PathBuf, String, usize)>, // source, dest, type, previous index

    /// Time of last general cache cleanup
    pub(crate) last_cache_cleanup: Instant,
    /// Time of last distant cache cleanup
    pub(crate) last_distant_cleanup: Instant,

    /// Receiver for application events
    pub(crate) event_receiver: mpsc::Receiver<ViewerEvent>,

    /// Whether the application should close
    pub(crate) should_close: bool,

    /// Whether to sort images by newest first
    pub(crate) sort_newest_first: bool,

    /// Maximum cached dimension for performance
    pub(crate) max_cached_dimension: u32,

    /// Rotation of the current image in 90-degree steps (0, 1, 2, 3)
    pub(crate) rotation: i32,

    /// Whether rotation was manually set by user (prevents EXIF metadata overwrite)
    pub(crate) rotation_manually_set: bool,

    /// Whether to show focus peaking overlay
    pub(crate) show_focus_peaking: bool,

    /// Quality multiplier for on-demand high resolution rendering (1: Normal, 2: 3x, 3: Original/Max)
    pub(crate) quality_multiplier: u8,

    /// Application configuration
    pub(crate) config: crate::config::Config,
}

impl ImageViewerApp {
    pub(crate) fn save_view_config(&mut self) {
        self.config.view.show_composition = self.show_composition;
        self.config.view.composition_type = format!("{:?}", self.composition_type);
        self.config.view.show_focus_peaking = self.show_focus_peaking;
        crate::config::save_config(&self.config);
    }
}

/// Represents the state of a bookmarked image position
#[derive(Clone, Debug)]
pub(crate) struct BookmarkState {
    /// Index of the image in the current directory
    pub(crate) index: usize,
    /// Zoom level at the time of bookmarking
    pub(crate) zoom: f32,
    /// Position of the image view at the time of bookmarking
    pub(crate) position: egui::Vec2,
    /// Whether composition lines were visible
    pub(crate) composition: bool,
    /// The type of composition guide that was active
    pub(crate) composition_type: CompositionType,
    /// Rotation at the time of bookmarking
    pub(crate) rotation: i32,
}

impl ImageViewerApp {
    pub(crate) fn current_path(&self) -> Option<&PathBuf> {
        current_path_from(&self.image_files, self.current_index)
    }

    pub(crate) fn clean_cache_for_moved_image(&mut self, moved_path: &Path) {
        let path_str = moved_path.to_string_lossy().to_string();

        // Remove from all caches
        self.image_cache.remove(&path_str);
        self.metadata_cache.remove(&path_str);

        // Also remove any composition cache entries for this image
        // Since composition cache keys might include image dimensions,
        // we need to check for keys that start with the path
        let composition_keys: Vec<String> = {
            let cache_guard = self.composition_cache.cache.lock().unwrap();
            cache_guard
                .iter()
                .filter_map(|(key, _)| {
                    if key.starts_with(&path_str) {
                        Some(key.clone())
                    } else {
                        None
                    }
                })
                .collect()
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
            self.image_cache
                .cleanup_old_entries(Duration::from_secs(120)); // 2 minutes
        }

        if self.metadata_cache.len() > METADATA_CACHE_THRESHOLD {
            self.metadata_cache
                .cleanup_old_entries(Duration::from_secs(180)); // 3 minutes
        }

        if self.composition_cache.len() > COMPOSITION_CACHE_THRESHOLD {
            self.composition_cache
                .cleanup_old_entries(Duration::from_secs(60)); // 1 minute
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
        let keep_paths: std::collections::HashSet<String> = self.image_files
            [start_index..=end_index]
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();

        // Remove entries not in the keep list
        let image_keys: Vec<String> = {
            let cache_guard = self.image_cache.cache.lock().unwrap();
            cache_guard
                .iter()
                .filter_map(|(key, _): (&String, _)| {
                    if !keep_paths.contains(key) {
                        Some(key.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        for key in image_keys {
            self.image_cache.remove(&key);
        }

        // Same for metadata cache
        let metadata_keys: Vec<String> = {
            let cache_guard = self.metadata_cache.cache.lock().unwrap();
            cache_guard
                .iter()
                .filter_map(|(key, _): (&String, _)| {
                    if !keep_paths.contains(key) {
                        Some(key.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        for key in metadata_keys {
            self.metadata_cache.remove(&key);
        }
    }
}

pub(crate) fn current_path_from(image_files: &[PathBuf], current_index: usize) -> Option<&PathBuf> {
    image_files.get(current_index)
}

#[cfg(test)]
mod tests {
    use super::current_path_from;
    use std::path::PathBuf;

    #[test]
    fn current_path_from_returns_none_for_empty_list() {
        let files: Vec<PathBuf> = Vec::new();
        assert!(current_path_from(&files, 0).is_none());
    }

    #[test]
    fn current_path_from_returns_path_when_index_is_valid() {
        let files = vec![PathBuf::from("a.jpg")];
        let expected = PathBuf::from("a.jpg");
        assert_eq!(current_path_from(&files, 0), Some(&expected));
    }
}
