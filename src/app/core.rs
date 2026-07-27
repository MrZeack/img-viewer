use super::state::ImageViewerApp;
use crate::cache::{create_image_cache, create_metadata_cache, CompositionCache};
use crate::config::Config;
use crate::event::{EventBus, ViewerEvent};
use crate::image_loader::{get_image_files, ImageLoader, ImagePreloader};
use crate::metadata_loader::MetadataLoader;
use eframe::egui::Context;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

impl ImageViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, config: Config) -> Self {
        let (event_sender, event_receiver) = mpsc::channel();
        let event_bus = EventBus::new(event_sender, Some(cc.egui_ctx.clone()));

        let current_dir = std::env::current_dir().unwrap_or_else(|err| {
            eprintln!("Warning: Could not get current directory: {}", err);
            PathBuf::from(".")
        });
        let image_files = match get_image_files(&current_dir, config.browsing.sort_newest_first) {
            Ok(files) => files,
            Err(e) => {
                eprintln!("Failed to get image files: {}", e);
                Vec::new()
            }
        };

        let image_cache = create_image_cache(config.cache.image_capacity);
        let image_loader = ImageLoader::new(config.performance.loader_threads, event_bus.clone());
        let preview_dim = config.performance.max_cached_dimension.min(1024);
        let preloader = ImagePreloader::new(
            image_cache.clone(),
            config.performance.preload_distance,
            event_bus.clone(),
            preview_dim,
            image_loader.pending_requests(),
        );

        let mut app = Self {
            current_dir,
            image_files,
            current_index: 0,
            current_image: None,
            zoom_factor: 1.0,
            image_position: egui::Vec2::ZERO,
            image_cache,
            metadata_cache: create_metadata_cache(config.cache.metadata_capacity),
            composition_cache: CompositionCache::new(30),
            image_loader,
            preloader,
            loading_requests: std::collections::HashMap::new(),
            current_metadata: None,
            metadata_loader: MetadataLoader::new(
                (config.performance.loader_threads / 2).max(1),
                event_bus.clone(),
            ),
            show_composition: false,
            composition_type: crate::composition::CompositionType::RuleOfThirds,
            show_sidebar: true,
            sidebar_width: 320.0,
            status_message: None,
            bookmarks: std::collections::HashMap::new(),
            last_moved: None,
            last_cache_cleanup: Instant::now(),
            last_distant_cleanup: Instant::now(),
            event_receiver,
            should_close: false,
            sort_newest_first: config.browsing.sort_newest_first,
            max_cached_dimension: config.performance.max_cached_dimension,
            rotation: 0,
            rotation_manually_set: false,
        };

        if !app.image_files.is_empty() {
            app.load_current_image(&cc.egui_ctx);
        }

        app
    }

    pub(crate) fn load_current_image(&mut self, ctx: &Context) {
        self.load_current_image_with_reset(ctx, true);
    }

    pub(crate) fn load_current_image_with_reset(&mut self, ctx: &Context, reset_view: bool) {
        let Some(current_path) = self.current_path().cloned() else {
            self.current_image = None;
            self.current_metadata = None;
            return;
        };
        let path_str = current_path.to_string_lossy().to_string();

        // Check cache first
        if let Some(cached_image_arc) = self.image_cache.get(&path_str) {
            self.current_image = Some(ctx.load_texture(
                &path_str,
                cached_image_arc.clone(),
                egui::TextureOptions::default(),
            ));
        } else {
            // Request image load
            let ext = current_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            // For RAW files, we allow a larger preview to avoid pixelation
            // 4096 is a good balance between quality and memory for modern RAW previews
            let d = if matches!(ext.as_str(), "arw" | "cr3") {
                self.max_cached_dimension.max(4096)
            } else {
                self.max_cached_dimension
            };

            self.image_loader
                .load_image(&current_path, Some((d, d)), true);
            self.loading_requests
                .insert(current_path.clone(), Instant::now());
        }

        // Start preloading adjacent images
        self.preloader
            .preload_adjacent(&current_path, &self.image_files);

        // Reset view state for new image only if requested
        if reset_view {
            self.zoom_factor = 1.0;
            self.image_position = egui::Vec2::ZERO;
            self.rotation = 0;
            self.rotation_manually_set = false;
        }

        // Check metadata cache for auto-rotation
        if let Some(cached_metadata) = self.metadata_cache.get(&path_str) {
            self.current_metadata = Some(cached_metadata.clone());
            if !self.rotation_manually_set {
                if let Some(exif) = &cached_metadata.exif_info {
                    if let Some(orientation) = &exif.orientation {
                        self.rotation = match orientation.as_str() {
                            "row 0 at top and column 0 at left" => 0,
                            "row 0 at bottom and column 0 at right" => 2,
                            "row 0 at right and column 0 at top" => 1,
                            "row 0 at left and column 0 at bottom" => 3,
                            "row 0 at top and column 0 at right" => 0,
                            "row 0 at bottom and column 0 at left" => 2,
                            "row 0 at left and column 0 at top" => 3,
                            "row 0 at right and column 0 at bottom" => 1,
                            _ => 0,
                        };
                    }
                }
            }
        } else {
            self.current_metadata = None;
        }

        // Request metadata extraction
        let path = current_path.clone();
        self.metadata_loader.load_metadata(&path);

        // Aggressive cleanup of distant entries on navigation
        self.cleanup_distant_cache_entries();
    }

    pub(crate) fn process_events(&mut self, ctx: &Context) {
        while let Ok(event) = self.event_receiver.try_recv() {
            match event {
                ViewerEvent::ImageLoaded(path, image_arc) => {
                    let path_str = path.to_string_lossy().to_string();
                    self.image_cache.put(path_str.clone(), image_arc.clone());

                    if should_apply_event(self.current_path(), &path) {
                        self.current_image = Some(ctx.load_texture(
                            path_str,
                            image_arc,
                            egui::TextureOptions::default(),
                        ));
                    }
                    self.loading_requests.remove(&path);
                }
                ViewerEvent::MetadataReady(path, metadata) => {
                    let metadata = *metadata;
                    if should_apply_event(self.current_path(), &path) {
                        self.current_metadata = Some(metadata.clone());
                        if !self.rotation_manually_set {
                            if let Some(exif) = &metadata.exif_info {
                                if let Some(orientation) = &exif.orientation {
                                    self.rotation = match orientation.as_str() {
                                        "row 0 at top and column 0 at left" => 0,
                                        "row 0 at bottom and column 0 at right" => 2,
                                        "row 0 at right and column 0 at top" => 1,
                                        "row 0 at left and column 0 at bottom" => 3,
                                        "row 0 at top and column 0 at right" => 0,
                                        "row 0 at bottom and column 0 at left" => 2,
                                        "row 0 at left and column 0 at top" => 3,
                                        "row 0 at right and column 0 at bottom" => 1,
                                        _ => 0,
                                    };
                                }
                            }
                        }
                    }
                    let path_str = path.to_string_lossy().to_string();
                    self.metadata_cache.put(path_str, metadata);
                }
            }
        }
    }

    pub(crate) fn set_status_message(&mut self, message: &str, duration_secs: f32) {
        self.status_message = Some((
            message.to_string(),
            Instant::now() + std::time::Duration::from_secs_f32(duration_secs),
        ));
    }
}

pub(crate) fn should_apply_event(
    current_path: Option<&std::path::PathBuf>,
    event_path: &std::path::PathBuf,
) -> bool {
    current_path.is_some_and(|path| path == event_path)
}

#[cfg(test)]
mod tests {
    use super::should_apply_event;
    use std::path::PathBuf;

    #[test]
    fn should_apply_event_accepts_matching_path() {
        let current = PathBuf::from("/tmp/current.jpg");
        let event_path = PathBuf::from("/tmp/current.jpg");
        assert!(should_apply_event(Some(&current), &event_path));
    }

    #[test]
    fn should_apply_event_rejects_stale_path() {
        let current = PathBuf::from("/tmp/current.jpg");
        let event_path = PathBuf::from("/tmp/stale.jpg");
        assert!(!should_apply_event(Some(&current), &event_path));
    }

    #[test]
    fn should_apply_event_rejects_missing_current_path() {
        let event_path = PathBuf::from("/tmp/stale.jpg");
        assert!(!should_apply_event(None, &event_path));
    }
}
