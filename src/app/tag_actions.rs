//! Acciones de tags sobre el estado de la app: teclas, modales, mover y deshacer.
use super::input::{move_with_fallback, truncate_preview};
use super::state::{ImageViewerApp, LastMove};
use super::tags::{execute_moves, MoveReport, PlannedMove, TagModal, ToggleOutcome};
use crate::image_loader::scan_image_files;
use eframe::egui::Context;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

impl ImageViewerApp {
    /// 1-9: asigna, reemplaza o quita el tag; si no tiene nombre, abre el modal.
    pub(crate) fn tag_key(&mut self, slot: u8) {
        let Some(path) = self.current_path().cloned() else {
            return;
        };
        match self.tags.toggle(&path, slot) {
            ToggleOutcome::Assigned(slot) => self.tag_status("Tag", slot, &path),
            ToggleOutcome::Removed(slot) => self.tag_status("Tag quitado", slot, &path),
            ToggleOutcome::NeedsName(slot) => {
                self.tag_modal = Some(TagModal::Name {
                    slot,
                    target: Some(path),
                    input: String::new(),
                    error: None,
                    focus_pending: true,
                });
            }
        }
    }

    /// 0: quita el tag de la imagen actual.
    pub(crate) fn untag_current(&mut self) {
        let Some(path) = self.current_path().cloned() else {
            return;
        };
        match self.tags.untag(&path) {
            Some(slot) => self.tag_status("Tag quitado", slot, &path),
            None => self.set_status_message("La imagen no tiene tag", 2.0),
        }
    }

    /// T: edita los nombres de los tags.
    pub(crate) fn open_tag_manager(&mut self) {
        let inputs = (1..=9)
            .map(|slot| self.tags.name(slot).unwrap_or_default().to_string())
            .collect();
        self.tag_modal = Some(TagModal::Manage {
            inputs,
            error: None,
        });
    }

    /// M: confirma mover las imágenes etiquetadas a su carpeta.
    pub(crate) fn open_move_confirm(&mut self) {
        if self.tags.counts(&self.image_files).is_empty() {
            self.set_status_message("No hay imágenes con tag", 2.0);
        } else {
            self.tag_modal = Some(TagModal::ConfirmMove);
        }
    }

    /// Guarda el nombre del modal `Name` y asigna el tag a su imagen.
    pub(crate) fn submit_tag_name(&mut self, input: &str) {
        let Some(TagModal::Name { slot, target, .. }) = self.tag_modal.clone() else {
            return;
        };
        match self.tags.set_name(slot, input) {
            Ok(()) => {
                self.persist_tag_names();
                self.tag_modal = None;
                match target {
                    Some(path) => {
                        self.tags.assign(&path, slot);
                        self.tag_status("Tag", slot, &path);
                    }
                    None => self.set_status_message(&format!("Tag {slot} guardado"), 2.0),
                }
            }
            Err(e) => {
                if let Some(TagModal::Name {
                    error,
                    input: field,
                    ..
                }) = &mut self.tag_modal
                {
                    *error = Some(e.to_string());
                    *field = input.to_string();
                }
            }
        }
    }

    /// Aplica los nombres 1-9 de una vez (vacío = sin nombre); si alguno falla, no cambia nada.
    pub(crate) fn submit_tag_manager(&mut self, inputs: &[String]) -> Result<(), String> {
        let mut next = self.tags.clone();
        for slot in 1..=9u8 {
            next.clear_name(slot, &[])?;
        }
        for (slot, input) in (1..=9u8).zip(inputs) {
            if input.trim().is_empty() {
                next.clear_name(slot, &self.image_files)?;
            } else {
                next.set_name(slot, input)
                    .map_err(|e| format!("Tag {slot}: {e}"))?;
            }
        }
        self.tags = next;
        self.persist_tag_names();
        self.tag_modal = None;
        self.set_status_message("Tags guardados", 2.0);
        Ok(())
    }

    /// Mueve las imágenes etiquetadas a `<carpeta>/<nombre del tag>/`.
    pub(crate) fn move_tagged(&mut self, ctx: &Context) {
        self.tag_modal = None;
        let plan = self.tags.plan_moves(&self.current_dir, &self.image_files);
        let report = execute_moves(plan, move_with_fallback);
        let (moved, skipped) = (report.moved.len(), report.skipped.len());
        self.apply_tag_move_report(report);

        let mut msg = format!("Movidas {moved} imagen(es) a sus carpetas de tag");
        if skipped > 0 {
            msg.push_str(&format!(" · {skipped} omitida(s): ya existen en destino"));
        }
        self.set_status_message(&msg, 4.0);

        if self.image_files.is_empty() {
            self.current_image = None;
            self.current_metadata = None;
        } else if self.show_grid {
            self.grid.scroll_to_selected = true;
        } else {
            self.load_current_image(ctx);
        }
    }

    pub(crate) fn apply_tag_move_report(&mut self, report: MoveReport) {
        if report.moved.is_empty() {
            return;
        }
        let current = self.current_path().cloned();
        let moved: HashSet<&PathBuf> = report.moved.iter().map(|m| &m.src).collect();
        let removed_before = self.image_files[..self.current_index.min(self.image_files.len())]
            .iter()
            .filter(|f| moved.contains(f))
            .count();

        for m in &report.moved {
            self.tags.untag(&m.src);
            self.clean_cache_for_moved_image(&m.src);
        }
        self.image_files.retain(|f| !moved.contains(f));

        self.current_index = match current {
            Some(path) if !moved.contains(&path) => self
                .image_files
                .iter()
                .position(|f| *f == path)
                .unwrap_or(0),
            _ => self.current_index - removed_before,
        };
        if self.current_index >= self.image_files.len() {
            self.current_index = self.image_files.len().saturating_sub(1);
        }
        self.current_image = None;
        self.undo_stack.push(LastMove::Batch(report.moved));
    }

    /// U tras un movimiento por tags: devuelve los archivos y restaura sus tags.
    pub(crate) fn undo_tag_move(&mut self, ctx: &Context, moves: Vec<PlannedMove>) {
        let mut restored = Vec::new();
        let mut failed = 0;
        for m in moves {
            if m.dest.exists() && !m.src.exists() && move_with_fallback(&m.dest, &m.src).is_ok() {
                restored.push(m);
            } else {
                failed += 1;
            }
        }
        let rescanned = match scan_image_files(&self.current_dir, None) {
            Ok(entries) => {
                self.mtimes.extend(entries.iter().cloned());
                entries.into_iter().map(|(path, _)| path).collect()
            }
            Err(_) => self.image_files.clone(),
        };
        let count = restored.len();
        self.apply_batch_undo(restored, rescanned);
        self.apply_sort();

        let mut msg = format!("Restauradas {count} imagen(es) con su tag");
        if failed > 0 {
            msg.push_str(&format!(" · {failed} no se pudieron restaurar"));
        }
        self.set_status_message(&msg, 4.0);
        if self.show_grid {
            self.grid.scroll_to_selected = true;
        } else {
            self.load_current_image(ctx);
        }
    }

    pub(crate) fn apply_batch_undo(&mut self, restored: Vec<PlannedMove>, rescanned: Vec<PathBuf>) {
        self.image_files = rescanned;
        for m in &restored {
            self.tags.assign(&m.src, m.slot);
        }
        if let Some(first) = restored.first() {
            if let Some(i) = self.image_files.iter().position(|f| *f == first.src) {
                self.current_index = i;
            }
        }
        if self.current_index >= self.image_files.len() {
            self.current_index = 0;
        }
        self.current_image = None;
        self.current_metadata = None;
    }

    fn persist_tag_names(&mut self) {
        self.config.tags.names = self.tags.to_config();
        #[cfg(not(test))]
        crate::config::save_config(&self.config);
    }

    fn tag_status(&mut self, action: &str, slot: u8, path: &Path) {
        let name = self.tags.name(slot).unwrap_or_default().to_string();
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        self.set_status_message(
            &format!("{action} {slot} ({name}): {}", truncate_preview(&file, 30)),
            2.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::state::test_app;
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn mv(src: &str, slot: u8) -> PlannedMove {
        PlannedMove {
            src: p(src),
            dest: p(&format!("/tmp/t{slot}/x")),
            slot,
        }
    }

    #[test]
    fn unnamed_tag_opens_name_modal_and_submit_assigns_current_image() {
        let mut app = test_app(vec![p("/tmp/a.jpg"), p("/tmp/b.jpg")], 1);
        app.tag_key(3);
        assert_eq!(
            app.tag_modal,
            Some(TagModal::Name {
                slot: 3,
                target: Some(p("/tmp/b.jpg")),
                input: String::new(),
                error: None,
                focus_pending: true,
            })
        );

        app.submit_tag_name("  Familia ");
        assert_eq!(app.tag_modal, None);
        assert_eq!(app.tags.name(3), Some("Familia"));
        assert_eq!(app.tags.tag_of(&p("/tmp/b.jpg")), Some(3));
        assert_eq!(
            app.config.tags.names.get("3").map(String::as_str),
            Some("Familia")
        );
    }

    #[test]
    fn invalid_name_keeps_modal_open_with_error() {
        let mut app = test_app(vec![p("/tmp/a.jpg")], 0);
        app.tag_key(1);
        app.submit_tag_name("a/b");
        assert!(matches!(
            app.tag_modal,
            Some(TagModal::Name { error: Some(_), .. })
        ));
        assert_eq!(app.tags.name(1), None);
    }

    #[test]
    fn named_tag_toggles_and_zero_untags() {
        let mut app = test_app(vec![p("/tmp/a.jpg")], 0);
        app.tags.set_name(1, "a").unwrap();
        app.tag_key(1);
        assert_eq!(app.tags.tag_of(&p("/tmp/a.jpg")), Some(1));
        app.tag_key(1);
        assert_eq!(app.tags.tag_of(&p("/tmp/a.jpg")), None);
        app.tag_key(1);
        app.untag_current();
        assert_eq!(app.tags.tag_of(&p("/tmp/a.jpg")), None);
        assert_eq!(app.tag_modal, None);
    }

    #[test]
    fn tag_keys_do_nothing_without_images() {
        let mut app = test_app(vec![], 0);
        app.tag_key(1);
        assert_eq!(app.tag_modal, None);
    }

    #[test]
    fn manager_swaps_names_and_rejects_clearing_tag_in_use_atomically() {
        let mut app = test_app(vec![p("/tmp/a.jpg")], 0);
        app.tags.set_name(1, "a").unwrap();
        app.tags.set_name(2, "b").unwrap();
        let mut inputs = vec![String::new(); 9];
        inputs[0] = "b".into();
        inputs[1] = "a".into();
        assert_eq!(app.submit_tag_manager(&inputs), Ok(()));
        assert_eq!((app.tags.name(1), app.tags.name(2)), (Some("b"), Some("a")));

        app.tags.assign(&p("/tmp/a.jpg"), 1);
        let mut inputs = vec![String::new(); 9];
        inputs[1] = "z".into();
        assert!(app.submit_tag_manager(&inputs).is_err());
        assert_eq!((app.tags.name(1), app.tags.name(2)), (Some("b"), Some("a")));
    }

    #[test]
    fn move_confirm_opens_only_with_tagged_images() {
        let mut app = test_app(vec![p("/tmp/a.jpg")], 0);
        app.open_move_confirm();
        assert_eq!(app.tag_modal, None);
        app.tags.set_name(1, "a").unwrap();
        app.tags.assign(&p("/tmp/a.jpg"), 1);
        app.open_move_confirm();
        assert_eq!(app.tag_modal, Some(TagModal::ConfirmMove));
    }

    #[test]
    fn move_report_removes_moved_files_and_shows_next_image() {
        let files = ["/tmp/a.jpg", "/tmp/b.jpg", "/tmp/c.jpg", "/tmp/d.jpg"]
            .map(p)
            .to_vec();
        let mut app = test_app(files, 2);
        app.tags.set_name(1, "a").unwrap();
        app.tags.assign(&p("/tmp/a.jpg"), 1);
        app.tags.assign(&p("/tmp/c.jpg"), 1);
        app.apply_tag_move_report(MoveReport {
            moved: vec![mv("/tmp/a.jpg", 1), mv("/tmp/c.jpg", 1)],
            skipped: vec![],
        });
        assert_eq!(app.image_files, vec![p("/tmp/b.jpg"), p("/tmp/d.jpg")]);
        assert_eq!(app.current_index, 1);
        assert_eq!(app.tags.tag_of(&p("/tmp/a.jpg")), None);
        assert!(matches!(app.undo_stack.last(), Some(LastMove::Batch(m)) if m.len() == 2));
    }

    #[test]
    fn move_report_keeps_current_image_when_not_moved() {
        let files = ["/tmp/a.jpg", "/tmp/b.jpg", "/tmp/c.jpg"].map(p).to_vec();
        let mut app = test_app(files, 2);
        app.apply_tag_move_report(MoveReport {
            moved: vec![mv("/tmp/a.jpg", 1)],
            skipped: vec![],
        });
        assert_eq!(app.current_path(), Some(&p("/tmp/c.jpg")));
    }

    #[test]
    fn batch_undo_restores_listing_tags_and_selects_first_restored() {
        let mut app = test_app(vec![p("/tmp/b.jpg")], 0);
        app.tags.set_name(1, "a").unwrap();
        app.apply_batch_undo(
            vec![mv("/tmp/c.jpg", 1)],
            ["/tmp/a.jpg", "/tmp/b.jpg", "/tmp/c.jpg"].map(p).to_vec(),
        );
        assert_eq!(app.image_files.len(), 3);
        assert_eq!(app.current_index, 2);
        assert_eq!(app.tags.tag_of(&p("/tmp/c.jpg")), Some(1));
    }
}
