use super::state::ImageViewerApp;
use crate::cache::{create_image_cache, create_metadata_cache, CompositionCache};
use crate::config::Config;
use crate::event::{EventBus, ViewerEvent};
use crate::image_loader::{ImageLoader, ImagePreloader};
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
        let image_cache = create_image_cache(config.cache.image_capacity);
        let image_loader = ImageLoader::new(config.performance.loader_threads, event_bus.clone());
        let preview_dim = config.performance.max_cached_dimension.min(1024);
        let preloader = ImagePreloader::new(
            image_loader.clone(),
            image_cache.clone(),
            config.performance.preload_distance,
            preview_dim,
        );

        let mut app = Self {
            current_dir,
            image_files: Vec::new(),
            current_index: 0,
            current_image: None,
            current_load_error: None,
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
            show_composition: config.view.show_composition,
            composition_type: parse_composition_type(&config.view.composition_type),
            show_sidebar: true,
            sidebar_width: 320.0,
            status_message: None,
            tags: super::tags::TagStore::from_config(&config.tags.names),
            tag_modal: None,
            show_grid: false,
            show_help: false,
            sort_order: super::sorting::SortOrder::from_config(&config.browsing.sort_order),
            mtimes: std::collections::HashMap::new(),
            folder_load: None,
            grid: Default::default(),
            last_moved: None,
            last_cache_cleanup: Instant::now(),
            last_distant_cleanup: Instant::now(),
            event_receiver,
            should_close: false,
            max_cached_dimension: config.performance.max_cached_dimension,
            rotation: 0,
            rotation_manually_set: false,
            show_focus_peaking: config.view.show_focus_peaking,
            focus_peaking_cache: None,
            quality_multiplier: 1,
            config,
        };

        let dir = app.current_dir.clone();
        app.start_folder_load(dir);
        app
    }

    pub(crate) fn load_current_image(&mut self, ctx: &Context) {
        self.load_current_image_with_reset(ctx, true);
    }

    pub(crate) fn desired_target_dim(&self, path: &std::path::Path) -> Option<u32> {
        match self.quality_multiplier {
            2 => Some((self.max_cached_dimension * 3).min(6144)),
            3 => None,
            _ => {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let d = if matches!(ext.as_str(), "arw" | "cr3") {
                    self.max_cached_dimension.max(4096)
                } else {
                    self.max_cached_dimension
                };
                Some(d)
            }
        }
    }

    pub(crate) fn load_current_image_with_reset(&mut self, ctx: &Context, reset_view: bool) {
        let Some(current_path) = self.current_path().cloned() else {
            self.current_image = None;
            self.current_load_error = None;
            self.current_metadata = None;
            return;
        };
        self.current_load_error = None;
        let path_str = current_path.to_string_lossy().to_string();
        let target_dim = self.desired_target_dim(&current_path);
        let target_size = target_dim.map(|d| (d, d));

        let cached = self.image_cache.get_entry(&path_str);
        let mut needs_load = true;
        if let Some(entry) = cached {
            // Show whatever resolution is already cached immediately to avoid blank frames.
            self.current_image = Some(ctx.load_texture(
                &path_str,
                entry.image.clone(),
                egui::TextureOptions::default(),
            ));
            if entry.satisfies(target_dim) {
                needs_load = false;
            }
        }

        if needs_load {
            self.image_loader
                .load_image(&current_path, target_size, true);
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
                ViewerEvent::ImageLoaded {
                    path,
                    image,
                    target_dim,
                } => {
                    let path_str = path.to_string_lossy().to_string();
                    let updated =
                        self.image_cache
                            .put_if_better(path_str.clone(), image.clone(), target_dim);

                    if should_apply_event(self.current_path(), &path) {
                        self.current_load_error = None;
                        let desired = self.desired_target_dim(&path);
                        let best = self
                            .image_cache
                            .get_entry(&path_str)
                            .map(|e| e.image)
                            .unwrap_or(image);
                        if updated || self.current_image.is_none() {
                            self.current_image = Some(ctx.load_texture(
                                path_str,
                                best,
                                egui::TextureOptions::default(),
                            ));
                        }
                        let satisfied = self
                            .image_cache
                            .get_entry(&path.to_string_lossy())
                            .is_some_and(|e| e.satisfies(desired));
                        if satisfied {
                            self.loading_requests.remove(&path);
                        }
                    } else {
                        self.loading_requests.remove(&path);
                    }
                }
                ViewerEvent::ImageLoadFailed { path, error } => {
                    self.loading_requests.remove(&path);
                    if should_apply_event(self.current_path(), &path) {
                        self.current_image = None;
                        self.current_load_error = Some(error);
                    }
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

fn parse_composition_type(name: &str) -> crate::composition::CompositionType {
    use crate::composition::CompositionType;
    match name {
        "GoldenRatio" => CompositionType::GoldenRatio,
        "GoldenSpiral" => CompositionType::GoldenSpiral,
        "CenterLines" => CompositionType::CenterLines,
        "Diagonal" => CompositionType::Diagonal,
        "Triangle" => CompositionType::Triangle,
        "PhiGrid" => CompositionType::PhiGrid,
        "DynamicSymmetry" => CompositionType::DynamicSymmetry,
        "ArmatureOfRectangle" => CompositionType::ArmatureOfRectangle,
        "Rabatment" => CompositionType::Rabatment,
        "HarmoniousTriangles" => CompositionType::HarmoniousTriangles,
        "SinisterDiagonal" => CompositionType::SinisterDiagonal,
        "AspectRatioGuides" => CompositionType::AspectRatioGuides,
        _ => CompositionType::RuleOfThirds,
    }
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

    #[test]
    fn failed_load_clears_loading_request_and_sets_error() {
        let p = PathBuf::from("/tmp/broken.jpg");
        let (tx, rx) = std::sync::mpsc::channel();
        let mut app = super::super::state::test_app(vec![p.clone()], 0);
        app.event_receiver = rx;
        app.loading_requests
            .insert(p.clone(), std::time::Instant::now());

        tx.send(crate::event::ViewerEvent::ImageLoadFailed {
            path: p.clone(),
            error: "corrupt file".into(),
        })
        .unwrap();

        let ctx = eframe::egui::Context::default();
        app.process_events(&ctx);

        assert!(!app.loading_requests.contains_key(&p));
        assert_eq!(app.current_load_error.as_deref(), Some("corrupt file"));
    }
}
