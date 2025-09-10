use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

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
    pub(crate) last_moved: Option<(PathBuf, PathBuf, String)>, // source, dest, type

    // Event bus
    pub(crate) event_receiver: mpsc::Receiver<ViewerEvent>,
}

#[derive(Clone, Debug)]
pub(crate) struct BookmarkState {
    pub(crate) index: usize,
    pub(crate) zoom: f32,
    pub(crate) position: egui::Vec2,
    pub(crate) composition: bool,
    pub(crate) composition_type: CompositionType,
}
