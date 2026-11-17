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

/// Califica una foto: estética sobre la miniatura (la red usa 224 px) y técnica sobre la
/// imagen de análisis en escala de grises.
pub fn score(thumb: &image::RgbImage, analysis: &image::GrayImage) -> Score {
    let aesthetic = nima::Nima::shared().and_then(|n| n.score(thumb).ok());
    let sharpness = technical::sharpness(analysis);
    let exposure = technical::exposure(analysis);
    Score {
        overall: combine(aesthetic, sharpness, exposure),
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

/// Penalización máxima por desenfoque y por exposición.
const MAX_BLUR_PENALTY: f32 = 3.0;
const MAX_EXPOSURE_PENALTY: f32 = 2.0;

/// Combina las tres señales en una nota 1-10 con un decimal.
///
/// La media NIMA se estira alrededor de `AESTHETIC_CENTER` para usar la escala completa;
/// luego se restan penalizaciones técnicas: nitidez por debajo de 4 y exposición por debajo de 5.
pub fn combine(aesthetic: Option<f32>, sharpness: f32, exposure: f32) -> f32 {
    let base = aesthetic.map_or(10.0, |mean| {
        5.0 + (mean - AESTHETIC_CENTER) * AESTHETIC_GAIN
    });
    let blur = ((4.0 - sharpness).max(0.0) * 0.75).min(MAX_BLUR_PENALTY);
    let light = ((5.0 - exposure).max(0.0) * 0.4).min(MAX_EXPOSURE_PENALTY);
    let score = (base - blur - light).clamp(1.0, 10.0);
    (score * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_technique_keeps_stretched_aesthetic() {
        assert_eq!(combine(Some(4.0), 9.0, 9.0), 5.0);
        assert_eq!(combine(Some(4.5), 9.0, 9.0), 7.0);
        assert_eq!(combine(Some(3.5), 9.0, 9.0), 3.0);
    }

    #[test]
    fn blur_and_bad_exposure_lower_the_score() {
        let base = combine(Some(4.5), 9.0, 9.0);
        let blurry = combine(Some(4.5), 1.0, 9.0);
        let dark = combine(Some(4.5), 9.0, 1.0);
        assert!(blurry < base - 1.5, "{blurry} vs {base}");
        assert!(dark < base - 1.0, "{dark} vs {base}");
        assert!(combine(Some(4.5), 1.0, 1.0) < blurry.min(dark));
    }

    #[test]
    fn result_is_clamped_and_rounded() {
        assert_eq!(combine(Some(10.0), 10.0, 10.0), 10.0);
        assert_eq!(combine(Some(1.0), 0.0, 0.0), 1.0);
        let v = combine(Some(4.37), 9.0, 9.0);
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
        assert!(combine(None, 2.0, 9.0) < combine(None, 9.0, 9.0));
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
