# Laplacian of Gaussian Focus Peaking Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade Focus Peaking implementation in `src/focus.rs` from standard Sobel to Laplacian of Gaussian (LoG) to filter out background noise and out-of-focus false positives.

**Architecture:** First apply a 3x3 Gaussian low-pass filter to smooth out sensor noise/out-of-focus high frequency texture, then calculate the Laplacian 2nd derivative. Compare the result against a tuned threshold.

**Tech Stack:** Rust 2021, egui 0.28.

## Global Constraints

- Never execute `git commit` unless explicitly asked.
- 4-space indentation, UTF-8.
- All tests, clippy, and fmt checks must pass.

---

### Task 1: Refactor `src/focus.rs` to Use Laplacian of Gaussian (LoG)

**Files:**
- Modify: `src/focus.rs`

**Interfaces:**
- Produces: `pub fn apply_focus_peaking(image: &egui::ColorImage, threshold: f32) -> egui::ColorImage`

- [ ] **Step 1: Update `src/focus.rs` implementation with 3x3 Gaussian Blur + 3x3 Laplacian**

```rust
use eframe::egui::{Color32, ColorImage};

/// Applies Laplacian of Gaussian (LoG) edge detection to highlight sharply focused areas with a red overlay.
pub fn apply_focus_peaking(image: &ColorImage, threshold: f32) -> ColorImage {
    let width = image.size[0];
    let height = image.size[1];

    if width < 3 || height < 3 {
        return image.clone();
    }

    // Convert pixels to luminance (0.0 to 255.0)
    let lum: Vec<f32> = image
        .pixels
        .iter()
        .map(|p| 0.299 * (p.r() as f32) + 0.587 * (p.g() as f32) + 0.114 * (p.b() as f32))
        .collect();

    // Step 1: Apply 3x3 Gaussian Blur to remove noise and out-of-focus fine grain
    let mut blurred = vec![0.0f32; width * height];
    for y in 1..(height - 1) {
        for x in 1..(width - 1) {
            let idx = |x: usize, y: usize| y * width + x;

            let g = lum[idx(x - 1, y - 1)] * 1.0
                + lum[idx(x, y - 1)] * 2.0
                + lum[idx(x + 1, y - 1)] * 1.0
                + lum[idx(x - 1, y)] * 2.0
                + lum[idx(x, y)] * 4.0
                + lum[idx(x + 1, y)] * 2.0
                + lum[idx(x - 1, y + 1)] * 1.0
                + lum[idx(x, y + 1)] * 2.0
                + lum[idx(x + 1, y + 1)] * 1.0;

            blurred[idx(x, y)] = g / 16.0;
        }
    }

    // Step 2: Apply 3x3 Laplacian operator on the blurred luminance
    let mut result_pixels = image.pixels.clone();

    for y in 1..(height - 1) {
        for x in 1..(width - 1) {
            let idx = |x: usize, y: usize| y * width + x;

            let laplacian = blurred[idx(x, y - 1)]
                + blurred[idx(x - 1, y)]
                + blurred[idx(x + 1, y)]
                + blurred[idx(x, y + 1)]
                - 4.0 * blurred[idx(x, y)];

            if laplacian.abs() > threshold {
                result_pixels[idx(x, y)] = Color32::from_rgb(255, 0, 0);
            }
        }
    }

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
}
```

- [ ] **Step 2: Update threshold call in `src/app/ui.rs`**
Update threshold parameter in `src/app/ui.rs` to `30.0` for optimal LoG sensitivity.

- [ ] **Step 3: Verify tests, clippy and formatting**
```bash
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```
