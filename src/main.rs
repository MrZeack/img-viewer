use eframe::egui;

mod app;
mod cache;
mod composition;
mod image_loader;
mod metadata_simple;

use app::ImageViewerApp;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("Image Viewer")
            .with_maximize_button(true),
        ..Default::default()
    };

    eframe::run_native(
        "Image Viewer",
        options,
        Box::new(|cc| {
            // Configure UI style for dark theme
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals = egui::Visuals::dark();
            cc.egui_ctx.set_style(style);

            Ok(Box::new(ImageViewerApp::new(cc)))
        }),
    )
}