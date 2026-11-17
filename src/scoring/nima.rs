//! NIMA (Neural Image Assessment) con MobileNet entrenado en AVA: distribución de votos 1-10.
//! Modelo convertido con `scripts/convert_nima.py`; pesos de idealo (Apache-2.0), ver `models/NOTICE`.
use image::RgbImage;
use std::sync::{Arc, OnceLock};
use tract_onnx::prelude::*;

pub const INPUT: u32 = 224;
static MODEL: &[u8] = include_bytes!("../../models/nima_mobilenet_aesthetic.onnx");

type Plan = Arc<TypedRunnableModel>;

pub struct Nima {
    plan: Plan,
}

impl Nima {
    pub fn load() -> Result<Self, String> {
        let plan = tract_onnx::onnx()
            .model_for_read(&mut std::io::Cursor::new(MODEL))
            .and_then(|m| {
                m.with_input_fact(0, f32::fact([1, INPUT as usize, INPUT as usize, 3]).into())
            })
            .and_then(|m| m.into_optimized())
            .and_then(|m| m.into_runnable())
            .map_err(|e| format!("NIMA: {e}"))?;
        Ok(Self { plan })
    }

    /// Instancia compartida; `None` si el modelo no carga (la nota estética queda deshabilitada).
    pub fn shared() -> Option<&'static Nima> {
        static SHARED: OnceLock<Option<Nima>> = OnceLock::new();
        SHARED
            .get_or_init(|| {
                Nima::load()
                    .map_err(|e| eprintln!("Aesthetic scoring disabled: {e}"))
                    .ok()
            })
            .as_ref()
    }

    /// Media de la distribución 1-10. Redimensiona a 224×224 como el entrenamiento original.
    pub fn score(&self, img: &RgbImage) -> Result<f32, String> {
        let resized;
        let img = if img.dimensions() == (INPUT, INPUT) {
            img
        } else {
            resized =
                image::imageops::resize(img, INPUT, INPUT, image::imageops::FilterType::Triangle);
            &resized
        };
        let side = INPUT as usize;
        // preprocess_input de MobileNet: [0, 255] -> [-1, 1], NHWC.
        let input = tract_ndarray::Array4::from_shape_fn((1, side, side, 3), |(_, y, x, c)| {
            img.get_pixel(x as u32, y as u32)[c] as f32 / 127.5 - 1.0
        });
        let outputs = self
            .plan
            .run(tvec![Tensor::from(input).into_tvalue()])
            .map_err(|e| format!("NIMA: {e}"))?;
        let probs = outputs[0]
            .to_plain_array_view::<f32>()
            .map_err(|e| format!("NIMA: {e}"))?;
        Ok(probs
            .iter()
            .enumerate()
            .map(|(i, p)| (i + 1) as f32 * p)
            .sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mismo patrón que `pattern()` en `scripts/convert_nima.py`.
    fn pattern() -> RgbImage {
        RgbImage::from_fn(INPUT, INPUT, |x, y| {
            image::Rgb([
                ((x * 7 + y * 3) % 256) as u8,
                ((x * y) % 256) as u8,
                ((x + 2 * y) % 256) as u8,
            ])
        })
    }

    #[test]
    fn matches_reference_scores_from_keras_conversion() {
        let nima = Nima::load().unwrap();
        let gray = RgbImage::from_pixel(INPUT, INPUT, image::Rgb([128, 128, 128]));
        assert!((nima.score(&pattern()).unwrap() - 6.803318).abs() < 1e-3);
        assert!((nima.score(&gray).unwrap() - 3.884836).abs() < 1e-3);
    }

    #[test]
    fn any_size_is_resized_and_score_is_in_range() {
        let nima = Nima::shared().unwrap();
        let wide = RgbImage::from_fn(384, 256, |x, y| image::Rgb([x as u8, y as u8, 90]));
        let score = nima.score(&wide).unwrap();
        assert!((1.0..=10.0).contains(&score), "{score}");
    }
}
