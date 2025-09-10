use eframe::egui;
use std::path::PathBuf;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("Image Viewer")
            .with_maximize_button(true),
        ..Default::default()
    };

    eframe::run_native(
        "Simple Image Viewer",
        options,
        Box::new(|cc| {
            // Configure dark theme
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals = egui::Visuals::dark();
            cc.egui_ctx.set_style(style);
            
            Box::new(SimpleImageViewer::new())
        }),
    )
}

struct SimpleImageViewer {
    current_dir: PathBuf,
    image_files: Vec<PathBuf>,
    current_index: usize,
    current_image: Option<egui::TextureHandle>,
    zoom_factor: f32,
    image_position: egui::Vec2,
}

impl SimpleImageViewer {
    fn new() -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let image_files = get_image_files(&current_dir);
        
        Self {
            current_dir,
            image_files,
            current_index: 0,
            current_image: None,
            zoom_factor: 1.0,
            image_position: egui::Vec2::ZERO,
        }
    }
    
    fn load_current_image(&mut self, ctx: &egui::Context) {
        if self.image_files.is_empty() {
            return;
        }
        
        let image_path = &self.image_files[self.current_index];
        
        match image::open(image_path) {
            Ok(img) => {
                let rgb_image = img.to_rgb8();
                let size = [rgb_image.width() as usize, rgb_image.height() as usize];
                let pixels: Vec<egui::Color32> = rgb_image
                    .pixels()
                    .map(|p| egui::Color32::from_rgb(p[0], p[1], p[2]))
                    .collect();
                
                let color_image = egui::ColorImage { size, pixels };
                let texture_name = format!("image_{}", self.current_index);
                
                self.current_image = Some(ctx.load_texture(
                    texture_name,
                    color_image,
                    egui::TextureOptions::default(),
                ));
                
                // Reset view for new image
                self.zoom_factor = 1.0;
                self.image_position = egui::Vec2::ZERO;
            }
            Err(e) => {
                eprintln!("Failed to load image: {}", e);
            }
        }
    }
    
    fn next_image(&mut self, ctx: &egui::Context) {
        if !self.image_files.is_empty() {
            self.current_index = (self.current_index + 1) % self.image_files.len();
            self.load_current_image(ctx);
        }
    }
    
    fn previous_image(&mut self, ctx: &egui::Context) {
        if !self.image_files.is_empty() {
            self.current_index = (self.current_index + self.image_files.len() - 1) % self.image_files.len();
            self.load_current_image(ctx);
        }
    }
}

impl eframe::App for SimpleImageViewer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle keyboard input
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            self.previous_image(ctx);
        } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            self.next_image(ctx);
        } else if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            std::process::exit(0);
        }
        
        // Load first image if none loaded yet
        if self.current_image.is_none() && !self.image_files.is_empty() {
            self.load_current_image(ctx);
        }
        
        // Status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if !self.image_files.is_empty() {
                    let current_file = self.image_files[self.current_index]
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy();
                    
                    ui.label(format!(
                        "{}/{} - {} - Zoom: {:.2}x",
                        self.current_index + 1,
                        self.image_files.len(),
                        current_file,
                        self.zoom_factor
                    ));
                }
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label("RIGHT/LEFT : Navigate | Mouse Wheel: Zoom | Esc: Exit");
                });
            });
        });
        
        // Main image display
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(texture) = &self.current_image {
                let image_size = texture.size_vec2();
                let available_size = ui.available_size();
                
                // Calculate fit-to-window scale
                let scale_x = available_size.x / image_size.x;
                let scale_y = available_size.y / image_size.y;
                let fit_scale = scale_x.min(scale_y);
                
                // Apply zoom
                let display_scale = fit_scale * self.zoom_factor;
                let display_size = image_size * display_scale;
                
                // Center image if smaller than available area
                let mut image_pos = self.image_position;
                if display_size.x < available_size.x {
                    image_pos.x = (available_size.x - display_size.x) * 0.5;
                }
                if display_size.y < available_size.y {
                    image_pos.y = (available_size.y - display_size.y) * 0.5;
                }
                
                // Draw image
                let image_rect = egui::Rect::from_min_size(
                    ui.min_rect().min + image_pos,
                    display_size
                );
                
                let response = ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::click_and_drag());
                
                ui.painter().image(
                    texture.id(),
                    image_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
                
                // Handle mouse input for zoom
                if response.hovered() {
                    let scroll_delta = ctx.input(|i| i.raw_scroll_delta.y);
                    if scroll_delta != 0.0 {
                        let zoom_factor = if scroll_delta > 0.0 { 1.1 } else { 0.9 };
                        self.zoom_factor = (self.zoom_factor * zoom_factor).clamp(0.1, 10.0);
                    }
                }
                
                // Handle dragging
                if response.dragged() {
                    self.image_position += response.drag_delta();
                }
            } else {
                ui.centered_and_justified(|ui| {
                    if self.image_files.is_empty() {
                        ui.label("No images found in current directory");
                        ui.label("Supported formats: JPEG, PNG, BMP, TIFF");
                    } else {
                        ui.label("Loading image...");
                    }
                });
            }
        });
        
        // Request repaint for smooth animations
        ctx.request_repaint();
    }
}

fn get_image_files(dir: &PathBuf) -> Vec<PathBuf> {
    let mut files = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            
            if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy().to_lowercase();
                if matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "bmp" | "tiff" | "webp") {
                    files.push(path);
                }
            }
        }
    }
    
    files.sort();
    files
}