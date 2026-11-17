//! Vista de cuadrícula: virtualizada (solo filas visibles), texturas por ruta y servicio de miniaturas.
use super::grid_layout::{self, Nav};
use super::state::ImageViewerApp;
use crate::thumbnails::service::ThumbnailService;
use crate::thumbnails::{cache_dir_for, worker_threads, THUMB_MAX};
use eframe::egui::{self, Color32, Context, Key, Rect, Sense, Stroke, TextureHandle, Vec2};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

pub(crate) const MIN_CELL: f32 = 96.0;
pub(crate) const MAX_CELL: f32 = 320.0;
const DEFAULT_CELL: f32 = 180.0;
const GAP: f32 = 8.0;
/// Texturas subidas por frame: evita picos al llegar muchas miniaturas juntas.
const UPLOADS_PER_FRAME: usize = 96;

pub(crate) struct GridState {
    service: Option<ThumbnailService>,
    service_dir: PathBuf,
    service_files: Vec<PathBuf>,
    textures: HashMap<PathBuf, TextureHandle>,
    failed: HashSet<PathBuf>,
    pub(crate) cell: f32,
    pub(crate) cols: usize,
    pub(crate) page_rows: usize,
    visible_rows: Range<usize>,
    last_request: Option<(usize, Range<usize>, usize, usize)>,
    pub(crate) scroll_to_selected: bool,
}

impl Default for GridState {
    fn default() -> Self {
        Self {
            service: None,
            service_dir: PathBuf::new(),
            service_files: Vec::new(),
            textures: HashMap::new(),
            failed: HashSet::new(),
            cell: DEFAULT_CELL,
            cols: 1,
            page_rows: 1,
            visible_rows: 0..0,
            last_request: None,
            scroll_to_selected: true,
        }
    }
}

impl GridState {
    pub(crate) fn progress(&self) -> Option<(usize, usize)> {
        self.service.as_ref().map(ThumbnailService::progress)
    }

    pub(crate) fn zoom(&mut self, factor: f32) {
        self.cell = (self.cell * factor).clamp(MIN_CELL, MAX_CELL);
        self.last_request = None;
    }
}

/// Índices de `keep` sin textura ni fallo: lo que hay que pedir al servicio.
pub(crate) fn wanted_indices(
    keep: Range<usize>,
    files: &[PathBuf],
    available: impl Fn(&Path) -> bool,
) -> Vec<usize> {
    keep.filter(|&i| files.get(i).is_some_and(|p| !available(p)))
        .collect()
}

/// Libera las entradas cuyo archivo no está en `files[keep]`.
pub(crate) fn evict_outside<V>(
    map: &mut HashMap<PathBuf, V>,
    files: &[PathBuf],
    keep: Range<usize>,
) {
    let start = keep.start.min(files.len());
    let end = keep.end.min(files.len());
    let keep: HashSet<&PathBuf> = files[start..end].iter().collect();
    map.retain(|path, _| keep.contains(path));
}

impl ImageViewerApp {
    pub(crate) fn open_grid(&mut self) {
        self.show_grid = true;
        self.grid.scroll_to_selected = true;
    }

    pub(crate) fn close_grid(&mut self, ctx: &Context) {
        self.show_grid = false;
        if !self.image_files.is_empty() {
            self.load_current_image(ctx);
        }
    }

    /// Teclas propias de la cuadrícula; devuelve `true` si consumió la tecla.
    pub(crate) fn handle_grid_keys(&mut self, ctx: &Context) -> bool {
        let pressed = |key| ctx.input(|i| i.key_pressed(key));
        if pressed(Key::G) || pressed(Key::Escape) || pressed(Key::Enter) {
            self.close_grid(ctx);
            return true;
        }
        if pressed(Key::Plus) || pressed(Key::Equals) {
            self.grid.zoom(1.15);
            return true;
        }
        if pressed(Key::Minus) {
            self.grid.zoom(1.0 / 1.15);
            return true;
        }
        let nav = [
            (Key::ArrowLeft, Nav::Left),
            (Key::ArrowRight, Nav::Right),
            (Key::ArrowUp, Nav::Up),
            (Key::ArrowDown, Nav::Down),
            (Key::PageUp, Nav::PageUp),
            (Key::PageDown, Nav::PageDown),
            (Key::Home, Nav::Home),
            (Key::End, Nav::End),
        ]
        .into_iter()
        .find(|(key, _)| pressed(*key));
        if let Some((_, nav)) = nav {
            self.current_index = grid_layout::navigate(
                self.current_index,
                nav,
                self.grid.cols,
                self.grid.page_rows,
                self.image_files.len(),
            );
            self.grid.scroll_to_selected = true;
            return true;
        }
        false
    }

    /// Arranca o actualiza el servicio para la carpeta y lista actuales.
    fn sync_grid_service(&mut self, ctx: &Context) {
        let grid = &mut self.grid;
        if grid.service.is_none() || grid.service_dir != self.current_dir {
            grid.service = None; // detiene los workers de la carpeta anterior
            grid.textures.clear();
            grid.failed.clear();
            grid.service_dir = self.current_dir.clone();
            grid.service_files = self.image_files.clone();
            grid.service = Some(ThumbnailService::start(
                self.image_files.clone(),
                cache_dir_for(&self.current_dir),
                THUMB_MAX,
                worker_threads(),
                Some(ctx.clone()),
            ));
            grid.last_request = None;
        } else if grid.service_files != self.image_files {
            grid.service_files = self.image_files.clone();
            if let Some(service) = &grid.service {
                service.set_files(self.image_files.clone());
            }
            grid.last_request = None;
        }
    }

    fn receive_thumbnails(&mut self, ctx: &Context) {
        let Some(service) = &self.grid.service else {
            return;
        };
        for _ in 0..UPLOADS_PER_FRAME {
            let Some(result) = service.try_recv() else {
                return;
            };
            match result.image {
                Some(image) => {
                    let name = format!("thumb:{}", result.path.display());
                    let texture = ctx.load_texture(name, image, egui::TextureOptions::LINEAR);
                    self.grid.textures.insert(result.path, texture);
                }
                None => {
                    self.grid.failed.insert(result.path);
                }
            }
        }
        // Se alcanzó el límite del frame: quedan resultados en el canal.
        ctx.request_repaint();
    }

    pub(crate) fn show_grid_view(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        self.sync_grid_service(ctx);
        self.receive_thumbnails(ctx);

        let zoom = ctx.input(|i| i.zoom_delta());
        if zoom != 1.0 {
            self.grid.zoom(zoom);
        }

        let total = self.image_files.len();
        if total == 0 {
            ui.centered_and_justified(|ui| ui.label("No hay imágenes en esta carpeta"));
            return;
        }

        let cell = self.grid.cell;
        let pitch = cell + GAP;
        let width = ui.available_width() - ui.spacing().scroll.allocated_width();
        let cols = grid_layout::columns(width, cell, GAP);
        let rows = grid_layout::row_count(total, cols);
        self.grid.cols = cols;
        self.grid.page_rows = ((ui.available_height() / pitch).floor() as usize).max(1);

        let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if self.grid.scroll_to_selected {
            self.grid.scroll_to_selected = false;
            let visible = if self.grid.cols == cols {
                self.grid.visible_rows.clone()
            } else {
                0..0
            };
            if let Some(offset) =
                grid_layout::scroll_to_row(self.current_index / cols, visible, pitch)
            {
                scroll = scroll.vertical_scroll_offset(offset);
            }
        }

        let mut clicked = None;
        let mut opened = None;
        let mut visible_rows = 0..0;
        ui.spacing_mut().item_spacing = Vec2::splat(GAP);
        scroll.show_rows(ui, cell, rows, |ui, row_range| {
            visible_rows = row_range.clone();
            for row in row_range {
                ui.horizontal(|ui| {
                    for i in row * cols..((row + 1) * cols).min(total) {
                        let response = self.draw_cell(ui, i, cell);
                        if response.double_clicked() {
                            opened = Some(i);
                        } else if response.clicked() {
                            clicked = Some(i);
                        }
                    }
                });
            }
        });
        // `show_rows` incluye filas parcialmente visibles; para hacer scroll solo cuentan las completas.
        self.grid.visible_rows = visible_rows.start
            ..visible_rows
                .end
                .saturating_sub(1)
                .max(visible_rows.start + 1);

        if let Some(i) = clicked {
            self.current_index = i;
        }
        if let Some(i) = opened {
            self.current_index = i;
            self.close_grid(ctx);
            return;
        }
        self.request_visible_thumbnails(visible_rows, cols, total);
    }

    fn draw_cell(&self, ui: &mut egui::Ui, i: usize, cell: f32) -> egui::Response {
        let path = &self.image_files[i];
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(cell), Sense::click());
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, Color32::from_gray(28));

        if let Some(texture) = self.grid.textures.get(path) {
            let size = texture.size_vec2();
            let scale = (cell / size.x).min(cell / size.y);
            let image_rect = Rect::from_center_size(rect.center(), size * scale);
            let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(texture.id(), image_rect, uv, Color32::WHITE);
        } else {
            let text = if self.grid.failed.contains(path) {
                "sin vista previa"
            } else {
                "…"
            };
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(13.0),
                Color32::from_gray(140),
            );
        }

        if let Some(slot) = self.tags.tag_of(path) {
            let center = rect.left_top() + Vec2::splat(14.0);
            painter.circle_filled(center, 11.0, Color32::from_rgb(230, 160, 40));
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                slot.to_string(),
                egui::FontId::proportional(13.0),
                Color32::BLACK,
            );
        }
        if i == self.current_index {
            painter.rect_stroke(
                rect.shrink(1.5),
                4.0,
                Stroke::new(3.0_f32, Color32::from_rgb(80, 160, 255)),
            );
        }

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        response.on_hover_text(name)
    }

    /// Pide lo visible ± una pantalla y libera texturas a más de tres pantallas.
    fn request_visible_thumbnails(
        &mut self,
        visible_rows: Range<usize>,
        cols: usize,
        total: usize,
    ) {
        let margin = visible_rows.len().max(1);
        let keep = grid_layout::keep_range(visible_rows.clone(), margin, cols, total);
        let focus = visible_rows.start * cols;
        let key = (
            focus,
            keep.clone(),
            self.grid.textures.len(),
            self.grid.failed.len(),
        );
        if self.grid.last_request.as_ref() == Some(&key) {
            return;
        }
        self.grid.last_request = Some(key);

        let retain = grid_layout::keep_range(visible_rows, margin * 3, cols, total);
        evict_outside(&mut self.grid.textures, &self.image_files, retain);

        let grid = &self.grid;
        let wanted = wanted_indices(keep, &self.image_files, |p| {
            grid.textures.contains_key(p) || grid.failed.contains(p)
        });
        if let Some(service) = &grid.service {
            service.set_view(focus, wanted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(n: usize) -> Vec<PathBuf> {
        (0..n)
            .map(|i| PathBuf::from(format!("/f/{i}.jpg")))
            .collect()
    }

    #[test]
    fn wanted_skips_available_and_out_of_range() {
        let files = files(5);
        let wanted = wanted_indices(1..9, &files, |p| p.ends_with("2.jpg"));
        assert_eq!(wanted, vec![1, 3, 4]);
    }

    #[test]
    fn evict_keeps_only_paths_in_range() {
        let files = files(6);
        let mut map: HashMap<PathBuf, u8> = files.iter().cloned().map(|p| (p, 0)).collect();
        map.insert(PathBuf::from("/otro/x.jpg"), 0);
        evict_outside(&mut map, &files, 2..4);
        let mut kept: Vec<_> = map.keys().cloned().collect();
        kept.sort();
        assert_eq!(kept, vec![files[2].clone(), files[3].clone()]);
        evict_outside(&mut map, &files, 10..20);
        assert!(map.is_empty());
    }

    #[test]
    fn zoom_is_clamped() {
        let mut grid = GridState::default();
        grid.zoom(100.0);
        assert_eq!(grid.cell, MAX_CELL);
        grid.zoom(0.0001);
        assert_eq!(grid.cell, MIN_CELL);
    }
}
