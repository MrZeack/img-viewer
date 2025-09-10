use crate::metadata_simple::ImageMetadata;
use egui::ColorImage;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum ViewerEvent {
    ImageLoaded(PathBuf, Arc<ColorImage>),
    MetadataReady(PathBuf, ImageMetadata),
}

#[derive(Clone)]
pub struct EventBus {
    sender: mpsc::Sender<ViewerEvent>,
}

impl EventBus {
    pub fn new(sender: mpsc::Sender<ViewerEvent>) -> Self {
        Self { sender }
    }

    pub fn publish(&self, event: ViewerEvent) {
        let _ = self.sender.send(event);
    }
}
