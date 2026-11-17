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
                self.set_status_message("Rotated counter-clockwise", 1.5);
            } else {
                self.rotation = (self.rotation + 1).rem_euclid(4);
                self.set_status_message("Rotated clockwise", 1.5);
            }
        } else if ctx.input(|i| i.key_pressed(Key::F)) {
            self.show_focus_peaking = !self.show_focus_peaking;
            if self.show_focus_peaking {
                self.set_status_message("Focus Peaking: ON", 2.0);
            } else {
                self.set_status_message("Focus Peaking: OFF", 2.0);
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
        }

        self.handle_tag_keys(ctx);
    }

    /// L/W/D/U; devuelve `true` si consumió la tecla.
    fn handle_move_keys(&mut self, ctx: &Context) -> bool {
        if ctx.input(|i| i.key_pressed(Key::L)) {
            self.like_image(ctx);
        } else if ctx.input(|i| i.key_pressed(Key::W)) {
            self.worse_image(ctx);
        } else if ctx.input(|i| i.key_pressed(Key::D)) {
            self.delete_image(ctx);
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

    fn like_image(&mut self, ctx: &Context) {
        if self.image_files.is_empty() {
            return;
        }

        let current_path = self.image_files[self.current_index].clone();
        if let Some(file_name) = current_path.file_name() {
            let dest_dir = self.current_dir.join("best");
            let dest_path = dest_dir.join(file_name);

            if move_with_fallback(&current_path, &dest_path).is_ok() {
                // Clean cache for moved image
                self.clean_cache_for_moved_image(&current_path);

                let prev_index = self.current_index;
                self.last_moved = Some(LastMove::Single {
                    source: current_path,
                    dest: dest_path,
                    label: "best".to_string(),
                    index: prev_index,
                });
                self.image_files.remove(self.current_index);

                if self.current_index >= self.image_files.len() && !self.image_files.is_empty() {
                    self.current_index = 0;
                }

                self.set_status_message("Moved to best/", 3.0);

                // Load next image automatically
                if !self.image_files.is_empty() {
                    self.load_current_image(ctx);
                } else {
                    self.current_image = None;
                    self.current_metadata = None;
                }
            } else {
                self.set_status_message("Error moving image", 3.0);
            }
        } else {
            self.set_status_message("Invalid file name", 3.0);
        }
    }

    fn worse_image(&mut self, ctx: &Context) {
        if self.image_files.is_empty() {
            return;
        }

        let current_path = self.image_files[self.current_index].clone();
        if let Some(file_name) = current_path.file_name() {
            let dest_dir = self.current_dir.join("worse");
            let dest_path = dest_dir.join(file_name);

            if move_with_fallback(&current_path, &dest_path).is_ok() {
                // Clean cache for moved image
                self.clean_cache_for_moved_image(&current_path);

                let prev_index = self.current_index;
                self.last_moved = Some(LastMove::Single {
                    source: current_path,
                    dest: dest_path,
                    label: "worse".to_string(),
                    index: prev_index,
                });
                self.image_files.remove(self.current_index);

                if self.current_index >= self.image_files.len() && !self.image_files.is_empty() {
                    self.current_index = 0;
                }

                self.set_status_message("Moved to worse/", 3.0);

                // Load next image automatically
                if !self.image_files.is_empty() {
                    self.load_current_image(ctx);
                } else {
                    self.current_image = None;
                    self.current_metadata = None;
                }
            } else {
                self.set_status_message("Error moving image", 3.0);
            }
        } else {
            self.set_status_message("Invalid file name", 3.0);
        }
    }

    fn delete_image(&mut self, ctx: &Context) {
        if self.image_files.is_empty() {
            return;
        }

        let current_path = self.image_files[self.current_index].clone();
        if let Some(file_name) = current_path.file_name() {
            let dest_dir = self.current_dir.join("deleted");
            let dest_path = dest_dir.join(file_name);

            if move_with_fallback(&current_path, &dest_path).is_ok() {
                // Clean cache for moved image
                self.clean_cache_for_moved_image(&current_path);

                let prev_index = self.current_index;
                self.last_moved = Some(LastMove::Single {
                    source: current_path,
                    dest: dest_path,
                    label: "deleted".to_string(),
                    index: prev_index,
                });
                self.image_files.remove(self.current_index);

                if self.current_index >= self.image_files.len() && !self.image_files.is_empty() {
                    self.current_index = 0;
                }

                self.set_status_message("Moved to deleted/", 3.0);

                // Load next image automatically
                if !self.image_files.is_empty() {
                    self.load_current_image(ctx);
                } else {
                    self.current_image = None;
                    self.current_metadata = None;
                }
            } else {
                self.set_status_message("Error deleting image", 3.0);
            }
        } else {
            self.set_status_message("Invalid file name", 3.0);
        }
    }

    fn undo_last_move(&mut self, ctx: &Context) {
        match self.last_moved.clone() {
            Some(LastMove::Single {
                source,
                dest,
                label,
                index,
            }) => {
                if !dest.exists() {
                    self.set_status_message("Image no longer exists", 3.0);
                    self.last_moved = None;
                } else if move_with_fallback(&dest, &source).is_ok() {
                    apply_undo_state(self, source, index);
                    self.last_moved = None;
                    self.load_current_image(ctx);
                    self.set_status_message(&format!("Restored from {}/", label), 3.0);
                } else {
                    self.set_status_message("Error restoring image", 3.0);
                }
            }
            Some(LastMove::Batch(moves)) => self.undo_tag_move(ctx, moves),
            None => self.set_status_message("No image to undo", 3.0),
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
}
