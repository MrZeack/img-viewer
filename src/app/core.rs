use super::state::ImageViewerApp;
use eframe::egui::{Context};
use std::time::Instant;
use std::path::PathBuf;
use crate::cache::{create_image_cache, create_metadata_cache, CompositionCache};
use crate::image_loader::{get_image_files, ImageLoader, ImagePreloader};
use crate::metadata_loader::MetadataLoader;
use crate::config::Config;
use crate::event::{EventBus, ViewerEvent};
use std::sync::mpsc;

impl ImageViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, config: Config) -> Self {
        let (event_sender, event_receiver) = mpsc::channel();
        let event_bus = EventBus::new(event_sender);

        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let image_files = get_image_files(&current_dir, config.browsing.sort_newest_first).unwrap_or_default();
        
        let image_cache = create_image_cache(config.cache.image_capacity);
        let preview_dim = config.performance.max_cached_dimension.min(1024);
        let preloader = ImagePreloader::new(
            image_cache.clone(),
            config.performance.preload_distance,
            event_bus.clone(),
            preview_dim,
        );
        
        // Create directories for image management
        std::fs::create_dir_all(current_dir.join("best")).ok();
        std::fs::create_dir_all(current_dir.join("worse")).ok();
        std::fs::create_dir_all(current_dir.join("deleted")).ok();
        
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
            image_loader: ImageLoader::new(config.performance.loader_threads, event_bus.clone()),
            preloader,
            loading_requests: std::collections::HashMap::new(),
            current_metadata: None,
            metadata_loader: MetadataLoader::new((config.performance.loader_threads / 2).max(1), event_bus.clone()),
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
        if self.image_files.is_empty() {
            return;
        }
        
        let current_path = &self.image_files[self.current_index];
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
            let d = self.max_cached_dimension;
            self.image_loader.load_image(current_path, Some((d, d)), true);
            self.loading_requests.insert(current_path.clone(), Instant::now());
        }
        
        // Start preloading adjacent images
        self.preloader.preload_adjacent(current_path, &self.image_files);
        
        // Reset view state for new image only if requested
        if reset_view {
            self.zoom_factor = 1.0;
            self.image_position = egui::Vec2::ZERO;
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

                    if path == self.image_files[self.current_index] {
                        self.current_image = Some(ctx.load_texture(
                            path_str,
                            image_arc,
                            egui::TextureOptions::default(),
                        ));
                    }
                    self.loading_requests.remove(&path);
                }
                ViewerEvent::MetadataReady(path, metadata) => {
                    if path == self.image_files[self.current_index] {
                        self.current_metadata = Some(metadata.clone());
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
            Instant::now() + std::time::Duration::from_secs_f32(duration_secs)
        ));
    }
}
