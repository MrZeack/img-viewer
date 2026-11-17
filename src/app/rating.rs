//! Calificación en la UI: orden por nota, colores y acciones de teclado.
use super::state::ImageViewerApp;
use crate::image_loader::get_image_files;
use crate::scoring::Score;
use eframe::egui::Color32;
use std::collections::HashMap;
use std::path::PathBuf;

/// Ordena por nota descendente; las fotos sin nota van al final en su orden actual.
pub(crate) fn sort_by_score(files: &[PathBuf], scores: &HashMap<PathBuf, Score>) -> Vec<PathBuf> {
    let mut sorted = files.to_vec();
    // `sort_by` es estable: empates y fotos sin nota conservan su orden.
    sorted.sort_by(|a, b| {
        let key = |p: &PathBuf| scores.get(p).map(|s| s.overall);
        match (key(a), key(b)) {
            (Some(x), Some(y)) => y.total_cmp(&x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });
    sorted
}

/// Verde ≥ 7, ámbar ≥ 4, rojo por debajo.
pub(crate) fn score_color(overall: f32) -> Color32 {
    if overall >= 7.0 {
        Color32::from_rgb(70, 190, 100)
    } else if overall >= 4.0 {
        Color32::from_rgb(230, 170, 50)
    } else {
        Color32::from_rgb(220, 80, 70)
    }
}

impl ImageViewerApp {
    pub(crate) fn current_score(&self) -> Option<Score> {
        self.current_path()
            .and_then(|p| self.grid.scores.get(p))
            .copied()
    }

    /// S: alterna entre orden por nota y por fecha, conservando la imagen seleccionada.
    pub(crate) fn toggle_score_sort(&mut self) {
        let current = self.current_path().cloned();
        if self.sort_by_score {
            self.image_files = get_image_files(&self.current_dir, self.sort_newest_first)
                .unwrap_or_else(|_| self.image_files.clone());
            self.sort_by_score = false;
            self.set_status_message("Orden: fecha", 2.0);
        } else {
            self.image_files = sort_by_score(&self.image_files, &self.grid.scores);
            self.sort_by_score = true;
            let scores = &self.grid.scores;
            let pending = self
                .image_files
                .iter()
                .filter(|f| !scores.contains_key(*f))
                .count();
            let msg = if pending > 0 {
                format!("Orden: nota ({pending} sin calificar al final)")
            } else {
                "Orden: nota".to_string()
            };
            self.set_status_message(&msg, 3.0);
        }
        if let Some(path) = current {
            self.current_index = self
                .image_files
                .iter()
                .position(|f| *f == path)
                .unwrap_or(0);
        }
        self.grid.scroll_to_selected = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(overall: f32) -> Score {
        Score {
            overall,
            aesthetic: Some(4.0),
            sharpness: 5.0,
            exposure: 5.0,
        }
    }

    fn p(name: &str) -> PathBuf {
        PathBuf::from(format!("/f/{name}"))
    }

    #[test]
    fn sort_puts_best_first_and_unscored_last_in_original_order() {
        let files = ["a", "b", "c", "d", "e"].map(p).to_vec();
        let scores = HashMap::from([(p("b"), s(3.0)), (p("d"), s(8.0)), (p("e"), s(3.0))]);
        assert_eq!(
            sort_by_score(&files, &scores),
            ["d", "b", "e", "a", "c"].map(p).to_vec()
        );
    }

    #[test]
    fn colors_by_threshold() {
        assert_eq!(score_color(7.0), score_color(9.5));
        assert_ne!(score_color(6.9), score_color(7.0));
        assert_eq!(score_color(4.0), score_color(6.9));
        assert_ne!(score_color(3.9), score_color(4.0));
    }

    #[test]
    fn toggle_sort_keeps_selected_image() {
        let mut app = super::super::state::test_app(["a", "b", "c"].map(p).to_vec(), 0);
        app.grid.scores = HashMap::from([(p("c"), s(9.0)), (p("a"), s(2.0))]);
        app.sort_by_score = false;
        app.toggle_score_sort();
        assert!(app.sort_by_score);
        assert_eq!(app.image_files, ["c", "a", "b"].map(p).to_vec());
        assert_eq!(app.current_path(), Some(&p("a")));
        assert_eq!(app.current_score(), Some(s(2.0)));
    }
}
