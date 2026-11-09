use eframe::egui;

#[path = "app.rs"]
mod app;
mod cache;
mod composition;
mod config;
mod cr3;
mod errors;
mod event;
mod focus;
mod image_loader;
mod metadata_loader;
mod metadata_simple;

use app::ImageViewerApp;

fn main() -> Result<(), eframe::Error> {
    let version = env!("CARGO_PKG_VERSION");
    if std::env::args()
        .skip(1)
        .any(|a| a == "--version" || a == "-V")
    {
        println!("img-viewer {version}");
        return Ok(());
    }

    let config = config::load_config();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title(format!("Image Viewer v{}", version))
            .with_maximize_button(true),
        ..Default::default()
    };

    eframe::run_native(
        &format!("Image Viewer v{}", version),
        options,
        Box::new(move |cc| {
            // Configure UI style for dark theme
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals = egui::Visuals::dark();
            cc.egui_ctx.set_style(style);

            Ok(Box::new(ImageViewerApp::new(cc, config)))
        }),
    )
}
