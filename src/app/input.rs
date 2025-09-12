use super::state::ImageViewerApp;
use eframe::egui::{Context, Key};

impl ImageViewerApp {
    pub(crate) fn handle_keyboard_input(&mut self, ctx: &Context) {
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
            if let Some(key) = Key::from_name(&num.to_string()) {
                if ctx.input(|i| i.key_pressed(key)) {
                    self.handle_bookmark(num, ctx);
                }
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
        if let Some(file_name) = current_path.file_name() {
            let dest_dir = self.current_dir.join("best");
            let dest_path = dest_dir.join(file_name);

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
        } else {
            self.set_status_message("Invalid file name", 3.0);
        }
    }

    fn undo_last_move(&mut self) {
        if let Some((original_path, moved_path, dir_type)) = &self.last_moved.clone() {
            if moved_path.exists() {
                if std::fs::rename(moved_path, original_path).is_ok() {
                    // Re-insert into file list at correct position
                    let original_modified = original_path.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    let mut insert_pos = self.image_files.len();
                    for (i, file) in self.image_files.iter().enumerate() {
                        let file_modified = file.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        if file_modified > original_modified {
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
            let bookmark = super::state::BookmarkState {
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
}
