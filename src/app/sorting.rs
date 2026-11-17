//! Orden de visualización: fecha o calificación, en ambos sentidos.
use super::state::ImageViewerApp;
use crate::scoring::Score;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SortOrder {
    /// Más antigua primero.
    #[default]
    DateAsc,
    DateDesc,
    /// Mejor nota primero.
    ScoreDesc,
    ScoreAsc,
}

impl SortOrder {
    pub(crate) const ALL: [SortOrder; 4] = [
        Self::DateAsc,
        Self::DateDesc,
        Self::ScoreDesc,
        Self::ScoreAsc,
    ];

    pub(crate) fn next(self) -> Self {
        let i = Self::ALL.iter().position(|o| *o == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::DateAsc => "Fecha ↑ (antiguas primero)",
            Self::DateDesc => "Fecha ↓ (recientes primero)",
            Self::ScoreDesc => "Nota ↓ (mejores primero)",
            Self::ScoreAsc => "Nota ↑ (peores primero)",
        }
    }

    pub(crate) fn config_key(self) -> &'static str {
        match self {
            Self::DateAsc => "date_asc",
            Self::DateDesc => "date_desc",
            Self::ScoreDesc => "score_desc",
            Self::ScoreAsc => "score_asc",
        }
    }

    /// Valor desconocido o ausente → `DateAsc`.
    pub(crate) fn from_config(key: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|o| o.config_key() == key)
            .unwrap_or_default()
    }

    pub(crate) fn by_score(self) -> bool {
        matches!(self, Self::ScoreDesc | Self::ScoreAsc)
    }
}

/// Ordena en sitio. Por nota, las fotos sin calificar van al final por fecha ascendente.
/// Los empates se resuelven por fecha y luego por ruta, para que el orden sea determinista.
pub(crate) fn sort_files(
    files: &mut [PathBuf],
    order: SortOrder,
    mtimes: &HashMap<PathBuf, SystemTime>,
    scores: &HashMap<PathBuf, Score>,
) {
    let date = |p: &PathBuf| mtimes.get(p).copied().unwrap_or(SystemTime::UNIX_EPOCH);
    let by_date = |a: &PathBuf, b: &PathBuf| date(a).cmp(&date(b)).then_with(|| a.cmp(b));
    match order {
        SortOrder::DateAsc => files.sort_by(by_date),
        SortOrder::DateDesc => files.sort_by(|a, b| by_date(b, a)),
        SortOrder::ScoreDesc | SortOrder::ScoreAsc => {
            files.sort_by(|a, b| match (scores.get(a), scores.get(b)) {
                (Some(x), Some(y)) => {
                    let by_score = x.overall.total_cmp(&y.overall);
                    let by_score = if order == SortOrder::ScoreDesc {
                        by_score.reverse()
                    } else {
                        by_score
                    };
                    by_score.then_with(|| by_date(a, b))
                }
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => by_date(a, b),
            })
        }
    }
}

impl ImageViewerApp {
    /// Reordena la lista actual conservando la imagen seleccionada.
    pub(crate) fn apply_sort(&mut self) {
        let current = self.current_path().cloned();
        sort_files(
            &mut self.image_files,
            self.sort_order,
            &self.mtimes,
            &self.grid.scores,
        );
        let scores = &self.grid.scores;
        self.grid.resort_pending =
            self.sort_order.by_score() && self.image_files.iter().any(|f| !scores.contains_key(f));
        if let Some(path) = current {
            self.current_index = self
                .image_files
                .iter()
                .position(|f| *f == path)
                .unwrap_or(0);
        }
        self.grid.scroll_to_selected = true;
    }

    /// Cambia el orden, lo aplica y lo guarda en la configuración.
    pub(crate) fn set_sort_order(&mut self, order: SortOrder) {
        self.sort_order = order;
        self.apply_sort();
        self.config.browsing.sort_order = order.config_key().to_string();
        #[cfg(not(test))]
        crate::config::save_config(&self.config);

        let scores = &self.grid.scores;
        let pending = self
            .image_files
            .iter()
            .filter(|f| !scores.contains_key(*f))
            .count();
        let msg = if order.by_score() && pending > 0 {
            format!(
                "Orden: {} · {pending} sin calificar al final",
                order.label()
            )
        } else {
            format!("Orden: {}", order.label())
        };
        self.set_status_message(&msg, 3.0);
    }

    /// Orden por nota aplicado con fotos sin calificar: reordena una vez cuando ya tienen nota
    /// o el servicio terminó.
    pub(crate) fn resort_when_scoring_finishes(&mut self) {
        if !self.grid.resort_pending || !self.sort_order.by_score() {
            return;
        }
        let service_done = self
            .grid
            .progress()
            .is_some_and(|(done, total)| done >= total);
        let scores = &self.grid.scores;
        let all_scored = self.image_files.iter().all(|f| scores.contains_key(f));
        if service_done || all_scored {
            self.apply_sort();
            // Las fotos que no se pudieron calificar no deben provocar un reordenado por frame.
            self.grid.resort_pending = false;
            self.set_status_message("Orden por nota actualizado", 2.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn p(name: &str) -> PathBuf {
        PathBuf::from(format!("/f/{name}"))
    }

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn score(overall: f32) -> Score {
        Score {
            overall,
            aesthetic: None,
            sharpness: 5.0,
            exposure: 5.0,
        }
    }

    fn fixture() -> (
        Vec<PathBuf>,
        HashMap<PathBuf, SystemTime>,
        HashMap<PathBuf, Score>,
    ) {
        let files = ["a", "b", "c", "d", "e"].map(p).to_vec();
        let mtimes = HashMap::from([
            (p("a"), at(30)),
            (p("b"), at(10)),
            (p("c"), at(50)),
            (p("d"), at(20)),
            (p("e"), at(40)),
        ]);
        let scores = HashMap::from([
            (p("a"), score(5.0)),
            (p("c"), score(8.0)),
            (p("e"), score(5.0)),
        ]);
        (files, mtimes, scores)
    }

    fn sorted(order: SortOrder) -> Vec<PathBuf> {
        let (mut files, mtimes, scores) = fixture();
        sort_files(&mut files, order, &mtimes, &scores);
        files
    }

    #[test]
    fn date_orders() {
        assert_eq!(
            sorted(SortOrder::DateAsc),
            ["b", "d", "a", "e", "c"].map(p).to_vec()
        );
        assert_eq!(
            sorted(SortOrder::DateDesc),
            ["c", "e", "a", "d", "b"].map(p).to_vec()
        );
    }

    #[test]
    fn score_orders_put_unscored_last_by_date() {
        // Empate a/e (5.0): desempata la fecha ascendente.
        assert_eq!(
            sorted(SortOrder::ScoreDesc),
            ["c", "a", "e", "b", "d"].map(p).to_vec()
        );
        assert_eq!(
            sorted(SortOrder::ScoreAsc),
            ["a", "e", "c", "b", "d"].map(p).to_vec()
        );
    }

    #[test]
    fn missing_mtime_sorts_as_oldest_then_by_path() {
        let mut files = vec![p("z"), p("y"), p("b")];
        let mtimes = HashMap::from([(p("b"), at(5))]);
        sort_files(&mut files, SortOrder::DateAsc, &mtimes, &HashMap::new());
        assert_eq!(files, ["y", "z", "b"].map(p).to_vec());
    }

    #[test]
    fn next_cycles_through_all_orders() {
        let mut order = SortOrder::default();
        assert_eq!(order, SortOrder::DateAsc);
        for expected in [
            SortOrder::DateDesc,
            SortOrder::ScoreDesc,
            SortOrder::ScoreAsc,
            SortOrder::DateAsc,
        ] {
            order = order.next();
            assert_eq!(order, expected);
        }
    }

    #[test]
    fn config_keys_roundtrip_and_default_on_unknown() {
        for order in SortOrder::ALL {
            assert_eq!(SortOrder::from_config(order.config_key()), order);
            assert!(!order.label().is_empty());
        }
        assert_eq!(SortOrder::from_config("nope"), SortOrder::DateAsc);
    }

    #[test]
    fn score_sort_resorts_once_when_all_scores_arrive() {
        let (files, mtimes, _) = fixture();
        let mut app = super::super::state::test_app(files, 0);
        app.mtimes = mtimes;
        app.set_sort_order(SortOrder::ScoreDesc);
        assert!(app.grid.resort_pending, "nadie calificado aún");
        assert_eq!(app.image_files[0], p("b"), "sin notas: por fecha");

        app.resort_when_scoring_finishes();
        assert_eq!(app.image_files[0], p("b"), "faltan notas");

        for (i, f) in app.image_files.clone().iter().enumerate() {
            app.grid.scores.insert(f.clone(), score(i as f32));
        }
        app.resort_when_scoring_finishes();
        assert!(!app.grid.resort_pending);
        assert_eq!(app.image_files[0], p("c"), "la de mejor nota primero");
    }

    #[test]
    fn set_sort_order_keeps_selection_and_persists_choice() {
        let (files, mtimes, scores) = fixture();
        let mut app = super::super::state::test_app(files, 0);
        app.mtimes = mtimes;
        app.grid.scores = scores;
        app.set_sort_order(SortOrder::ScoreDesc);
        assert_eq!(app.image_files[0], p("c"));
        assert_eq!(app.current_path(), Some(&p("a")));
        assert_eq!(app.config.browsing.sort_order, "score_desc");
    }
}
