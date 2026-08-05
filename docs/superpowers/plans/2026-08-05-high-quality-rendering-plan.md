# High Quality (HQ) / Full Resolution Rendering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement on-demand high quality / full resolution image rendering toggled via key `H`.

**Architecture:** Add `quality_multiplier` to state (1 = Normal, 2 = 2x/3x, 3 = Max/Original). Update image loading logic in `src/app/core.rs` to compute target dimensions based on multiplier, add keyboard handler for `H` key, and display status feedback in `src/app/ui.rs`. Reset multiplier to 1 when navigating to adjacent images.

**Tech Stack:** Rust 2021, egui 0.28.

## Global Constraints

- Never execute `git commit` unless explicitly asked.
- 4-space indentation, UTF-8.
- All tests, clippy, and fmt checks must pass.

---

### Task 1: Add Quality Multiplier State and Loading Calculation

**Files:**
- Modify: `src/app/state.rs`
- Modify: `src/app/core.rs`
- Modify: `src/app/input.rs`
- Modify: `src/app/ui.rs`

**Interfaces:**
- Produces: `quality_multiplier: u8` field on `ImageViewerApp` and keyboard handler for `Key::H`.

- [ ] **Step 1: Update `src/app/state.rs`, `src/app/core.rs`, and tests with `quality_multiplier`**

Add `pub(crate) quality_multiplier: u8` to `ImageViewerApp` in `src/app/state.rs`. Initialize `quality_multiplier: 1` in `ImageViewerApp::new` in `src/app/core.rs` and in test helpers in `src/app/input.rs`.

- [ ] **Step 2: Update `load_current_image_with_reset` in `src/app/core.rs`**

Modify image dimensions calculation based on `self.quality_multiplier`:
- If `quality_multiplier == 1`: standard limit (`self.max_cached_dimension`).
- If `quality_multiplier == 2`: 3x standard limit (`(self.max_cached_dimension * 3).min(6144)`).
- If `quality_multiplier == 3`: `None` (full native resolution / no scaling limit).

In `next_image` and `previous_image` in `src/app/input.rs`, reset `self.quality_multiplier = 1`.

- [ ] **Step 3: Add key handling for `Key::F` and `Key::H` in `src/app/input.rs`**

Add handler for `Key::H`:
```rust
} else if ctx.input(|i| i.key_pressed(Key::H)) {
    self.quality_multiplier = if self.quality_multiplier >= 3 { 1 } else { self.quality_multiplier + 1 };
    let msg = match self.quality_multiplier {
        1 => "Calidad: Normal",
        2 => "Calidad: Alta (3x)",
        3 => "Calidad: Original (Máxima)",
        _ => "Calidad: Normal",
    };
    self.set_status_message(msg, 2.0);
    self.load_current_image_with_reset(ctx, false);
}
```

- [ ] **Step 4: Update status bar and legend in `src/app/ui.rs`**

Add quality indicator to status bar if `quality_multiplier > 1`:
```rust
if self.quality_multiplier == 2 {
    status_text.push_str(" - Calidad: 3x");
} else if self.quality_multiplier == 3 {
    status_text.push_str(" - Calidad: MAX");
}
```
And add `H: Calidad` to key legend.

- [ ] **Step 5: Verify all tests, clippy, and formatting**

Run:
```bash
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```
