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

    /// Tags 1-9: names (persisted) and session assignments
    pub(crate) tags: super::tags::TagStore,
    /// Open tag modal, if any (blocks global shortcuts)
    pub(crate) tag_modal: Option<super::tags::TagModal>,

    /// Grid (thumbnail) view instead of the single image
    pub(crate) show_grid: bool,
    pub(crate) grid: super::grid::GridState,

    /// Last move, for undo (U)
    pub(crate) last_moved: Option<LastMove>,

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

/// Last move performed, so it can be undone
#[derive(Clone, Debug)]
pub(crate) enum LastMove {
    /// Single image moved to best/worse/deleted
    Single {
        source: PathBuf,
        dest: PathBuf,
        label: String,
        index: usize,
    },
    /// Tagged images moved to their tag folders
    Batch(Vec<super::tags::PlannedMove>),
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
pub(crate) fn test_app(files: Vec<PathBuf>, current_index: usize) -> ImageViewerApp {
    use crate::cache::ImageCache;
    use crate::event::EventBus;
    let (tx, rx) = mpsc::channel::<ViewerEvent>();
    let event_bus = EventBus::new(tx, None);
    let image_loader = ImageLoader::new(1, event_bus.clone());
    let preloader = ImagePreloader::new(
        ImageCache::new(2),
        1,
        event_bus.clone(),
        1,
        image_loader.pending_requests(),
    );
    ImageViewerApp {
        current_dir: PathBuf::from("/tmp"),
        image_files: files,
        current_index,
        current_image: None,
        zoom_factor: 1.0,
        image_position: egui::Vec2::ZERO,
        image_cache: ImageCache::new(2),
        metadata_cache: MetadataCache::new(2),
        composition_cache: CompositionCache::new(2),
        image_loader,
        preloader,
        loading_requests: HashMap::new(),
        current_metadata: None,
        metadata_loader: MetadataLoader::new(1, event_bus),
        show_composition: false,
        composition_type: CompositionType::RuleOfThirds,
        show_sidebar: true,
        sidebar_width: 320.0,
        status_message: None,
        tags: Default::default(),
        tag_modal: None,
        show_grid: false,
        grid: Default::default(),
        last_moved: None,
        last_cache_cleanup: Instant::now(),
        last_distant_cleanup: Instant::now(),
        event_receiver: rx,
        should_close: false,
        sort_newest_first: true,
        max_cached_dimension: 1024,
        rotation: 0,
        rotation_manually_set: false,
        show_focus_peaking: false,
        quality_multiplier: 1,
        config: crate::config::Config::default(),
    }
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
