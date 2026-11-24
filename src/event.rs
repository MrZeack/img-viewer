use crate::metadata_simple::ImageMetadata;
use egui::ColorImage;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum ViewerEvent {
    ImageLoaded {
        path: PathBuf,
        image: Arc<ColorImage>,
        /// Maximum target dimension requested (`None` = native full resolution).
        target_dim: Option<u32>,
    },
    ImageLoadFailed {
        path: PathBuf,
        error: String,
    },
    MetadataReady(PathBuf, Box<ImageMetadata>),
}

#[derive(Clone)]
pub struct EventBus {
    sender: mpsc::Sender<ViewerEvent>,
    ctx: Option<egui::Context>,
}

impl EventBus {
    pub fn new(sender: mpsc::Sender<ViewerEvent>, ctx: Option<egui::Context>) -> Self {
        Self { sender, ctx }
    }

    pub fn publish(&self, event: ViewerEvent) {
        let _ = self.sender.send(event);
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
    }
}
