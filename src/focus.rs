use eframe::egui::{Color32, ColorImage};
use rayon::prelude::*;

/// Applies Laplacian of Gaussian (LoG) edge detection to highlight sharply focused areas with a red overlay.
pub fn apply_focus_peaking(image: &ColorImage, threshold: f32) -> ColorImage {
    let width = image.size[0];
    let height = image.size[1];

    if width < 3 || height < 3 {
        return image.clone();
    }

    // Convert pixels to luminance (0.0 to 255.0) in parallel.
    let lum: Vec<f32> = image
        .pixels
        .par_iter()
        .map(|p| 0.299 * (p.r() as f32) + 0.587 * (p.g() as f32) + 0.114 * (p.b() as f32))
        .collect();

    // Step 1: 3x3 Gaussian blur (parallel across rows) to suppress sensor noise.
    let mut blurred = vec![0.0f32; width * height];
    blurred
        .par_chunks_mut(width)
        .enumerate()
        .for_each(|(y, row)| {
            if y == 0 || y == height - 1 {
                row.copy_from_slice(&lum[y * width..(y + 1) * width]);
                return;
            }
            let prev = &lum[(y - 1) * width..y * width];
            let curr = &lum[y * width..(y + 1) * width];
            let next = &lum[(y + 1) * width..(y + 2) * width];

            row[0] = curr[0];
            row[width - 1] = curr[width - 1];
            for x in 1..(width - 1) {
                let g = prev[x - 1]
                    + prev[x] * 2.0
                    + prev[x + 1]
                    + curr[x - 1] * 2.0
                    + curr[x] * 4.0
                    + curr[x + 1] * 2.0
                    + next[x - 1]
                    + next[x] * 2.0
                    + next[x + 1];
                row[x] = g / 16.0;
            }
        });

    // Step 2: 3x3 Laplacian on blurred luminance (parallel across rows).
    let mut result_pixels = vec![Color32::TRANSPARENT; width * height];
    let src_pixels = &image.pixels;
    result_pixels
        .par_chunks_mut(width)
        .enumerate()
        .for_each(|(y, row)| {
            let src_row = &src_pixels[y * width..(y + 1) * width];
            row.copy_from_slice(src_row);
            if y == 0 || y == height - 1 {
                return;
            }
            let prev = &blurred[(y - 1) * width..y * width];
            let curr = &blurred[y * width..(y + 1) * width];
            let next = &blurred[(y + 1) * width..(y + 2) * width];

            for x in 1..(width - 1) {
                let laplacian = prev[x] + curr[x - 1] + curr[x + 1] + next[x] - 4.0 * curr[x];
                if laplacian.abs() > threshold {
                    row[x] = Color32::from_rgb(255, 0, 0);
                }
            }
        });

    ColorImage {
        size: [width, height],
        pixels: result_pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_focus_peaking_uniform_image() {
        let img = ColorImage::new([10, 10], Color32::from_rgb(100, 100, 100));
        let result = apply_focus_peaking(&img, 30.0);
        assert_eq!(result.pixels, img.pixels);
    }

    #[test]
    fn test_apply_focus_peaking_high_contrast_edge() {
        let mut pixels = vec![Color32::BLACK; 49]; // 7x7 image
        for y in 0..7 {
            for x in 4..7 {
                pixels[y * 7 + x] = Color32::WHITE;
            }
        }
        let img = ColorImage {
            size: [7, 7],
            pixels,
        };
        let result = apply_focus_peaking(&img, 20.0);
        // Sharp edge transition should turn red
        let edge_pixel = result.pixels[3 * 7 + 3];
        assert_eq!(edge_pixel, Color32::from_rgb(255, 0, 0));
    }

    #[test]
    fn test_apply_focus_peaking_large_step_edge_preserves_borders() {
        let (w, h) = (128, 96);
        let mut pixels = vec![Color32::BLACK; w * h];
        for y in 0..h {
            for x in (w / 2)..w {
                pixels[y * w + x] = Color32::WHITE;
            }
        }
        let img = ColorImage {
            size: [w, h],
            pixels,
        };
        let result = apply_focus_peaking(&img, 25.0);
        assert_eq!(
            result.pixels[48 * w + (w / 2 - 1)],
            Color32::from_rgb(255, 0, 0)
        );
        assert_eq!(result.pixels[48 * w + 10], Color32::BLACK);
        assert_eq!(result.pixels[0], Color32::BLACK);
        assert_eq!(result.pixels[(h - 1) * w + (w - 1)], Color32::WHITE);
    }
}
