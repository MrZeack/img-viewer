use super::state::ImageViewerApp;
use eframe::egui::{Context, Key};
use std::fs;
use std::io;
use std::path::Path;

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
            self.undo_last_move(ctx);
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
                self.last_moved = Some((current_path, dest_path, "best".to_string(), prev_index));
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
                self.last_moved = Some((current_path, dest_path, "worse".to_string(), prev_index));
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
                self.last_moved =
                    Some((current_path, dest_path, "deleted".to_string(), prev_index));
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
        if let Some((original_path, moved_path, dir_type, prev_index)) = &self.last_moved.clone() {
            if moved_path.exists() {
                if move_with_fallback(moved_path, original_path).is_ok() {
                    apply_undo_state(self, original_path.clone(), *prev_index);
                    self.last_moved = None;
                    self.load_current_image(ctx);

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

    fn handle_bookmark(&mut self, num: usize, ctx: &Context) {
        if let Some(bookmark) = self.bookmarks.get(&num) {
            // Restore bookmark
            if bookmark.index < self.image_files.len() {
                self.current_index = bookmark.index;
                self.zoom_factor = bookmark.zoom;
                self.image_position = bookmark.position;
                self.show_composition = bookmark.composition;
                self.composition_type = bookmark.composition_type;
                self.rotation = bookmark.rotation;
                self.rotation_manually_set = true;

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
                rotation: self.rotation,
            };
            self.bookmarks.insert(num, bookmark);

            let filename = self
                .image_files
                .get(self.current_index)
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();

            self.set_status_message(
                &format!(
                    "Saved bookmark {}: {}...",
                    num,
                    truncate_preview(&filename, 20)
                ),
                3.0,
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

fn move_with_fallback(src: &Path, dest: &Path) -> Result<(), io::Error> {
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

fn truncate_preview(text: &str, max_chars: usize) -> String {
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
    use crate::cache::{CompositionCache, ImageCache, MetadataCache};
    use crate::composition::CompositionType;
    use crate::event::{EventBus, ViewerEvent};
    use crate::image_loader::{ImageLoader, ImagePreloader};
    use crate::metadata_loader::MetadataLoader;
    use eframe::egui;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::Instant;

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
        let (tx, rx) = mpsc::channel::<ViewerEvent>();
        let event_bus = EventBus::new(tx, None);
        let image_loader = ImageLoader::new(1, event_bus.clone());
        let preloader = ImagePreloader::new(
            ImageCache::new(2),
            1,
            event_bus.clone(),
            1,
            image_loader.pending_requests(),
        );
        let mut app = super::super::state::ImageViewerApp {
            current_dir: PathBuf::from("/tmp"),
            image_files: vec![PathBuf::from("/tmp/a.jpg"), PathBuf::from("/tmp/c.jpg")],
            current_index: 1,
            current_image: None,
            zoom_factor: 1.0,
            image_position: egui::Vec2::ZERO,
            image_cache: ImageCache::new(2),
            metadata_cache: MetadataCache::new(2),
            composition_cache: CompositionCache::new(2),
            image_loader,
            preloader,
            loading_requests: HashMap::new(),
            current_metadata: Some(crate::metadata_simple::ImageMetadata {
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
            }),
            metadata_loader: MetadataLoader::new(1, event_bus.clone()),
            show_composition: false,
            composition_type: CompositionType::RuleOfThirds,
            show_sidebar: true,
            sidebar_width: 320.0,
            status_message: None,
            bookmarks: HashMap::new(),
            last_moved: None,
            last_cache_cleanup: Instant::now(),
            last_distant_cleanup: Instant::now(),
            event_receiver: rx,
            should_close: false,
            sort_newest_first: true,
            max_cached_dimension: 1024,
            rotation: 0,
            rotation_manually_set: false,
            show_focus_peaking: false,
            quality_multiplier: 1,
            config: crate::config::Config::default(),
        };

        let insert_pos = apply_undo_state(&mut app, PathBuf::from("/tmp/b.jpg"), 1);

        assert_eq!(insert_pos, 1);
        assert_eq!(app.current_index, 1);
        assert_eq!(app.image_files[1], PathBuf::from("/tmp/b.jpg"));
        assert!(app.current_image.is_none());
        assert!(app.current_metadata.is_none());
    }
}
