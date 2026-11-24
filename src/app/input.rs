use super::state::{ImageViewerApp, LastMove};
use eframe::egui::{Context, Key};
use std::fs;
use std::io;
use std::path::Path;

impl ImageViewerApp {
    pub(crate) fn handle_keyboard_input(&mut self, ctx: &Context) {
        // A modal or a focused text field owns the keyboard: typing must not trigger moves.
        if self.tag_modal.is_some() || ctx.wants_keyboard_input() {
            return;
        }
        if self.handle_help_keys(ctx) || self.is_loading() {
            return;
        }
        if self.show_grid {
            if ctx.input(|i| i.key_pressed(Key::S)) {
                self.set_sort_order(self.sort_order.next());
            } else if !self.handle_grid_keys(ctx) {
                self.handle_move_keys(ctx);
            }
            self.handle_tag_keys(ctx);
            return;
        }

        if ctx.input(|i| i.key_pressed(Key::G)) {
            self.open_grid();
        } else if ctx.input(|i| i.key_pressed(Key::S)) {
            self.set_sort_order(self.sort_order.next());
        } else if ctx.input(|i| i.key_pressed(Key::ArrowLeft)) {
            self.previous_image(ctx);
        } else if ctx.input(|i| i.key_pressed(Key::ArrowRight)) {
            self.next_image(ctx);
        } else if self.handle_move_keys(ctx) {
        } else if ctx.input(|i| i.key_pressed(Key::C)) {
            let shift = ctx.input(|i| i.modifiers.shift);
            if shift {
                self.show_composition = false;
                self.set_status_message("Composición desactivada", 2.0);
            } else if !self.show_composition {
                self.show_composition = true;
                self.set_status_message(
                    &format!("Composición: {}", self.composition_type.name()),
                    2.0,
                );
            } else {
                self.composition_type = self.composition_type.next();
                self.composition_cache.clear();
                self.set_status_message(
                    &format!("Composición: {}", self.composition_type.name()),
                    2.0,
                );
            }
            self.save_view_config();
        } else if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
            if self.show_composition {
                self.composition_type = self.composition_type.next();
                self.composition_cache.clear();
                self.save_view_config();
            }
        } else if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
            if self.show_composition {
                self.composition_type = self.composition_type.prev();
                self.composition_cache.clear();
                self.save_view_config();
            }
        } else if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.should_close = true;
        } else if ctx.input(|i| i.key_pressed(Key::R)) {
            self.rotation_manually_set = true;
            let shift = ctx.input(|i| i.modifiers.shift);
            if shift {
                self.rotation = (self.rotation - 1).rem_euclid(4);
                self.set_status_message("Rotada 90° antihorario", 1.5);
            } else {
                self.rotation = (self.rotation + 1).rem_euclid(4);
                self.set_status_message("Rotada 90° horario", 1.5);
            }
        } else if ctx.input(|i| i.key_pressed(Key::F)) {
            self.show_focus_peaking = !self.show_focus_peaking;
            if self.show_focus_peaking {
                self.set_status_message("Focus peaking: activado", 2.0);
            } else {
                self.focus_peaking_cache = None;
                self.set_status_message("Focus peaking: desactivado", 2.0);
            }
            self.save_view_config();
        } else if ctx.input(|i| i.key_pressed(Key::H)) {
            self.quality_multiplier = if self.quality_multiplier >= 3 {
                1
            } else {
                self.quality_multiplier + 1
            };
            let msg = match self.quality_multiplier {
                1 => "Calidad: Normal",
                2 => "Calidad: Alta (3x)",
                3 => "Calidad: Original (Máxima)",
                _ => "Calidad: Normal",
            };
            self.set_status_message(msg, 2.5);
            self.load_current_image_with_reset(ctx, false);
        } else if ctx.input(|i| i.key_pressed(Key::I) || i.key_pressed(Key::Tab)) {
            self.show_sidebar = !self.show_sidebar;
            self.set_status_message(
                if self.show_sidebar {
                    "Panel lateral visible"
                } else {
                    "Panel lateral oculto"
                },
                1.5,
            );
        } else if ctx.input(|i| i.key_pressed(Key::Z)) {
            self.toggle_actual_size_zoom();
        } else if ctx.input(|i| i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals)) {
            self.zoom_factor = (self.zoom_factor * 1.25).clamp(0.1, 10.0);
        } else if ctx.input(|i| i.key_pressed(Key::Minus)) {
            self.zoom_factor = (self.zoom_factor / 1.25).clamp(0.1, 10.0);
        } else if ctx.input(|i| i.key_pressed(Key::Space)) {
            self.zoom_factor = 1.0;
            self.image_position = egui::Vec2::ZERO;
            self.set_status_message("Zoom: ajustar", 1.5);
        }

        self.handle_tag_keys(ctx);
    }

    pub(crate) fn toggle_actual_size_zoom(&mut self) {
        if (self.zoom_factor - 1.0).abs() > 0.05 {
            self.zoom_factor = 1.0;
            self.image_position = egui::Vec2::ZERO;
            self.set_status_message("Zoom: ajustar", 1.5);
        } else {
            let one_to_one = if self.last_fit_scale > f32::EPSILON {
                (1.0 / self.last_fit_scale).clamp(1.0, 10.0)
            } else {
                2.0
            };
            self.zoom_factor = one_to_one;
            self.set_status_message("Zoom: 100% (1:1)", 1.5);
        }
    }

    /// L/W/D/U; devuelve `true` si consumió la tecla.
    fn handle_move_keys(&mut self, ctx: &Context) -> bool {
        if ctx.input(|i| i.key_pressed(Key::L)) {
            self.move_current_to(ctx, "best");
        } else if ctx.input(|i| i.key_pressed(Key::W)) {
            self.move_current_to(ctx, "worse");
        } else if ctx.input(|i| i.key_pressed(Key::D)) {
            self.move_current_to(ctx, "deleted");
        } else if ctx.input(|i| i.key_pressed(Key::U)) {
            self.undo_last_move(ctx);
        } else {
            return false;
        }
        true
    }

    /// Tags: 1-9 asigna, 0 quita, T edita nombres, M mueve las etiquetadas.
    fn handle_tag_keys(&mut self, ctx: &Context) {
        for slot in 1..=9u8 {
            if let Some(key) = Key::from_name(&slot.to_string()) {
                if ctx.input(|i| i.key_pressed(key)) {
                    self.tag_key(slot);
                }
            }
        }

        if ctx.input(|i| i.key_pressed(Key::Num0)) {
            self.untag_current();
        } else if ctx.input(|i| i.key_pressed(Key::T)) {
            self.open_tag_manager();
        } else if ctx.input(|i| i.key_pressed(Key::M)) {
            self.open_move_confirm();
        }
    }

    fn previous_image(&mut self, ctx: &Context) {
        if !self.image_files.is_empty() {
            self.quality_multiplier = 1;
            self.current_index =
                (self.current_index + self.image_files.len() - 1) % self.image_files.len();
            self.load_current_image(ctx);
        }
    }

    fn next_image(&mut self, ctx: &Context) {
        if !self.image_files.is_empty() {
            self.quality_multiplier = 1;
            self.current_index = (self.current_index + 1) % self.image_files.len();
            self.load_current_image(ctx);
        }
    }

    pub(crate) fn move_current_to(&mut self, ctx: &Context, subdir: &str) {
        if self.image_files.is_empty() {
            return;
        }

        let current_path = self.image_files[self.current_index].clone();
        let Some(file_name) = current_path.file_name() else {
            self.set_status_message("Nombre de archivo inválido", 3.0);
            return;
        };
        let dest_path = self.current_dir.join(subdir).join(file_name);
        if dest_path.exists() {
            let preview = truncate_preview(&file_name.to_string_lossy(), 30);
            self.set_status_message(&format!("Ya existe en {subdir}/: {preview}"), 3.5);
            return;
        }

        if move_with_fallback(&current_path, &dest_path).is_err() {
            self.set_status_message(&format!("Error al mover a {subdir}/"), 3.0);
            return;
        }

        self.clean_cache_for_moved_image(&current_path);
        let tag = self.tags.untag(&current_path);
        let prev_index = self.current_index;
        self.undo_stack.push(LastMove::Single {
            source: current_path,
            dest: dest_path,
            label: subdir.to_string(),
            index: prev_index,
            tag,
        });
        self.image_files.remove(self.current_index);

        if self.current_index >= self.image_files.len() {
            self.current_index = self.image_files.len().saturating_sub(1);
        }

        self.set_status_message(&format!("Movida a {subdir}/"), 3.0);

        if self.show_grid {
            self.grid.scroll_to_selected = true;
            if self.image_files.is_empty() {
                self.current_image = None;
                self.current_metadata = None;
            }
        } else if !self.image_files.is_empty() {
            self.load_current_image(ctx);
        } else {
            self.current_image = None;
            self.current_metadata = None;
        }
    }

    pub(crate) fn undo_last_move(&mut self, ctx: &Context) {
        match self.undo_stack.pop() {
            Some(LastMove::Single {
                source,
                dest,
                label,
                index,
                tag,
            }) => {
                if !dest.exists() {
                    self.set_status_message("La imagen ya no existe en destino", 3.0);
                } else if source.exists() {
                    self.set_status_message("No se restauró: ya existe un archivo en origen", 3.5);
                } else if move_with_fallback(&dest, &source).is_ok() {
                    if let Some(slot) = tag {
                        self.tags.assign(&source, slot);
                    }
                    apply_undo_state(self, source, index);
                    if self.show_grid {
                        self.grid.scroll_to_selected = true;
                    } else {
                        self.load_current_image(ctx);
                    }
                    self.set_status_message(&format!("Restaurada desde {label}/"), 3.0);
                } else {
                    self.undo_stack.push(LastMove::Single {
                        source,
                        dest,
                        label,
                        index,
                        tag,
                    });
                    self.set_status_message("Error al restaurar la imagen", 3.0);
                }
            }
            Some(LastMove::Batch(moves)) => self.undo_tag_move(ctx, moves),
            None => self.set_status_message("No hay movimientos para deshacer", 3.0),
        }
    }
}

pub(crate) fn move_with_fallback(src: &Path, dest: &Path) -> Result<(), io::Error> {
    if let Some(parent) = dest.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!(
                "Failed to create parent directory for {}: {}",
                dest.display(),
                e
            );
            return Err(e);
        }
    }
    match fs::rename(src, dest) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Cross-device or permission issues: try copy + remove
            fs::copy(src, dest)
                .and_then(|_| fs::remove_file(src))
                .map(|_| ())
                .or(Err(e))
        }
    }
}

pub(crate) fn truncate_preview(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn apply_undo_state(
    app: &mut ImageViewerApp,
    original_path: std::path::PathBuf,
    prev_index: usize,
) -> usize {
    let insert_pos = prev_index.min(app.image_files.len());
    app.image_files.insert(insert_pos, original_path);
    app.current_index = insert_pos;
    app.current_image = None;
    app.current_metadata = None;
    insert_pos
}

#[cfg(test)]
mod tests {
    use super::apply_undo_state;
    use super::truncate_preview;
    use std::path::PathBuf;

    #[test]
    fn truncate_preview_handles_multibyte_text() {
        let text = "niño-東京-café";
        let truncated = truncate_preview(text, 6);
        assert_eq!(truncated, "niño-東");
    }

    #[test]
    fn truncate_preview_keeps_short_text_intact() {
        let text = "abc";
        assert_eq!(truncate_preview(text, 20), "abc");
    }

    #[test]
    fn apply_undo_state_restores_previous_index_and_clears_stale_view() {
        let mut app = super::super::state::test_app(
            vec![PathBuf::from("/tmp/a.jpg"), PathBuf::from("/tmp/c.jpg")],
            1,
        );
        app.current_metadata = Some(crate::metadata_simple::ImageMetadata {
            file_info: crate::metadata_simple::FileInfo {
                name: "c.jpg".to_string(),
                size: 1,
                created: None,
                modified: None,
                path: "/tmp".to_string(),
            },
            image_info: crate::metadata_simple::ImageInfo {
                dimensions: (1, 1),
                megapixels: 0.0,
                aspect_ratio: "1:1".to_string(),
                format: "JPG".to_string(),
            },
            exif_info: None,
        });

        let insert_pos = apply_undo_state(&mut app, PathBuf::from("/tmp/b.jpg"), 1);

        assert_eq!(insert_pos, 1);
        assert_eq!(app.current_index, 1);
        assert_eq!(app.image_files[1], PathBuf::from("/tmp/b.jpg"));
        assert!(app.current_image.is_none());
        assert!(app.current_metadata.is_none());
    }

    fn temp_cull_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "img-viewer-cull-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn move_current_to_does_not_overwrite_existing_destination() {
        let dir = temp_cull_dir("no-overwrite");
        let src = dir.join("a.jpg");
        let best_dir = dir.join("best");
        std::fs::create_dir_all(&best_dir).unwrap();
        std::fs::write(&src, "new").unwrap();
        std::fs::write(best_dir.join("a.jpg"), "existing").unwrap();

        let mut app = super::super::state::test_app(vec![src.clone()], 0);
        app.current_dir = dir.clone();
        let ctx = eframe::egui::Context::default();

        app.move_current_to(&ctx, "best");

        assert_eq!(app.image_files, vec![src.clone()]);
        assert_eq!(std::fs::read_to_string(&src).unwrap(), "new");
        assert_eq!(
            std::fs::read_to_string(best_dir.join("a.jpg")).unwrap(),
            "existing"
        );
        assert!(app.undo_stack.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn multi_level_undo_restores_moves_and_tags_in_lifo_order() {
        let dir = temp_cull_dir("multi-undo");
        let a = dir.join("a.jpg");
        let b = dir.join("b.jpg");
        let c = dir.join("c.jpg");
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        std::fs::write(&c, "c").unwrap();

        let mut app = super::super::state::test_app(vec![a.clone(), b.clone(), c.clone()], 0);
        app.current_dir = dir.clone();
        app.show_grid = true;
        app.tags.set_name(1, "Retrato").unwrap();
        app.tags.assign(&a, 1);
        let ctx = eframe::egui::Context::default();

        app.move_current_to(&ctx, "best");
        app.move_current_to(&ctx, "deleted");
        assert_eq!(app.image_files, vec![c.clone()]);
        assert_eq!(app.undo_stack.len(), 2);
        assert!(
            app.loading_requests.is_empty(),
            "grid moves must not trigger full-res image loading"
        );

        app.undo_last_move(&ctx);
        assert_eq!(app.image_files, vec![b.clone(), c.clone()]);
        assert!(b.exists());

        app.undo_last_move(&ctx);
        assert_eq!(app.image_files, vec![a.clone(), b.clone(), c.clone()]);
        assert!(a.exists());
        assert_eq!(app.tags.tag_of(&a), Some(1));
        assert!(app.undo_stack.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn moving_last_image_clamps_index_instead_of_wrapping_to_zero() {
        let dir = temp_cull_dir("clamp-last");
        let files: Vec<PathBuf> = ["a.jpg", "b.jpg", "c.jpg"]
            .iter()
            .map(|n| {
                let p = dir.join(n);
                std::fs::write(&p, n).unwrap();
                p
            })
            .collect();

        let mut app = super::super::state::test_app(files, 2);
        app.current_dir = dir.clone();
        app.show_grid = true;
        let ctx = eframe::egui::Context::default();

        app.move_current_to(&ctx, "worse");
        assert_eq!(app.image_files.len(), 2);
        assert_eq!(app.current_index, 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
