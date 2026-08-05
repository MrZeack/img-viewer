# Focus Peaking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Focus Peaking overlay (red highlights on sharp/focused areas) triggered manually by pressing key `F`.

**Architecture:** Create a `focus` module implementing Sobel edge detection on `egui::ColorImage`. Update `ImageViewerApp` state, keyboard handling, and UI rendering to toggle and show the focus peaking overlay.

**Tech Stack:** Rust 2021, egui 0.28, standard Rust tests (`#[test]`).

## Global Constraints

- Never execute `git commit` unless explicitly asked (per memory rule).
- Use 4-space indentation, UTF-8, `snake_case` / `CamelCase`.
- Ensure `cargo test`, `cargo fmt`, and `cargo clippy -- -D warnings` pass.

---

### Task 1: Create `src/focus.rs` Module for Focus Peaking Logic

**Files:**
- Create: `src/focus.rs`
- Modify: `src/main.rs:1-15`
- Test: `src/focus.rs` (inline test module)

**Interfaces:**
- Produces: `pub fn apply_focus_peaking(image: &egui::ColorImage, threshold: f32) -> egui::ColorImage`

- [ ] **Step 1: Write the failing unit tests for `apply_focus_peaking`**

Create `src/focus.rs`:

```rust
use eframe::egui::{Color32, ColorImage};

/// Applies Sobel edge detection to highlight focused (sharp) areas with a red overlay.
pub fn apply_focus_peaking(image: &ColorImage, threshold: f32) -> ColorImage {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_focus_peaking_uniform_image() {
        // A uniform image has 0 gradient, so no pixels should be turned red (255, 0, 0)
        let img = ColorImage::from_pixel([10, 10], Color32::from_rgb(100, 100, 100));
        let result = apply_focus_peaking(&img, 50.0);
        assert_eq!(result.pixels, img.pixels);
    }

    #[test]
    fn test_apply_focus_peaking_high_contrast_edge() {
        // Create an image with a high contrast edge in the middle
        let mut pixels = vec![Color32::BLACK; 25]; // 5x5 image
        // Right side is WHITE
        for y in 0..5 {
            for x in 3..5 {
                pixels[y * 5 + x] = Color32::WHITE;
            }
        }
        let img = ColorImage {
            size: [5, 5],
            pixels,
        };
        let result = apply_focus_peaking(&img, 50.0);
        // Pixels on the edge (e.g. x=2, y=2) should become red Color32::from_rgb(255, 0, 0)
        let edge_pixel = result.pixels[2 * 5 + 2];
        assert_eq!(edge_pixel, Color32::from_rgb(255, 0, 0));
    }
}
```

Add `mod focus;` to `src/main.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test focus::tests -- --nocapture`
Expected: FAIL with `not yet implemented`

- [ ] **Step 3: Implement `apply_focus_peaking`**

Update `src/focus.rs`:

```rust
use eframe::egui::{Color32, ColorImage};

/// Applies Sobel edge detection to highlight focused (sharp) areas with a red overlay.
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

    let mut result_pixels = image.pixels.clone();

    for y in 1..(height - 1) {
        for x in 1..(width - 1) {
            let idx = |x: usize, y: usize| y * width + x;

            let p00 = lum[idx(x - 1, y - 1)];
            let p01 = lum[idx(x, y - 1)];
            let p02 = lum[idx(x + 1, y - 1)];

            let p10 = lum[idx(x - 1, y)];
            let p12 = lum[idx(x + 1, y)];

            let p20 = lum[idx(x - 1, y + 1)];
            let p21 = lum[idx(x, y + 1)];
            let p22 = lum[idx(x + 1, y + 1)];

            let gx = (p02 + 2.0 * p12 + p22) - (p00 + 2.0 * p10 + p20);
            let gy = (p20 + 2.0 * p21 + p22) - (p00 + 2.0 * p01 + p02);

            let magnitude = (gx * gx + gy * gy).sqrt();

            if magnitude > threshold {
                result_pixels[idx(x, y)] = Color32::from_rgb(255, 0, 0);
            }
        }
    }

    ColorImage {
        size: [width, height],
        pixels: result_pixels,
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test focus::tests -- --nocapture`
Expected: PASS

---

### Task 2: Add State, Keyboard Input, and UI Status for Focus Peaking

**Files:**
- Modify: `src/app/state.rs`
- Modify: `src/app/input.rs`
- Modify: `src/app/ui.rs`

**Interfaces:**
- Consumes: `apply_focus_peaking` from `src/focus.rs`
- Modifies: `ImageViewerApp` struct and keyboard handler

- [ ] **Step 1: Add `show_focus_peaking` state and keyboard toggle**

In `src/app/state.rs`:
Add field `pub(crate) show_focus_peaking: bool` to `ImageViewerApp` struct, initialized to `false` in `ImageViewerApp::new` in `src/app/core.rs`.

In `src/app/input.rs`:
Add key handling for `Key::F`:
```rust
} else if ctx.input(|i| i.key_pressed(Key::F)) {
    self.show_focus_peaking = !self.show_focus_peaking;
    if self.show_focus_peaking {
        self.set_status_message("Focus Peaking: ON", 2.0);
    } else {
        self.set_status_message("Focus Peaking: OFF", 2.0);
    }
}
```

- [ ] **Step 2: Update UI rendering to apply Focus Peaking texture overlay**

In `src/app/ui.rs`:
When `self.show_focus_peaking` is true and an image is loaded, apply focus peaking overlay to the loaded image texture when displaying. Also update status bar string to include ` - Focus: ON` when active, and update help text at the bottom right to include `F: Focus Peaking`.

- [ ] **Step 3: Run full tests, clippy and formatting checks**

Run:
```bash
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```
Expected: PASS with 0 warnings or errors.
