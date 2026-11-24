use super::state::ImageViewerApp;
use crate::composition::generate_composition_lines;
use crate::focus::apply_focus_peaking;
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
        self.poll_folder_load(ctx);
        self.sync_thumbnail_service(ctx);
        self.receive_thumbnails(ctx);
        self.resort_when_scoring_finishes();

        // Periodic cache cleanup (every 30 seconds)
        let now = Instant::now();
        if now.duration_since(self.last_cache_cleanup).as_secs() >= 30 {
            self.cleanup_old_cache_entries();
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

        // Handle dropped files/folders (drag & drop onto window)
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if let Some(path) = dropped.into_iter().next() {
            if path.is_dir() {
                self.tags.clear_assignments();
                self.undo_stack.clear();
                self.start_folder_load(path);
            } else if let Some(parent) = path.parent() {
                self.tags.clear_assignments();
                self.undo_stack.clear();
                self.start_folder_load_selecting(parent.to_path_buf(), Some(path));
            }
        }

        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Archivo", |ui| {
                    if ui.button("Abrir carpeta…").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.tags.clear_assignments();
                            self.undo_stack.clear();
                            self.start_folder_load(path);
                        }
                        ui.close_menu();
                    }
                    if ui.button("Limpiar caché de miniaturas").clicked() {
                        self.clear_thumbnail_service();
                        if let Some(root) = crate::thumbnails::thumbs_root_dir() {
                            match crate::thumbnails::clear_cache_dir(&root) {
                                Ok(bytes) => {
                                    let freed = format_file_size(bytes);
                                    self.set_status_message(
                                        &format!("Caché de miniaturas limpiado ({freed})"),
                                        3.5,
                                    );
                                }
                                Err(e) => {
                                    self.set_status_message(
                                        &format!("No se pudo limpiar el caché: {e}"),
                                        3.5,
                                    );
                                }
                            }
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Salir").clicked() {
                        self.should_close = true;
                    }
                });
                ui.menu_button("Orden", |ui| {
                    for order in super::sorting::SortOrder::ALL {
                        if ui.radio(self.sort_order == order, order.label()).clicked() {
                            self.set_sort_order(order);
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button("Ayuda", |ui| {
                    if ui.button("Atajos de teclado (F1)").clicked() {
                        self.toggle_help();
                        ui.close_menu();
                    }
                });
            });
        });

        // Status bar
        TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if !self.image_files.is_empty() {
                    let current_file = self
                        .current_path()
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
                        status_text
                            .push_str(&format!(" - Composición: {}", self.composition_type.name()));
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

                    if let Some(score) = self.current_score() {
                        status_text.push_str(&format!(" - Nota: {:.1}", score.overall));
                    }
                    status_text.push_str(&format!(" - Orden: {}", self.sort_order.label()));
                    if let Some((done, total)) = self.grid.progress() {
                        if done < total {
                            status_text.push_str(&format!(" - Calificando: {done}/{total}"));
                        }
                    }

                    if let Some((message, _)) = &self.status_message {
                        status_text.push_str(&format!(" - {}", message));
                    }

                    ui.label(status_text);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Atajos (F1)").clicked() {
                        self.toggle_help();
                    }
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

                    ui.group(|ui| {
                        ui.strong("CALIFICACIÓN");
                        match self.current_score() {
                            Some(score) => {
                                ui.label(
                                    egui::RichText::new(format!("{:.1} / 10", score.overall))
                                        .size(22.0)
                                        .color(super::rating::score_color(score.overall)),
                                );
                                egui::Grid::new("score_grid").show(ui, |ui| {
                                    ui.label("Estética (NIMA):");
                                    ui.label(
                                        score.aesthetic.map_or("—".into(), |a| format!("{a:.2}")),
                                    );
                                    ui.end_row();
                                    ui.label("Nitidez:");
                                    ui.label(format!("{:.1}", score.sharpness));
                                    ui.end_row();
                                    ui.label("Exposición:");
                                    ui.label(format!("{:.1}", score.exposure));
                                    ui.end_row();
                                });
                            }
                            None => {
                                ui.label("Calculando…");
                            }
                        }
                    });

                    ui.group(|ui| {
                        ui.strong("HISTOGRAMA");
                        if let Some(path) = self.current_path().cloned() {
                            if let Some(hist) = self.ensure_histogram(&path) {
                                hist.draw(ui);
                            } else {
                                ui.weak("Cargando…");
                            }
                        } else {
                            ui.weak("—");
                        }
                    });

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
                                    if metadata.image_info.dimensions == (0, 0) {
                                        ui.label("—");
                                    } else {
                                        ui.label(format!(
                                            "{}×{}",
                                            metadata.image_info.dimensions.0,
                                            metadata.image_info.dimensions.1
                                        ));
                                    }
                                    ui.end_row();

                                    ui.label("Megapíxeles:");
                                    if metadata.image_info.dimensions == (0, 0) {
                                        ui.label("—");
                                    } else {
                                        ui.label(format!(
                                            "{:.1} MP",
                                            metadata.image_info.megapixels
                                        ));
                                    }
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
        if let Some(load) = &self.folder_load {
            let (dir, found, elapsed) = (load.dir.clone(), load.found(), load.started.elapsed());
            CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() / 2.0 - 40.0);
                        ui.add(egui::Spinner::new().size(32.0));
                        ui.label(format!("Cargando fotos de {}…", dir.display()));
                        ui.label(format!(
                            "{found} encontradas · {:.0} s",
                            elapsed.as_secs_f32()
                        ));
                    });
                });
            });
            self.show_help_modal(ctx);
            self.schedule_repaint(ctx);
            return;
        }

        if self.show_grid {
            CentralPanel::default().show(ctx, |ui| self.show_grid_view(ui, ctx));
            self.show_tag_modal(ctx);
            self.show_help_modal(ctx);
            self.schedule_repaint(ctx);
            return;
        }

        CentralPanel::default().show(ctx, |ui| {
            let active_texture = if self.show_focus_peaking {
                self.current_path()
                    .cloned()
                    .and_then(|path| self.ensure_focus_peaking_texture(ctx, &path))
                    .or_else(|| self.current_image.clone())
            } else {
                self.current_image.clone()
            };

            if let Some(texture) = active_texture.as_ref() {
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
                self.last_fit_scale = fit_scale;

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

                    // Shift cached local lines to current image position, with dark outline for visibility
                    let offset = image_rect.min.to_vec2();
                    let painter = ui.painter();
                    for line in lines {
                        let start = line.start + offset;
                        let end = line.end + offset;
                        let shadow = egui::Stroke::new(
                            line.stroke.width + 1.5,
                            egui::Color32::from_black_alpha(120),
                        );
                        painter.line_segment([start, end], shadow);
                        painter.line_segment([start, end], line.stroke);
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

                    if let Some(factor) = zoom_change {
                        let (new_zoom, new_pos) = zoom_around_cursor(
                            self.zoom_factor,
                            factor,
                            image_pos,
                            ui.min_rect().min,
                            response.hover_pos(),
                        );
                        self.zoom_factor = new_zoom;
                        self.image_position = new_pos;
                    }
                }

                // Handle double-click to toggle 100% (1:1) zoom centered on click point
                if response.double_clicked() {
                    if (self.zoom_factor - 1.0).abs() > 0.05 {
                        self.zoom_factor = 1.0;
                        self.image_position = egui::Vec2::ZERO;
                        self.set_status_message("Zoom: ajustar", 1.5);
                    } else if fit_scale > f32::EPSILON {
                        let target_zoom = (1.0 / fit_scale).clamp(1.0, 10.0);
                        let factor = target_zoom / self.zoom_factor;
                        let (new_zoom, new_pos) = zoom_around_cursor(
                            self.zoom_factor,
                            factor,
                            image_pos,
                            ui.min_rect().min,
                            response.interact_pointer_pos(),
                        );
                        self.zoom_factor = new_zoom;
                        self.image_position = new_pos;
                        self.set_status_message("Zoom: 100% (1:1)", 1.5);
                    }
                }

                // Handle dragging
                if response.dragged() {
                    self.image_position += response.drag_delta();
                }
            } else {
                ui.centered_and_justified(|ui| {
                    if self.image_files.is_empty() {
                        ui.label("No se encontraron imágenes en esta carpeta");
                    } else if let Some(error) = &self.current_load_error {
                        ui.vertical_centered(|ui| {
                            ui.colored_label(
                                egui::Color32::from_rgb(230, 90, 90),
                                "No se pudo cargar la imagen",
                            );
                            ui.weak(error);
                        });
                    } else {
                        ui.horizontal_centered(|ui| {
                            ui.spinner();
                            ui.label("Cargando imagen…");
                        });
                    }
                });
            }
        });

        self.show_tag_modal(ctx);
        self.show_help_modal(ctx);
        self.schedule_repaint(ctx);
    }
}

/// Computes `(zoom_factor, image_position)` zooming smoothly around `pointer_pos`
/// starting from the currently displayed `current_display_pos` (avoiding jumps from centered view).
pub(crate) fn zoom_around_cursor(
    old_zoom: f32,
    factor: f32,
    current_display_pos: egui::Vec2,
    panel_min: egui::Pos2,
    pointer_pos: Option<egui::Pos2>,
) -> (f32, egui::Vec2) {
    let new_zoom = (old_zoom * factor).clamp(0.1, 10.0);
    let mut new_pos = current_display_pos;
    if let Some(ptr) = pointer_pos {
        let zoom_ratio = new_zoom / old_zoom;
        let image_origin = panel_min + current_display_pos;
        let relative_pos = ptr - image_origin;
        new_pos += relative_pos * (1.0 - zoom_ratio);
    }
    (new_zoom, new_pos)
}

impl ImageViewerApp {
    /// Returns a cached focus-peaking texture for `path`, rebuilding only when the path or
    /// cached image resolution changes.
    pub(crate) fn ensure_focus_peaking_texture(
        &mut self,
        ctx: &Context,
        path: &std::path::Path,
    ) -> Option<egui::TextureHandle> {
        let path_str = path.to_string_lossy();
        let color_img = self.image_cache.get(&path_str)?;
        let size = color_img.size;
        let hit = self
            .focus_peaking_cache
            .as_ref()
            .is_some_and(|(cached_path, cached_size, _)| {
                cached_path == path && *cached_size == size
            });
        if !hit {
            const THRESHOLD: f32 = 30.0;
            let peaked_img = apply_focus_peaking(&color_img, THRESHOLD);
            let texture = ctx.load_texture(
                format!("{}_focus_{}x{}", path_str, size[0], size[1]),
                peaked_img,
                egui::TextureOptions::default(),
            );
            self.focus_peaking_cache = Some((path.to_path_buf(), size, texture));
        }
        self.focus_peaking_cache
            .as_ref()
            .map(|(_, _, tex)| tex.clone())
    }

    pub(crate) fn ensure_histogram(
        &mut self,
        path: &std::path::Path,
    ) -> Option<super::histogram::Histogram> {
        let path_str = path.to_string_lossy();
        let color_img = self.image_cache.get(&path_str)?;
        let size = color_img.size;
        let hit = self
            .histogram_cache
            .as_ref()
            .is_some_and(|(cached_path, cached_size, _)| {
                cached_path == path && *cached_size == size
            });
        if !hit {
            let hist = super::histogram::Histogram::from_color_image(&color_img);
            self.histogram_cache = Some((path.to_path_buf(), size, hist));
        }
        self.histogram_cache.as_ref().map(|(_, _, h)| h.clone())
    }

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
        if self.is_loading() {
            ctx.request_repaint_after(Duration::from_millis(100));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_around_cursor_preserves_point_under_cursor_from_centered_start() {
        let panel_min = egui::pos2(0.0, 24.0);
        let centered_pos = egui::vec2(200.0, 100.0);
        let pointer = egui::pos2(500.0, 300.0);
        let (new_zoom, new_pos) =
            zoom_around_cursor(1.0, 2.0, centered_pos, panel_min, Some(pointer));
        assert!((new_zoom - 2.0).abs() < 1e-5);
        // Before zoom: pointer relative to image origin = (500-200, 300-124) = (300, 176).
        // After 2x zoom: image origin moves so same image point stays under pointer.
        let before_rel = pointer - (panel_min + centered_pos);
        let after_rel = pointer - (panel_min + new_pos);
        assert!((after_rel.x - before_rel.x * 2.0).abs() < 1e-3);
        assert!((after_rel.y - before_rel.y * 2.0).abs() < 1e-3);
    }

    #[test]
    fn focus_peaking_cache_reuses_texture_until_path_or_size_changes() {
        let path = std::path::PathBuf::from("/tmp/a.jpg");
        let mut app = super::super::state::test_app(vec![path.clone()], 0);
        let ctx = Context::default();
        let img1 = std::sync::Arc::new(egui::ColorImage::new([16, 16], egui::Color32::GRAY));
        app.image_cache
            .put_if_better(path.to_string_lossy().to_string(), img1, Some(1024));

        let id1 = app.ensure_focus_peaking_texture(&ctx, &path).unwrap().id();
        let id2 = app.ensure_focus_peaking_texture(&ctx, &path).unwrap().id();
        assert_eq!(id1, id2, "must reuse cached texture on subsequent frames");

        // Upgrade resolution -> cache key size changes -> rebuilds texture.
        let img2 = std::sync::Arc::new(egui::ColorImage::new([32, 32], egui::Color32::GRAY));
        app.image_cache
            .put_if_better(path.to_string_lossy().to_string(), img2, Some(2048));
        let id3 = app.ensure_focus_peaking_texture(&ctx, &path).unwrap().id();
        assert_ne!(id1, id3, "resolution upgrade must rebuild focus texture");
    }
}
