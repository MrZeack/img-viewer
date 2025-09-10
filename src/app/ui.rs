use super::state::ImageViewerApp;
use eframe::egui::{self, CentralPanel, Context, SidePanel, TopBottomPanel};
use crate::image_loader::get_image_files;
use crate::metadata_simple::format_file_size;
use crate::composition::generate_composition_lines;
use std::time::Instant;

impl eframe::App for ImageViewerApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Handle keyboard input
        self.handle_keyboard_input(ctx);

        // Process events
        self.process_events(ctx);

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
                            self.image_files = get_image_files(&self.current_dir).unwrap_or_default();
                            self.current_index = 0;
                            self.current_image = None;
                            self.current_metadata = None;
                            if !self.image_files.is_empty() {
                                self.load_current_image(ctx);
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("Exit").clicked() {
                        std::process::exit(0);
                    }
                });
            });
        });

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
                                    ui.label(format!("{}×{}", metadata.image_info.dimensions.0, metadata.image_info.dimensions.1));
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
                                        if exif.camera_make.is_none() && exif.camera_model.is_none() && exif.date_taken.is_none() && exif.shutter_speed.is_none() && exif.aperture.is_none() && exif.iso.is_none() && exif.focal_length.is_none() && exif.lens_model.is_none() && exif.gps_location.is_none() {
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
