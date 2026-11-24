//! Calificación automática 1-10: estética (NIMA/AVA) ajustada por calidad técnica.
pub mod nima;
pub mod technical;

/// Cambiarlo invalida las notas guardadas en el índice de miniaturas.
pub const SCORING_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    /// 1-10; resultado mostrado.
    pub overall: f32,
    /// Media NIMA 1-10, o `None` si el modelo no está disponible.
    pub aesthetic: Option<f32>,
    /// 0-10
    pub sharpness: f32,
    /// 0-10
    pub exposure: f32,
}

/// Califica una foto con pesos por defecto: estética sobre la miniatura (224 px) y técnica sobre
/// la imagen de análisis en escala de grises.
#[cfg_attr(not(test), allow(dead_code))]
pub fn score(thumb: &image::RgbImage, analysis: &image::GrayImage) -> Score {
    score_with(thumb, analysis, ScoreWeights::default())
}

/// Califica una foto usando los pesos indicados.
pub fn score_with(
    thumb: &image::RgbImage,
    analysis: &image::GrayImage,
    weights: ScoreWeights,
) -> Score {
    let aesthetic = nima::Nima::shared().and_then(|n| n.score(thumb).ok());
    let sharpness = technical::sharpness(analysis);
    let exposure = technical::exposure(analysis);
    Score {
        overall: combine_with(aesthetic, sharpness, exposure, weights),
        aesthetic,
        sharpness,
        exposure,
    }
}

/// Calibrado con 309 ARW reales (Sony, 2026-11-17): la media NIMA de fotos sin editar
/// va de 3,5 (p5) a 4,6 (p95), por debajo de las fotos de concurso de AVA. Centrar en 4,0
/// con ganancia ×4 lleva ese rango a ~3-7,3.
const AESTHETIC_CENTER: f32 = 4.0;
const AESTHETIC_GAIN: f32 = 4.0;

/// Pesos por defecto del promedio. La estética domina (≥ 50 %): en 309 ARW reales, pesos de 0,5 a 0,8
/// separan igual `best/` del resto (AUC 0,66-0,67), así que se prioriza la preferencia.
pub const AESTHETIC_WEIGHT: f32 = 0.7;
pub const SHARPNESS_WEIGHT: f32 = 0.15;
pub const EXPOSURE_WEIGHT: f32 = 0.15;
/// Peso mínimo de la estética, también para los pesos configurados en `config.toml`.
pub const MIN_AESTHETIC_WEIGHT: f32 = 0.5;
const _: () = assert!(
    AESTHETIC_WEIGHT >= MIN_AESTHETIC_WEIGHT,
    "la estética debe pesar al menos 50 %"
);
const _: () = assert!((AESTHETIC_WEIGHT + SHARPNESS_WEIGHT + EXPOSURE_WEIGHT - 1.0).abs() < 1e-6);

/// Pesos normalizados para combinar estética, nitidez y exposición.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreWeights {
    pub aesthetic: f32,
    pub sharpness: f32,
    pub exposure: f32,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            aesthetic: AESTHETIC_WEIGHT,
            sharpness: SHARPNESS_WEIGHT,
            exposure: EXPOSURE_WEIGHT,
        }
    }
}

impl ScoreWeights {
    /// Normaliza pesos no negativos a suma 1.0 con la estética en al menos `MIN_AESTHETIC_WEIGHT`
    /// (el resto se reparte en proporción); si todos son ≤ 0 o no finitos, usa los por defecto.
    pub fn normalized(aesthetic: f32, sharpness: f32, exposure: f32) -> Self {
        let clean = |v: f32| if v.is_finite() && v > 0.0 { v } else { 0.0 };
        let (a, s, e) = (clean(aesthetic), clean(sharpness), clean(exposure));
        let sum = a + s + e;
        if sum <= 0.0 {
            return Self::default();
        }
        let aesthetic = (a / sum).max(MIN_AESTHETIC_WEIGHT);
        let technical = s + e;
        let (sharpness, exposure) = if technical > 0.0 {
            let left = 1.0 - aesthetic;
            (s / technical * left, e / technical * left)
        } else {
            (0.0, 0.0)
        };
        let aesthetic = if technical > 0.0 { aesthetic } else { 1.0 };
        Self {
            aesthetic,
            sharpness,
            exposure,
        }
    }
}

/// Nota 1-10 con un decimal usando los pesos por defecto.
#[cfg_attr(not(test), allow(dead_code))]
pub fn combine(aesthetic: Option<f32>, sharpness: f32, exposure: f32) -> f32 {
    combine_with(aesthetic, sharpness, exposure, ScoreWeights::default())
}

/// Nota 1-10 con un decimal: promedio ponderado de la estética estirada a 1-10 y de nitidez y
/// exposición (0-10). La media NIMA se estira alrededor de `AESTHETIC_CENTER` para usar la
/// escala completa. Sin modelo, solo cuenta la técnica.
pub fn combine_with(
    aesthetic: Option<f32>,
    sharpness: f32,
    exposure: f32,
    weights: ScoreWeights,
) -> f32 {
    let tech_sum = weights.sharpness + weights.exposure;
    let technical = |weight_left: f32| {
        if tech_sum > 0.0 {
            (weights.sharpness * sharpness + weights.exposure * exposure) / tech_sum * weight_left
        } else {
            (sharpness + exposure) * 0.5 * weight_left
        }
    };
    let score = match aesthetic {
        Some(mean) => {
            let stretched = (5.0 + (mean - AESTHETIC_CENTER) * AESTHETIC_GAIN).clamp(1.0, 10.0);
            weights.aesthetic * stretched + technical(1.0 - weights.aesthetic)
        }
        None => technical(1.0),
    };
    (score.clamp(1.0, 10.0) * 10.0).round() / 10.0
}

/// Recalcula la nota con los pesos por defecto.
#[cfg_attr(not(test), allow(dead_code))]
pub fn rescore(score: Score) -> Score {
    rescore_with(score, ScoreWeights::default())
}

/// Recalcula la nota con los pesos actuales; el índice guarda los componentes, así que
/// cambiar pesos en `config.toml` no obliga a regenerar miniaturas.
pub fn rescore_with(score: Score, weights: ScoreWeights) -> Score {
    Score {
        overall: combine_with(score.aesthetic, score.sharpness, score.exposure, weights),
        ..score
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_average_of_stretched_aesthetic_and_technique() {
        // Estética 4,0 → 5 en escala; 0,7·5 + 0,15·5 + 0,15·5 = 5.
        assert_eq!(combine(Some(4.0), 5.0, 5.0), 5.0);
        // Estética 4,5 → 7: 0,7·7 + 0,15·10 + 0,15·10 = 7,9.
        assert_eq!(combine(Some(4.5), 10.0, 10.0), 7.9);
    }

    #[test]
    fn aesthetic_dominates_technique() {
        let beautiful_but_flawed = combine(Some(5.25), 0.0, 0.0);
        let dull_but_perfect = combine(Some(2.75), 10.0, 10.0);
        assert!(
            beautiful_but_flawed > dull_but_perfect,
            "{beautiful_but_flawed} vs {dull_but_perfect}"
        );
        assert!(
            combine(Some(4.5), 9.0, 9.0) > combine(Some(4.5), 1.0, 1.0),
            "la técnica aún cuenta"
        );
    }

    #[test]
    fn rescore_recomputes_overall_from_components() {
        let stored = Score {
            overall: 99.0,
            aesthetic: Some(4.0),
            sharpness: 5.0,
            exposure: 5.0,
        };
        assert_eq!(rescore(stored).overall, 5.0);
        assert_eq!(rescore(stored).aesthetic, stored.aesthetic);
    }

    #[test]
    fn result_is_clamped_and_rounded() {
        assert_eq!(combine(Some(10.0), 10.0, 10.0), 10.0);
        assert_eq!(combine(Some(1.0), 0.0, 0.0), 1.0);
        let v = combine(Some(4.37), 7.3, 6.1);
        assert_eq!((v * 10.0).round() / 10.0, v);
    }

    #[test]
    fn score_runs_model_and_technical_checks() {
        let thumb = image::RgbImage::from_fn(384, 256, |x, y| image::Rgb([x as u8, y as u8, 80]));
        let sharp =
            image::GrayImage::from_fn(1024, 683, |x, y| image::Luma([((x ^ y) * 37 % 256) as u8]));
        let blurred = image::imageops::blur(&sharp, 5.0);
        let a = score(&thumb, &sharp);
        let b = score(&thumb, &blurred);
        assert!(a.aesthetic.is_some());
        assert_eq!(a.aesthetic, b.aesthetic);
        assert!(a.sharpness > b.sharpness);
        assert!(a.overall > b.overall);
    }

    #[test]
    fn without_model_uses_technical_quality_only() {
        assert_eq!(combine(None, 10.0, 10.0), 10.0);
        assert_eq!(combine(None, 2.0, 8.0), 5.0);
    }

    #[test]
    fn configured_weights_keep_aesthetic_at_least_half() {
        let w = ScoreWeights::normalized(0.2, 0.4, 0.4);
        assert!((w.aesthetic - 0.5).abs() < 1e-6);
        assert!((w.sharpness - 0.25).abs() < 1e-6);
        assert!((w.exposure - 0.25).abs() < 1e-6);
        let only_sharp = ScoreWeights::normalized(0.0, 1.0, 0.0);
        assert!((only_sharp.aesthetic - 0.5).abs() < 1e-6);
        assert!((only_sharp.sharpness - 0.5).abs() < 1e-6);
        let mostly_aesthetic = ScoreWeights::normalized(0.9, 0.1, 0.0);
        assert!(
            (mostly_aesthetic.aesthetic - 0.9).abs() < 1e-6,
            "por encima del mínimo no cambia"
        );
    }

    #[test]
    fn custom_weights_recompute_overall_score() {
        // Aesthetic 4.5 -> stretched 7.0; sharpness 10.0; exposure 2.0.
        // With weights (0.5, 0.5, 0.0) => 0.5*7 + 0.5*10 = 8.5.
        let w = ScoreWeights::normalized(0.5, 0.5, 0.0);
        assert_eq!(combine_with(Some(4.5), 10.0, 2.0, w), 8.5);
        let stored = Score {
            overall: 1.0,
            aesthetic: Some(4.5),
            sharpness: 10.0,
            exposure: 2.0,
        };
        assert_eq!(rescore_with(stored, w).overall, 8.5);
    }

    /// `IMG_BENCH_DIR=<carpeta> cargo test --release bench_scores -- --ignored --nocapture`
    /// Con `best/` en la carpeta, imprime el AUC de cada señal (best/ contra el resto).
    #[test]
    #[ignore]
    fn bench_scores() {
        use std::time::{Duration, Instant};
        struct Row {
            score: Score,
            log_var: f32,
            stats: technical::ExposureStats,
        }
        let measure =
            |files: &[std::path::PathBuf], times: &mut (Duration, Duration)| -> Vec<Row> {
                files
                    .iter()
                    .filter_map(|f| {
                        let t = Instant::now();
                        let g = crate::thumbnails::generate::generate(f, 384).ok()?;
                        times.0 += t.elapsed();
                        let t = Instant::now();
                        let score = score(&g.thumb, &g.analysis);
                        times.1 += t.elapsed();
                        Some(Row {
                            score,
                            log_var: technical::sharpness_log_variance(&g.analysis).unwrap_or(0.0),
                            stats: technical::exposure_stats(&g.analysis)?,
                        })
                    })
                    .collect()
            };
        let dir = std::path::PathBuf::from(std::env::var("IMG_BENCH_DIR").expect("IMG_BENCH_DIR"));
        let files = crate::image_loader::get_image_files(&dir, true).unwrap();
        let sample: Vec<_> = files
            .iter()
            .step_by((files.len() / 300).max(1))
            .cloned()
            .collect();
        let mut times = Default::default();
        let rest = measure(&sample, &mut times);
        let n = (rest.len() as u32).max(1);
        println!(
            "{n} fotos · generar {:.1?}/foto · calificar {:.1?}/foto",
            times.0 / n,
            times.1 / n
        );

        let pct = |mut v: Vec<f32>| {
            v.sort_by(f32::total_cmp);
            [5, 25, 50, 75, 95].map(|p| v[(v.len() - 1) * p / 100])
        };
        type Field = fn(&Row) -> f32;
        let fields: [(&str, Field); 8] = [
            ("nota", |r| r.score.overall),
            ("estética", |r| r.score.aesthetic.unwrap_or(0.0)),
            ("nitidez", |r| r.score.sharpness),
            ("exposición", |r| r.score.exposure),
            ("log var lap", |r| r.log_var),
            ("quemadas", |r| r.stats.hi),
            ("empastadas", |r| -r.stats.lo),
            ("brillo", |r| r.stats.mean),
        ];
        let best = crate::image_loader::get_image_files(&dir.join("best"), true)
            .map(|b| measure(&b, &mut Default::default()))
            .unwrap_or_default();
        println!(
            "{:<12} p5/p25/p50/p75/p95 (resto)              AUC best/",
            ""
        );
        for (name, f) in fields {
            let auc = if best.is_empty() {
                f32::NAN
            } else {
                let wins: f32 = best
                    .iter()
                    .flat_map(|b| rest.iter().map(move |r| (f(b), f(r))))
                    .map(|(b, r)| match b.total_cmp(&r) {
                        std::cmp::Ordering::Greater => 1.0,
                        std::cmp::Ordering::Equal => 0.5,
                        std::cmp::Ordering::Less => 0.0,
                    })
                    .sum();
                wins / (best.len() * rest.len()) as f32
            };
            println!(
                "{name:<12} {:>7.2?}  {auc:.2}",
                pct(rest.iter().map(f).collect())
            );
        }
    }
}
