//! Calificación en la UI: nota de la imagen actual y colores.
use super::state::ImageViewerApp;
use crate::scoring::Score;
use eframe::egui::Color32;

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

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
    fn colors_by_threshold() {
        assert_eq!(score_color(7.0), score_color(9.5));
        assert_ne!(score_color(6.9), score_color(7.0));
        assert_eq!(score_color(4.0), score_color(6.9));
        assert_ne!(score_color(3.9), score_color(4.0));
    }

    #[test]
    fn current_score_follows_selection() {
        let mut app = super::super::state::test_app(["a", "b"].map(p).to_vec(), 1);
        app.grid.scores = HashMap::from([(p("b"), s(6.0))]);
        assert_eq!(app.current_score(), Some(s(6.0)));
        app.current_index = 0;
        assert_eq!(app.current_score(), None);
    }
}
