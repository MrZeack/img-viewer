//! Calidad técnica sin redes: nitidez (varianza del Laplaciano por bloques) y exposición (histograma).
use image::GrayImage;

/// log10 de la varianza del Laplaciano que corresponde a nitidez 0 y 10, a 800 px.
/// En 309 ARW reales el p5-p95 fue 2,0-3,6.
const SHARP_LOG_MIN: f32 = 1.6;
const SHARP_LOG_MAX: f32 = 3.6;
const TILES: u32 = 8;
/// Fracción de bloques más nítidos que define la nitidez de la foto.
const TOP_TILES: f32 = 0.1;

/// Nitidez 0-10. Usa los bloques más nítidos: un fondo desenfocado (bokeh) no penaliza
/// si el sujeto está a foco. Calibrada para imágenes de 800 px de lado mayor (`ANALYSIS_MAX`).
pub fn sharpness(gray: &GrayImage) -> f32 {
    match sharpness_log_variance(gray) {
        Some(log) => {
            ((log - SHARP_LOG_MIN) / (SHARP_LOG_MAX - SHARP_LOG_MIN) * 10.0).clamp(0.0, 10.0)
        }
        None => 0.0,
    }
}

/// log10 de la varianza del Laplaciano en los bloques más nítidos; `None` si no hay detalle.
pub(crate) fn sharpness_log_variance(gray: &GrayImage) -> Option<f32> {
    let (w, h) = gray.dimensions();
    if w < TILES * 3 || h < TILES * 3 {
        return None;
    }
    let (tw, th) = (w / TILES, h / TILES);
    let mut sum = [0f64; (TILES * TILES) as usize];
    let mut sq = [0f64; (TILES * TILES) as usize];
    let mut count = [0u32; (TILES * TILES) as usize];
    let px = |x: u32, y: u32| gray.get_pixel(x, y)[0] as i32;
    for y in 1..h - 1 {
        let ty = (y / th).min(TILES - 1);
        for x in 1..w - 1 {
            let lap = 4 * px(x, y) - px(x - 1, y) - px(x + 1, y) - px(x, y - 1) - px(x, y + 1);
            let t = (ty * TILES + (x / tw).min(TILES - 1)) as usize;
            sum[t] += lap as f64;
            sq[t] += (lap * lap) as f64;
            count[t] += 1;
        }
    }
    let mut variances: Vec<f64> = (0..sum.len())
        .filter(|&t| count[t] > 0)
        .map(|t| {
            let n = count[t] as f64;
            sq[t] / n - (sum[t] / n).powi(2)
        })
        .collect();
    variances.sort_by(|a, b| b.total_cmp(a));
    let top = ((variances.len() as f32 * TOP_TILES).ceil() as usize).max(1);
    let v = variances[..top].iter().sum::<f64>() / top as f64;
    (v > 1.0).then(|| v.log10() as f32)
}

/// Exposición 0-10: penaliza altas luces quemadas, sombras empastadas y brillo medio extremo.
pub fn exposure(gray: &GrayImage) -> f32 {
    let Some(ExposureStats { hi, lo, mean }) = exposure_stats(gray) else {
        return 5.0;
    };
    let penalty = 2.0 * (hi - 0.01).max(0.0)
        + 1.5 * (lo - 0.02).max(0.0)
        + 2.0 * ((mean - 0.47).abs() - 0.2).max(0.0);
    10.0 * (1.0 - penalty.min(1.0))
}

/// Fracción de píxeles quemados (≥250) y empastados (≤5) y brillo medio 0-1.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExposureStats {
    pub hi: f32,
    pub lo: f32,
    pub mean: f32,
}

pub(crate) fn exposure_stats(gray: &GrayImage) -> Option<ExposureStats> {
    let n = gray.pixels().len();
    if n == 0 {
        return None;
    }
    let (mut hi, mut lo, mut total) = (0usize, 0usize, 0u64);
    for p in gray.pixels() {
        let v = p[0];
        hi += (v >= 250) as usize;
        lo += (v <= 5) as usize;
        total += v as u64;
    }
    Some(ExposureStats {
        hi: hi as f32 / n as f32,
        lo: lo as f32 / n as f32,
        mean: total as f32 / n as f32 / 255.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    /// Textura fina pseudoaleatoria, determinista.
    fn texture(w: u32, h: u32) -> GrayImage {
        GrayImage::from_fn(w, h, |x, y| {
            let v = (x.wrapping_mul(2654435761) ^ y.wrapping_mul(40503)).wrapping_mul(2246822519);
            Luma([(64 + (v >> 24) % 128) as u8])
        })
    }

    #[test]
    fn sharp_texture_scores_high_and_blur_scores_low() {
        let sharp = texture(800, 533);
        let blurred = image::imageops::blur(&sharp, 4.0);
        let (s, b) = (sharpness(&sharp), sharpness(&blurred));
        assert!(s >= 8.0, "nítida: {s}");
        assert!(b <= 3.0, "desenfocada: {b}");
    }

    #[test]
    fn subject_in_focus_with_blurred_background_stays_sharp() {
        let mut img = image::imageops::blur(&texture(800, 533), 6.0);
        let subject = texture(300, 300);
        image::imageops::replace(&mut img, &subject, 250, 116);
        assert!(sharpness(&img) >= 7.0, "{}", sharpness(&img));
    }

    #[test]
    fn flat_or_tiny_images_have_zero_sharpness() {
        assert_eq!(
            sharpness(&GrayImage::from_pixel(512, 512, Luma([120]))),
            0.0
        );
        assert_eq!(sharpness(&GrayImage::new(2, 2)), 0.0);
    }

    #[test]
    fn exposure_rewards_midtones_and_penalizes_clipping() {
        let well = GrayImage::from_fn(256, 256, |x, _| Luma([(30 + x * 190 / 255) as u8]));
        assert!(exposure(&well) >= 9.0, "{}", exposure(&well));
        assert!(exposure(&GrayImage::from_pixel(64, 64, Luma([255]))) <= 1.0);
        assert!(exposure(&GrayImage::from_pixel(64, 64, Luma([0]))) <= 1.0);

        let mut half_blown = well.clone();
        for (x, _, p) in half_blown.enumerate_pixels_mut() {
            if x < 77 {
                *p = Luma([255]);
            }
        }
        let e = exposure(&half_blown);
        assert!(e < exposure(&well) && e > 1.0, "{e}");
    }

    #[test]
    fn exposure_of_empty_image_is_neutral() {
        assert_eq!(exposure(&GrayImage::new(0, 0)), 5.0);
    }
}
