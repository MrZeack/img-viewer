use eframe::egui;
use egui::{CentralPanel, Context, SidePanel, TopBottomPanel, Key};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

use crate::cache::{create_image_cache, create_metadata_cache, CompositionCache, ImageCache, MetadataCache};
use crate::composition::{generate_composition_lines, CompositionType};
use crate::image_loader::{get_image_files, ImageLoader, ImagePreloader};
use crate::metadata_simple::{extract_metadata, format_file_size, ImageMetadata};

pub struct ImageViewerApp {
    // Core state
    current_dir: PathBuf,
    image_files: Vec<PathBuf>,
    current_index: usize,
    
    // Image display
    current_image: Option<egui::TextureHandle>,
    zoom_factor: f32,
    image_position: egui::Vec2,
    drag_start: Option<egui::Pos2>,
    
    // Caches
    image_cache: ImageCache,
    metadata_cache: MetadataCache,
    composition_cache: CompositionCache,
    
    // Image loading
    image_loader: ImageLoader,
    preloader: ImagePreloader,
    loading_requests: HashMap<PathBuf, Instant>,
    
    // Metadata
    current_metadata: Option<ImageMetadata>,
    metadata_receiver: Option<mpsc::Receiver<(PathBuf, anyhow::Result<ImageMetadata>)>>,
    metadata_sender: mpsc::Sender<(PathBuf, anyhow::Result<ImageMetadata>)>,
    
    // Composition lines
    show_composition: bool,
    composition_type: CompositionType,
    
    // UI state
    show_sidebar: bool,
    sidebar_width: f32,
    status_message: Option<(String, Instant)>,
    
    // Bookmarks (1-9 keys)
    bookmarks: HashMap<usize, BookmarkState>,
    
    // Image management
    last_moved: Option<(PathBuf, PathBuf, String)>, // source, dest, type
}

#[derive(Clone, Debug)]
struct BookmarkState {
    index: usize,
    zoom: f32,
    position: egui::Vec2,
    composition: bool,
    composition_type: CompositionType,
}

impl ImageViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let image_files = get_image_files(&current_dir).unwrap_or_default();
        
        let image_cache = create_image_cache(100);
        let preloader = ImagePreloader::new(image_cache.clone());
        
        let (metadata_tx, metadata_rx) = mpsc::channel();
        
        // Start metadata extraction for first image
        if !image_files.is_empty() {
            let first_image = image_files[0].clone();
            let tx = metadata_tx.clone();
            std::thread::spawn(move || {
                let metadata = extract_metadata(&first_image);
                let _ = tx.send((first_image, metadata));
            });
        }
        
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
            drag_start: None,
            image_cache,
            metadata_cache: create_metadata_cache(50),
            composition_cache: CompositionCache::new(30),
            image_loader: ImageLoader::new(),
            preloader,
            loading_requests: HashMap::new(),
            current_metadata: None,
            metadata_receiver: Some(metadata_rx),
            metadata_sender: metadata_tx.clone(),
            show_composition: false,
            composition_type: CompositionType::RuleOfThirds,
            show_sidebar: true,
            sidebar_width: 320.0,
            status_message: None,
            bookmarks: HashMap::new(),
            last_moved: None,
        };
        
        if !app.image_files.is_empty() {
            app.load_current_image(&cc.egui_ctx);
        }
        
        app
    }
    
    fn load_current_image(&mut self, ctx: &Context) {
        self.load_current_image_with_reset(ctx, true);
    }

    fn load_current_image_with_reset(&mut self, ctx: &Context, reset_view: bool) {
        if self.image_files.is_empty() {
            return;
        }
        
        let current_path = &self.image_files[self.current_index];
        let path_str = current_path.to_string_lossy().to_string();
        
        // Check cache first
        if let Some(cached_image) = self.image_cache.lock().unwrap().get(&path_str) {
            self.current_image = Some(ctx.load_texture(
                &path_str,
                cached_image.clone(),
                egui::TextureOptions::default(),
            ));
        } else {
            // Request image load
            self.image_loader.load_image(current_path, None, true);
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
        let tx = self.metadata_sender.clone();
        std::thread::spawn(move || {
            let metadata = extract_metadata(&path);
            let _ = tx.send((path, metadata));
        });
    }
    
    fn handle_keyboard_input(&mut self, ctx: &Context) {
        if ctx.input(|i| i.key_pressed(Key::ArrowLeft)) {
            self.previous_image(ctx);
        } else if ctx.input(|i| i.key_pressed(Key::ArrowRight)) {
            self.next_image(ctx);
        } else if ctx.input(|i| i.key_pressed(Key::L)) {
            self.like_image(ctx);
        } else if ctx.input(|i| i.key_pressed(Key::W)) {
            self.worse_image(ctx);
        } else if ctx.input(|i| i.key_pressed(Key::D)) {
            self.delete_image(ctx);
        } else if ctx.input(|i| i.key_pressed(Key::U)) {
            self.undo_last_move();
        } else if ctx.input(|i| i.key_pressed(Key::C)) {
            self.toggle_composition_lines();
        } else if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
            if self.show_composition {
                self.composition_type = self.composition_type.next();
                self.composition_cache.clear();
            }
        } else if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
            if self.show_composition {
                self.composition_type = self.composition_type.prev();
                self.composition_cache.clear();
            }
        } else if ctx.input(|i| i.key_pressed(Key::Escape)) {
            std::process::exit(0);
        }
        
        // Handle number keys for bookmarks
        for num in 1..=9 {
            if ctx.input(|i| i.key_pressed(Key::from_name(&num.to_string()).unwrap())) {
                self.handle_bookmark(num, ctx);
            }
        }
        
        if ctx.input(|i| i.key_pressed(Key::Num0)) {
            self.clear_all_bookmarks();
        }
    }
    
    fn previous_image(&mut self, ctx: &Context) {
        if !self.image_files.is_empty() {
            self.current_index = (self.current_index + self.image_files.len() - 1) % self.image_files.len();
            self.load_current_image(ctx);
        }
    }
    
    fn next_image(&mut self, ctx: &Context) {
        if !self.image_files.is_empty() {
            self.current_index = (self.current_index + 1) % self.image_files.len();
            self.load_current_image(ctx);
        }
    }
    
    fn like_image(&mut self, ctx: &Context) {
        if self.image_files.is_empty() {
            return;
        }
        
        let current_path = self.image_files[self.current_index].clone();
        let dest_dir = self.current_dir.join("best");
        let dest_path = dest_dir.join(current_path.file_name().unwrap());
        
        if std::fs::rename(&current_path, &dest_path).is_ok() {
            self.last_moved = Some((current_path, dest_path, "best".to_string()));
            self.image_files.remove(self.current_index);
            
            if self.current_index >= self.image_files.len() && !self.image_files.is_empty() {
                self.current_index = 0;
            }
            
            self.set_status_message("Moved to best/", 3.0);
            
            // Load next image automatically
            if !self.image_files.is_empty() {
                self.load_current_image(ctx);
            }
        } else {
            self.set_status_message("Error moving image", 3.0);
        }
    }
    
    fn worse_image(&mut self, ctx: &Context) {
        if self.image_files.is_empty() {
            return;
        }
        
        let current_path = self.image_files[self.current_index].clone();
        let dest_dir = self.current_dir.join("worse");
        let dest_path = dest_dir.join(current_path.file_name().unwrap());
        
        if std::fs::rename(&current_path, &dest_path).is_ok() {
            self.last_moved = Some((current_path, dest_path, "worse".to_string()));
            self.image_files.remove(self.current_index);
            
            if self.current_index >= self.image_files.len() && !self.image_files.is_empty() {
                self.current_index = 0;
            }
            
            self.set_status_message("Moved to worse/", 3.0);
            
            // Load next image automatically
            if !self.image_files.is_empty() {
                self.load_current_image(ctx);
            }
        } else {
            self.set_status_message("Error moving image", 3.0);
        }
    }
    
    fn delete_image(&mut self, ctx: &Context) {
        if self.image_files.is_empty() {
            return;
        }
        
        let current_path = self.image_files[self.current_index].clone();
        let dest_dir = self.current_dir.join("deleted");
        let dest_path = dest_dir.join(current_path.file_name().unwrap());
        
        if std::fs::rename(&current_path, &dest_path).is_ok() {
            self.last_moved = Some((current_path, dest_path, "deleted".to_string()));
            self.image_files.remove(self.current_index);
            
            if self.current_index >= self.image_files.len() && !self.image_files.is_empty() {
                self.current_index = 0;
            }
            
            self.set_status_message("Moved to deleted/", 3.0);
            
            // Load next image automatically
            if !self.image_files.is_empty() {
                self.load_current_image(ctx);
            }
        } else {
            self.set_status_message("Error deleting image", 3.0);
        }
    }
    
    fn undo_last_move(&mut self) {
        if let Some((original_path, moved_path, dir_type)) = &self.last_moved.clone() {
            if moved_path.exists() {
                if std::fs::rename(moved_path, original_path).is_ok() {
                    // Re-insert into file list at correct position
                    let mut insert_pos = self.image_files.len();
                    for (i, file) in self.image_files.iter().enumerate() {
                        if file.file_name() > original_path.file_name() {
                            insert_pos = i;
                            break;
                        }
                    }
                    
                    self.image_files.insert(insert_pos, original_path.clone());
                    self.current_index = insert_pos;
                    self.last_moved = None;
                    
                    self.set_status_message(&format!("Restored from {}/", dir_type), 3.0);
                } else {
                    self.set_status_message("Error restoring image", 3.0);
                }
            } else {
                self.set_status_message("Image no longer exists", 3.0);
                self.last_moved = None;
            }
        } else {
            self.set_status_message("No image to undo", 3.0);
        }
    }
    
    fn update_metadata(&mut self) {
        // Check for received metadata
        if let Some(rx) = &mut self.metadata_receiver {
            while let Ok((path, metadata_result)) = rx.try_recv() {
                if let Ok(metadata) = metadata_result {
                    // Update current metadata if it's for the current image
                    if !self.image_files.is_empty() && path == self.image_files[self.current_index] {
                        self.current_metadata = Some(metadata.clone());
                    }
                    
                    // Store in cache
                    let path_str = path.to_string_lossy().to_string();
                    self.metadata_cache.lock().unwrap().put(path_str, metadata);
                }
            }
        }
    }
    
    fn toggle_composition_lines(&mut self) {
        self.show_composition = !self.show_composition;
    }
    
    fn handle_bookmark(&mut self, num: usize, ctx: &Context) {
        if let Some(bookmark) = self.bookmarks.get(&num) {
            // Restore bookmark
            if bookmark.index < self.image_files.len() {
                self.current_index = bookmark.index;
                self.zoom_factor = bookmark.zoom;
                self.image_position = bookmark.position;
                self.show_composition = bookmark.composition;
                self.composition_type = bookmark.composition_type;
                
                self.load_current_image_with_reset(ctx, false);
                
                self.set_status_message(&format!("Restored bookmark {}", num), 3.0);
            } else {
                // Invalid bookmark
                self.bookmarks.remove(&num);
                self.set_status_message(&format!("Bookmark {} removed (invalid)", num), 3.0);
            }
        } else {
            // Save bookmark
            let bookmark = BookmarkState {
                index: self.current_index,
                zoom: self.zoom_factor,
                position: self.image_position,
                composition: self.show_composition,
                composition_type: self.composition_type,
            };
            self.bookmarks.insert(num, bookmark);
            
            let filename = self.image_files.get(self.current_index)
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();
            
            self.set_status_message(
                &format!("Saved bookmark {}: {}...", num, &filename[..filename.len().min(20)]),
                3.0
            );
        }
    }
    
    fn clear_all_bookmarks(&mut self) {
        let count = self.bookmarks.len();
        self.bookmarks.clear();
        
        if count > 0 {
            self.set_status_message(&format!("Cleared {} bookmark(s)", count), 3.0);
        } else {
            self.set_status_message("No bookmarks to clear", 3.0);
        }
    }
    
    fn set_status_message(&mut self, message: &str, duration_secs: f32) {
        self.status_message = Some((
            message.to_string(),
            Instant::now() + std::time::Duration::from_secs_f32(duration_secs)
        ));
    }
    
    fn update_image_loading(&mut self, ctx: &Context) {
        // Update preloader
        self.preloader.update();
        
        // Check for completed image loads
        while let Some(result) = self.image_loader.try_recv_result() {
            if let Ok(loaded_image) = result.result {
                let path_str = result.path.to_string_lossy().to_string();
                
                // Update cache
                self.image_cache.lock().unwrap().put(path_str.clone(), loaded_image.image.clone());
                
                // If this is the current image, display it
                if result.path == self.image_files[self.current_index] {
                    self.current_image = Some(ctx.load_texture(
                        &path_str,
                        loaded_image.image,
                        egui::TextureOptions::default(),
                    ));
                }
                
                self.loading_requests.remove(&result.path);
            }
        }
    }
}

impl eframe::App for ImageViewerApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Handle keyboard input
        self.handle_keyboard_input(ctx);
        
        // Update image loading
        self.update_image_loading(ctx);
        
        // Process metadata updates
        self.update_metadata();
        
        // Update status message timeout
        if let Some((_, expiry)) = &self.status_message {
            if Instant::now() > *expiry {
                self.status_message = None;
            }
        }
        
        // Status bar
        TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if !self.image_files.is_empty() {
                    let current_file = self.image_files[self.current_index]
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy();
                    
                    let mut status_text = format!(
                        "{}/{} - {} - Zoom: {:.2}x",
                        self.current_index + 1,
                        self.image_files.len(),
                        current_file,
                        self.zoom_factor
                    );
                    
                    if self.show_composition {
                        status_text.push_str(&format!(" - Composición: {}", self.composition_type.name()));
                    }
                    
                    if !self.bookmarks.is_empty() {
                        let bookmarks: Vec<_> = self.bookmarks.keys().collect();
                        status_text.push_str(&format!(" - Bookmarks: {:?}", bookmarks));
                    }
                    
                    if let Some((message, _)) = &self.status_message {
                        status_text.push_str(&format!(" - {}", message));
                    }
                    
                    ui.label(status_text);
                }
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("LEFT/RIGHT: Navigate | Wheel: Zoom | L: Like | W: Worse | D: Delete | U: Undo | C: Composition | UP/DOWN: Change | 1-9: Bookmarks | 0: Clear");
                });
            });
        });
        
        // Metadata sidebar
        if self.show_sidebar {
            SidePanel::right("metadata_sidebar")
                .resizable(true)
                .default_width(self.sidebar_width)
                .show(ctx, |ui| {
                    ui.heading("METADATOS");
                    ui.separator();
                    
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if let Some(metadata) = &self.current_metadata {
                            // File information
                            ui.group(|ui| {
                                ui.strong("ARCHIVO");
                                ui.separator();
                                ui.label(format!("Nombre: {}", metadata.file_info.name));
                                ui.label(format!("Tamaño: {}", format_file_size(metadata.file_info.size)));
                                if let Some(created) = &metadata.file_info.created {
                                    ui.label(format!("Creado: {}", created.format("%Y-%m-%d %H:%M:%S")));
                                }
                                if let Some(modified) = &metadata.file_info.modified {
                                    ui.label(format!("Modificado: {}", modified.format("%Y-%m-%d %H:%M:%S")));
                                }
                                ui.label(format!("Ruta: {}", metadata.file_info.path));
                            });
                            
                            ui.add_space(10.0);
                            
                            // Image information
                            ui.group(|ui| {
                                ui.strong("IMAGEN");
                                ui.separator();
                                ui.label(format!("Dimensiones: {}×{}", metadata.image_info.dimensions.0, metadata.image_info.dimensions.1));
                                ui.label(format!("Megapíxeles: {:.1} MP", metadata.image_info.megapixels));
                                ui.label(format!("Relación: {}", metadata.image_info.aspect_ratio));
                                ui.label(format!("Formato: {}", metadata.image_info.format));
                            });
                            
                            ui.add_space(10.0);
                            
                            // EXIF information
                            if let Some(exif) = &metadata.exif_info {
                                ui.group(|ui| {
                                    ui.strong("EXIF");
                                    ui.separator();
                                    if let Some(make) = &exif.camera_make {
                                        ui.label(format!("Marca: {}", make));
                                    }
                                    if let Some(model) = &exif.camera_model {
                                        ui.label(format!("Modelo: {}", model));
                                    }
                                    if let Some(date) = &exif.date_taken {
                                        ui.label(format!("Fecha: {}", date.format("%Y-%m-%d %H:%M:%S")));
                                    }
                                    if let Some(shutter) = &exif.shutter_speed {
                                        ui.label(format!("Velocidad: {}", shutter));
                                    }
                                    if let Some(aperture) = &exif.aperture {
                                        ui.label(format!("Apertura: {}", aperture));
                                    }
                                    if let Some(iso) = &exif.iso {
                                        ui.label(format!("ISO: {}", iso));
                                    }
                                    if let Some(focal) = &exif.focal_length {
                                        ui.label(format!("Focal: {}", focal));
                                    }
                                    if let Some(lens) = &exif.lens_model {
                                        ui.label(format!("Lente: {}", lens));
                                    }
                                    if let Some(flash) = &exif.flash {
                                        ui.label(format!("Flash: {}", flash));
                                    }
                                    if let Some(color_space) = &exif.color_space {
                                        ui.label(format!("Espacio de color: {}", color_space));
                                    }
                                    if let Some(metering_mode) = &exif.metering_mode {
                                        ui.label(format!("Modo de medición: {}", metering_mode));
                                    }
                                    if let Some(exposure_bias) = &exif.exposure_bias_value {
                                        ui.label(format!("Compensación de exposición: {}", exposure_bias));
                                    }
                                    if let Some(aperture_value) = &exif.aperture_value {
                                        ui.label(format!("Valor de apertura: {}", aperture_value));
                                    }
                                    if let Some(sensitivity_type) = &exif.sensitivity_type {
                                        ui.label(format!("Tipo de sensibilidad: {}", sensitivity_type));
                                    }
                                    if let Some(exposure_program) = &exif.exposure_program {
                                        ui.label(format!("Programa de exposición: {}", exposure_program));
                                    }
                                    if let Some(orientation) = &exif.orientation {
                                        ui.label(format!("Orientación: {}", orientation));
                                    }
                                    if let Some(gps) = &exif.gps_location {
                                        ui.label(format!("GPS: {}", gps));
                                    }
                                    if exif.camera_make.is_none() && exif.camera_model.is_none() && exif.date_taken.is_none() && exif.shutter_speed.is_none() && exif.aperture.is_none() && exif.iso.is_none() && exif.focal_length.is_none() && exif.lens_model.is_none() && exif.gps_location.is_none() {
                                        ui.label("No hay información EXIF disponible");
                                    }
                                });
                            }
                        } else {
                            ui.label("Cargando metadatos...");
                        }
                    });
                });
        }
        
        // Main image display
        CentralPanel::default().show(ctx, |ui| {
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
                
                // Draw composition lines
                if self.show_composition {
                    let cache_key = format!(
                        "{:?}_{:.0}x{:.0}_{:.0},{:.0}",
                        self.composition_type,
                        display_size.x,
                        display_size.y,
                        image_pos.x,
                        image_pos.y
                    );
                    
                    let lines = if let Some(cached_lines) = self.composition_cache.get(&cache_key) {
                        cached_lines
                    } else {
                        let lines = generate_composition_lines(self.composition_type, image_rect);
                        self.composition_cache.put(cache_key, lines.clone());
                        lines
                    };
                    
                    for line in lines {
                        ui.painter().line_segment([line.start, line.end], line.stroke);
                    }
                }
                
                // Handle mouse input for zoom and pan
                if response.hovered() {
                    let scroll_delta = ctx.input(|i| i.raw_scroll_delta.y);
                    if scroll_delta != 0.0 {
                        let zoom_factor = if scroll_delta > 0.0 { 1.1 } else { 0.9 };
                        let old_zoom = self.zoom_factor;
                        self.zoom_factor = (self.zoom_factor * zoom_factor).clamp(0.1, 10.0);
                        
                        // Zoom towards mouse cursor
                        if let Some(pointer_pos) = response.hover_pos() {
                            let zoom_ratio = self.zoom_factor / old_zoom;
                            let relative_pos = pointer_pos - image_rect.min;
                            self.image_position += relative_pos * (1.0 - zoom_ratio);
                        }
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