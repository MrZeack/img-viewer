use super::state::ImageViewerApp;
use crate::composition::generate_composition_lines;
use crate::focus::apply_focus_peaking;
use crate::image_loader::get_image_files;
use crate::metadata_simple::format_file_size;
use eframe::egui::{self, CentralPanel, Context, SidePanel, TopBottomPanel};
use std::time::Duration;
use std::time::Instant;

impl eframe::App for ImageViewerApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        if self.should_close {
            // eframe 0.28: close via viewport command
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        // Handle keyboard input
        self.handle_keyboard_input(ctx);

        // Process events
        self.process_events(ctx);

        // Periodic cache cleanup (every 30 seconds)
        let now = Instant::now();
        if now.duration_since(self.last_cache_cleanup).as_secs() >= 30 {
            self.cleanup_old_cache_entries();
            self.cleanup_cache_by_usage();
            self.last_cache_cleanup = now;
        }

        // Cleanup distant entries less frequently (every 2 minutes)
        if now.duration_since(self.last_distant_cleanup).as_secs() >= 120 {
            self.cleanup_distant_cache_entries();
            self.last_distant_cleanup = now;
        }

        // Update status message timeout
        if let Some((_, expiry)) = &self.status_message {
            if Instant::now() > *expiry {
                self.status_message = None;
            }
        }

        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open Folder...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.current_dir = path;
                            self.image_files =
                                get_image_files(&self.current_dir, self.sort_newest_first)
                                    .unwrap_or_default();
                            self.current_index = 0;
                            self.tags.clear_assignments();
                            self.last_moved = None;
                            self.current_image = None;
                            self.current_metadata = None;
                            if !self.image_files.is_empty() {
                                self.load_current_image(ctx);
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("Exit").clicked() {
                        self.should_close = true;
                    }
                });
            });
        });

        // Status bar
        TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if !self.image_files.is_empty() {
                    let current_file = self.current_path()
                        .and_then(|path| path.file_name())
                        .unwrap_or_default()
                        .to_string_lossy();

                    let mut status_text = format!(
                        "{}/{} - {} - Zoom: {:.2}x",
                        self.current_index + 1,
                        self.image_files.len(),
                        current_file,
                        self.zoom_factor
                    );

                    if self.rotation != 0 {
                        status_text.push_str(&format!(" - Rotación: {}°", self.rotation * 90));
                    }

                    if self.show_composition {
                        status_text.push_str(&format!(" - Composición: {}", self.composition_type.name()));
                    }

                    if self.show_focus_peaking {
                        status_text.push_str(" - Focus: ON");
                    }

                    if self.quality_multiplier == 2 {
                        status_text.push_str(" - Calidad: 3x");
                    } else if self.quality_multiplier == 3 {
                        status_text.push_str(" - Calidad: MAX");
                    }

                    if let Some(slot) = self.current_path().and_then(|p| self.tags.tag_of(p)) {
                        let name = self.tags.name(slot).unwrap_or_default();
                        status_text.push_str(&format!(" - Tag {}: {}", slot, name));
                    }

                    if self.show_grid {
                        if let Some((done, total)) = self.grid.progress() {
                            if done < total {
                                status_text.push_str(&format!(" - Miniaturas: {done}/{total}"));
                            }
                        }
                    }

                    if let Some((message, _)) = &self.status_message {
                        status_text.push_str(&format!(" - {}", message));
                    }

                    ui.label(status_text);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.show_grid {
                        ui.label("Flechas/RePág/AvPág: Mover | Enter/G/Esc/Doble clic: Abrir | +/-/Pinch: Tamaño | L/W/D/U | 1-9/0: Tag | M: Mover tags");
                        return;
                    }
                    ui.label("G: Cuadrícula | LEFT/RIGHT: Navigate | Wheel: Zoom | L: Like | W: Worse | D: Delete | U: Undo | C: Composición | Shift+C: Off | F: Focus | H: Calidad | UP/DOWN: Tipo | R/Shift+R: Rotate | 1-9: Tag | 0: Quitar tag | T: Tags | M: Mover tags");
                });
            });
        });

        // Metadata sidebar
        if self.show_sidebar && !self.show_grid {
            SidePanel::right("metadata_sidebar")
                .resizable(true)
                .default_width(self.sidebar_width)
                .show(ctx, |ui| {
                    let tag_counts = self.tags.counts(&self.image_files);
                    if !tag_counts.is_empty() {
                        ui.group(|ui| {
                            ui.strong("TAGS (M: mover)");
                            for (slot, count) in &tag_counts {
                                let name = self.tags.name(*slot).unwrap_or_default();
                                ui.label(format!("{slot} · {name}: {count}"));
                            }
                        });
                    }

                    ui.heading("METADATOS");
                    ui.separator();

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if let Some(metadata) = &self.current_metadata {
                            // File information
                            ui.group(|ui| {
                                ui.strong("ARCHIVO");
                                ui.separator();
                                egui::Grid::new("file_info_grid").show(ui, |ui| {
                                    ui.label("Nombre:");
                                    ui.label(&metadata.file_info.name);
                                    ui.end_row();

                                    ui.label("Tamaño:");
                                    ui.label(format_file_size(metadata.file_info.size));
                                    ui.end_row();

                                    if let Some(created) = &metadata.file_info.created {
                                        ui.label("Creado:");
                                        ui.label(created.format("%Y-%m-%d %H:%M:%S").to_string());
                                        ui.end_row();
                                    }
                                    if let Some(modified) = &metadata.file_info.modified {
                                        ui.label("Modificado:");
                                        ui.label(modified.format("%Y-%m-%d %H:%M:%S").to_string());
                                        ui.end_row();
                                    }
                                    ui.label("Ruta:");
                                    ui.label(&metadata.file_info.path);
                                    ui.end_row();
                                });
                            });

                            ui.add_space(10.0);

                            // Image information
                            ui.group(|ui| {
                                ui.strong("IMAGEN");
                                ui.separator();
                                egui::Grid::new("image_info_grid").show(ui, |ui| {
                                    ui.label("Dimensiones:");
                                    ui.label(format!(
                                        "{}×{}",
                                        metadata.image_info.dimensions.0,
                                        metadata.image_info.dimensions.1
                                    ));
                                    ui.end_row();

                                    ui.label("Megapíxeles:");
                                    ui.label(format!("{:.1} MP", metadata.image_info.megapixels));
                                    ui.end_row();

                                    ui.label("Relación:");
                                    ui.label(&metadata.image_info.aspect_ratio);
                                    ui.end_row();

                                    ui.label("Formato:");
                                    ui.label(&metadata.image_info.format);
                                    ui.end_row();
                                });
                            });

                            ui.add_space(10.0);

                            // EXIF information
                            if let Some(exif) = &metadata.exif_info {
                                ui.group(|ui| {
                                    ui.strong("EXIF");
                                    ui.separator();
                                    egui::Grid::new("exif_info_grid").show(ui, |ui| {
                                        if let Some(make) = &exif.camera_make {
                                            ui.label("Marca:");
                                            ui.label(make);
                                            ui.end_row();
                                        }
                                        if let Some(model) = &exif.camera_model {
                                            ui.label("Modelo:");
                                            ui.label(model);
                                            ui.end_row();
                                        }
                                        if let Some(date) = &exif.date_taken {
                                            ui.label("Fecha:");
                                            ui.label(date.format("%Y-%m-%d %H:%M:%S").to_string());
                                            ui.end_row();
                                        }
                                        if let Some(shutter) = &exif.shutter_speed {
                                            ui.label("Velocidad:");
                                            ui.label(shutter);
                                            ui.end_row();
                                        }
                                        if let Some(aperture) = &exif.aperture {
                                            ui.label("Apertura:");
                                            ui.label(aperture);
                                            ui.end_row();
                                        }
                                        if let Some(iso) = &exif.iso {
                                            ui.label("ISO:");
                                            ui.label(iso.to_string());
                                            ui.end_row();
                                        }
                                        if let Some(focal) = &exif.focal_length {
                                            ui.label("Focal:");
                                            ui.label(focal);
                                            ui.end_row();
                                        }
                                        if let Some(lens) = &exif.lens_model {
                                            ui.label("Lente:");
                                            ui.label(lens);
                                            ui.end_row();
                                        }
                                        if let Some(flash) = &exif.flash {
                                            ui.label("Flash:");
                                            ui.label(flash);
                                            ui.end_row();
                                        }
                                        if let Some(color_space) = &exif.color_space {
                                            ui.label("Espacio de color:");
                                            ui.label(color_space);
                                            ui.end_row();
                                        }
                                        if let Some(metering_mode) = &exif.metering_mode {
                                            ui.label("Modo de medición:");
                                            ui.label(metering_mode);
                                            ui.end_row();
                                        }
                                        if let Some(exposure_bias) = &exif.exposure_bias_value {
                                            ui.label("Comp. exposición:");
                                            ui.label(exposure_bias);
                                            ui.end_row();
                                        }
                                        if let Some(aperture_value) = &exif.aperture_value {
                                            ui.label("Valor de apertura:");
                                            ui.label(aperture_value);
                                            ui.end_row();
                                        }
                                        if let Some(sensitivity_type) = &exif.sensitivity_type {
                                            ui.label("Tipo de sensibilidad:");
                                            ui.label(sensitivity_type);
                                            ui.end_row();
                                        }
                                        if let Some(exposure_program) = &exif.exposure_program {
                                            ui.label("Programa Exp.:");
                                            ui.label(exposure_program);
                                            ui.end_row();
                                        }
                                        if let Some(orientation) = &exif.orientation {
                                            ui.label("Orientación:");
                                            ui.label(orientation);
                                            ui.end_row();
                                        }
                                        if let Some(gps) = &exif.gps_location {
                                            ui.label("GPS:");
                                            ui.label(gps);
                                            ui.end_row();
                                        }
                                        if exif.camera_make.is_none()
                                            && exif.camera_model.is_none()
                                            && exif.date_taken.is_none()
                                            && exif.shutter_speed.is_none()
                                            && exif.aperture.is_none()
                                            && exif.iso.is_none()
                                            && exif.focal_length.is_none()
                                            && exif.lens_model.is_none()
                                            && exif.gps_location.is_none()
                                        {
                                            ui.label("No hay información EXIF disponible");
                                            ui.end_row();
                                        }
                                    });
                                });
                            }
                        } else {
                            ui.label("Cargando metadatos...");
                        }
                    });
                });
        }

        // Main image display
        if self.show_grid {
            CentralPanel::default().show(ctx, |ui| self.show_grid_view(ui, ctx));
            self.show_tag_modal(ctx);
            self.schedule_repaint(ctx);
            return;
        }

        CentralPanel::default().show(ctx, |ui| {
            let focus_texture_storage;
            let current_path_str = self.current_path().map(|p| p.to_string_lossy().to_string());

            let active_texture = if self.show_focus_peaking {
                if let Some(path_str) = &current_path_str {
                    if let Some(color_img) = self.image_cache.get(path_str) {
                        let threshold = 30.0;
                        let peaked_img = apply_focus_peaking(&color_img, threshold);
                        focus_texture_storage = ctx.load_texture(
                            format!("{}_focus", path_str),
                            peaked_img,
                            egui::TextureOptions::default(),
                        );
                        Some(&focus_texture_storage)
                    } else {
                        self.current_image.as_ref()
                    }
                } else {
                    self.current_image.as_ref()
                }
            } else {
                self.current_image.as_ref()
            };

            if let Some(texture) = active_texture {
                let image_size = texture.size_vec2();
                let available_size = ui.available_size();

                // Swap dimensions for display sizing calculation if rotated 90 or 270 degrees
                let visual_image_size = if self.rotation % 2 != 0 {
                    egui::vec2(image_size.y, image_size.x)
                } else {
                    image_size
                };

                // Calculate fit-to-window scale
                let scale_x = available_size.x / visual_image_size.x;
                let scale_y = available_size.y / visual_image_size.y;
                let fit_scale = scale_x.min(scale_y);

                // Apply zoom
                let display_scale = fit_scale * self.zoom_factor;
                let display_size = visual_image_size * display_scale;

                // Center image if smaller than available area
                let mut image_pos = self.image_position;
                if display_size.x < available_size.x {
                    image_pos.x = (available_size.x - display_size.x) * 0.5;
                }
                if display_size.y < available_size.y {
                    image_pos.y = (available_size.y - display_size.y) * 0.5;
                }

                // Draw image
                let image_rect =
                    egui::Rect::from_min_size(ui.min_rect().min + image_pos, display_size);

                let response = ui.allocate_rect(
                    ui.available_rect_before_wrap(),
                    egui::Sense::click_and_drag(),
                );

                // Map UV coordinates based on 90-degree step rotations
                let (uv_tl, uv_tr, uv_br, uv_bl) = match self.rotation {
                    1 => (
                        egui::pos2(0.0, 1.0),
                        egui::pos2(0.0, 0.0),
                        egui::pos2(1.0, 0.0),
                        egui::pos2(1.0, 1.0),
                    ),
                    2 => (
                        egui::pos2(1.0, 1.0),
                        egui::pos2(0.0, 1.0),
                        egui::pos2(0.0, 0.0),
                        egui::pos2(1.0, 0.0),
                    ),
                    3 => (
                        egui::pos2(1.0, 0.0),
                        egui::pos2(1.0, 1.0),
                        egui::pos2(0.0, 1.0),
                        egui::pos2(0.0, 0.0),
                    ),
                    _ => (
                        egui::pos2(0.0, 0.0),
                        egui::pos2(1.0, 0.0),
                        egui::pos2(1.0, 1.0),
                        egui::pos2(0.0, 1.0),
                    ),
                };

                let mut mesh = egui::Mesh::with_texture(texture.id());
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: image_rect.left_top(),
                    uv: uv_tl,
                    color: egui::Color32::WHITE,
                });
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: image_rect.right_top(),
                    uv: uv_tr,
                    color: egui::Color32::WHITE,
                });
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: image_rect.right_bottom(),
                    uv: uv_br,
                    color: egui::Color32::WHITE,
                });
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: image_rect.left_bottom(),
                    uv: uv_bl,
                    color: egui::Color32::WHITE,
                });

                mesh.indices.push(0);
                mesh.indices.push(1);
                mesh.indices.push(2);
                mesh.indices.push(0);
                mesh.indices.push(2);
                mesh.indices.push(3);

                ui.painter().add(egui::Shape::mesh(mesh));

                // Draw composition lines
                if self.show_composition {
                    let unrotated_display_size = image_size * display_scale;
                    // Cache by type + size + rotation
                    let cache_key = format!(
                        "{:?}_{:.0}x{:.0}_r{}",
                        self.composition_type,
                        unrotated_display_size.x,
                        unrotated_display_size.y,
                        self.rotation
                    );

                    let lines = if let Some(cached_lines) = self.composition_cache.get(&cache_key) {
                        cached_lines
                    } else {
                        let local_rect =
                            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), unrotated_display_size);
                        let mut lines =
                            generate_composition_lines(self.composition_type, local_rect);

                        // Rotate lines based on self.rotation (0: 0, 1: 90, 2: 180, 3: 270)
                        let w = unrotated_display_size.x;
                        let h = unrotated_display_size.y;

                        for line in &mut lines {
                            for pt in &mut [&mut line.start, &mut line.end] {
                                let (x, y) = (pt.x, pt.y);
                                match self.rotation {
                                    1 => {
                                        pt.x = h - y;
                                        pt.y = x;
                                    }
                                    2 => {
                                        pt.x = w - x;
                                        pt.y = h - y;
                                    }
                                    3 => {
                                        pt.x = y;
                                        pt.y = w - x;
                                    }
                                    _ => {}
                                }
                            }
                        }

                        self.composition_cache.put(cache_key, lines.clone());
                        lines
                    };

                    // Shift cached local lines to current image position
                    let offset = image_rect.min.to_vec2();
                    for line in lines {
                        let start = line.start + offset;
                        let end = line.end + offset;
                        ui.painter().line_segment([start, end], line.stroke);
                    }
                }

                // Handle mouse input for zoom and pan
                if response.hovered() {
                    // Mouse wheel zoom
                    let scroll_delta = ctx.input(|i| i.raw_scroll_delta.y);
                    // Trackpad pinch zoom
                    let pinch_zoom = ctx.input(|i| i.zoom_delta());

                    let mut zoom_change: Option<f32> = None;
                    if scroll_delta != 0.0 {
                        zoom_change = Some(if scroll_delta > 0.0 { 1.1 } else { 0.9 });
                    } else if (pinch_zoom - 1.0).abs() > f32::EPSILON {
                        zoom_change = Some(pinch_zoom);
                    }

                    if let Some(zoom_factor) = zoom_change {
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

        self.show_tag_modal(ctx);
        self.schedule_repaint(ctx);
    }
}

impl ImageViewerApp {
    /// Repaint throttle: only when needed
    fn schedule_repaint(&self, ctx: &Context) {
        // El contador de indexación avanza sin eventos de UI: refrescarlo con calma.
        if self.show_grid
            && self
                .grid
                .progress()
                .is_some_and(|(done, total)| done < total)
        {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        let mut needs_repaint = false;
        if let Some((_, expiry)) = &self.status_message {
            if Instant::now() <= *expiry {
                needs_repaint = true;
            }
        }
        if !self.loading_requests.is_empty() {
            needs_repaint = true;
        }
        if needs_repaint {
            // Aim ~60 FPS while active, or slower for status-only updates
            let interval = if !self.loading_requests.is_empty() {
                16
            } else {
                100
            };
            ctx.request_repaint_after(Duration::from_millis(interval));
        }
    }
}
