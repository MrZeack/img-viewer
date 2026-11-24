//! Histograma RGB y luminancia compacto para la barra lateral.
use eframe::egui::{self, Color32, ColorImage, Pos2, Rect, Stroke, Vec2};

pub(crate) const BINS: usize = 64;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Histogram {
    pub(crate) r: [u32; BINS],
    pub(crate) g: [u32; BINS],
    pub(crate) b: [u32; BINS],
    pub(crate) lum: [u32; BINS],
    pub(crate) max_bin: u32,
    pub(crate) clipped_hi_pct: f32,
    pub(crate) clipped_lo_pct: f32,
}

impl Histogram {
    /// Calcula el histograma muestreando hasta ~200k píxeles para mantenerse por debajo de 1 ms.
    pub(crate) fn from_color_image(img: &ColorImage) -> Self {
        let mut r = [0u32; BINS];
        let mut g = [0u32; BINS];
        let mut b = [0u32; BINS];
        let mut lum = [0u32; BINS];
        let total = img.pixels.len();
        if total == 0 {
            return Self {
                r,
                g,
                b,
                lum,
                max_bin: 0,
                clipped_hi_pct: 0.0,
                clipped_lo_pct: 0.0,
            };
        }

        let stride = (total / 200_000).max(1);
        let mut sampled = 0u32;
        let mut hi = 0u32;
        let mut lo = 0u32;
        for px in img.pixels.iter().step_by(stride) {
            let (rv, gv, bv) = (px.r(), px.g(), px.b());
            let lv = ((77u32 * rv as u32 + 150u32 * gv as u32 + 29u32 * bv as u32) >> 8) as u8;
            r[(rv as usize * BINS) >> 8] += 1;
            g[(gv as usize * BINS) >> 8] += 1;
            b[(bv as usize * BINS) >> 8] += 1;
            lum[(lv as usize * BINS) >> 8] += 1;
            if lv >= 250 {
                hi += 1;
            } else if lv <= 5 {
                lo += 1;
            }
            sampled += 1;
        }

        let max_bin = r
            .iter()
            .chain(g.iter())
            .chain(b.iter())
            .chain(lum.iter())
            .copied()
            .max()
            .unwrap_or(0);
        let denom = sampled.max(1) as f32;
        Self {
            r,
            g,
            b,
            lum,
            max_bin,
            clipped_hi_pct: hi as f32 * 100.0 / denom,
            clipped_lo_pct: lo as f32 * 100.0 / denom,
        }
    }

    pub(crate) fn draw(&self, ui: &mut egui::Ui) {
        let width = ui.available_width().max(120.0);
        let height = 72.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, Color32::from_gray(18));

        if self.max_bin == 0 {
            return;
        }
        let max_val = self.max_bin as f32;
        let bar_w = rect.width() / BINS as f32;
        let inner_h = rect.height() - 4.0;
        let base_y = rect.bottom() - 2.0;

        let draw_channel = |bins: &[u32; BINS], color: Color32| {
            for (i, &count) in bins.iter().enumerate() {
                if count == 0 {
                    continue;
                }
                let h = (count as f32 / max_val) * inner_h;
                let x0 = rect.left() + i as f32 * bar_w;
                let bar = Rect::from_min_max(
                    Pos2::new(x0, base_y - h),
                    Pos2::new(x0 + bar_w.max(1.0), base_y),
                );
                painter.rect_filled(bar, 0.0, color);
            }
        };

        draw_channel(&self.r, Color32::from_rgba_unmultiplied(230, 70, 70, 95));
        draw_channel(&self.g, Color32::from_rgba_unmultiplied(70, 210, 90, 95));
        draw_channel(&self.b, Color32::from_rgba_unmultiplied(70, 140, 240, 95));

        let points: Vec<Pos2> = self
            .lum
            .iter()
            .enumerate()
            .map(|(i, &count)| {
                let x = rect.left() + (i as f32 + 0.5) * bar_w;
                let y = base_y - (count as f32 / max_val) * inner_h;
                Pos2::new(x, y)
            })
            .collect();
        painter.add(egui::Shape::line(
            points,
            Stroke::new(1.2_f32, Color32::from_white_alpha(200)),
        ));

        ui.horizontal(|ui| {
            ui.weak(format!("Sombras: {:.1}%", self.clipped_lo_pct));
            ui.weak("·");
            ui.weak(format!("Altas luces: {:.1}%", self.clipped_hi_pct));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_counts_channels_and_clipping() {
        let mut pixels = vec![Color32::from_rgb(255, 255, 255); 20];
        pixels.extend(vec![Color32::from_rgb(0, 0, 0); 10]);
        pixels.extend(vec![Color32::from_rgb(128, 128, 128); 70]);
        let img = ColorImage {
            size: [10, 10],
            pixels,
        };
        let hist = Histogram::from_color_image(&img);
        assert_eq!(hist.lum[0], 10);
        assert_eq!(hist.lum[BINS - 1], 20);
        assert_eq!(hist.max_bin, 70);
        assert!((hist.clipped_hi_pct - 20.0).abs() < 0.01);
        assert!((hist.clipped_lo_pct - 10.0).abs() < 0.01);
    }

    #[test]
    fn histogram_handles_empty_image() {
        let img = ColorImage {
            size: [0, 0],
            pixels: Vec::new(),
        };
        let hist = Histogram::from_color_image(&img);
        assert_eq!(hist.max_bin, 0);
        assert_eq!(hist.clipped_hi_pct, 0.0);
    }
}
