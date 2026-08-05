# Composition Shortcut & Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update `C` / `Shift+C` keyboard handling for composition guides and persist `view` settings (composition active state, composition guide type, and focus peaking state) in `config.toml`.

**Architecture:** Extend `Config` struct in `src/config.rs` with `ViewConfig` and add a `save_config` function. Update `src/app/core.rs` to load view defaults, and `src/app/input.rs` to update composition shortcuts and save configuration on changes.

**Tech Stack:** Rust 2021, egui 0.28, toml.

## Global Constraints

- Never execute `git commit` unless explicitly asked.
- 4-space indentation, UTF-8.
- All tests, clippy, and fmt checks must pass.

---

### Task 1: Update `src/config.rs` with `ViewConfig` and `save_config`

**Files:**
- Modify: `src/config.rs`
- Test: `src/config.rs`

**Interfaces:**
- Produces: `pub struct ViewConfig`, `pub fn save_config(config: &Config)`

- [ ] **Step 1: Write failing unit test for `ViewConfig` serialization**

Add unit test in `src/config.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_view_config_default_and_serialization() {
        let config = Config::default();
        assert!(!config.view.show_composition);
        assert_eq!(config.view.composition_type, "RuleOfThirds");
        assert!(!config.view.show_focus_peaking);

        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("show_composition"));
    }
}
```

- [ ] **Step 2: Implement `ViewConfig` and `save_config`**

In `src/config.rs`:
Add `ViewConfig` struct and field `pub view: ViewConfig` to `Config`.
Add `pub fn save_config(config: &Config)`:
```rust
pub fn save_config(config: &Config) {
    if let Some(path) = get_config_path() {
        if let Ok(toml_string) = toml::to_string_pretty(config) {
            let _ = fs::write(path, toml_string);
        }
    }
}
```

- [ ] **Step 3: Run `cargo test` to verify `ViewConfig` test passes**

Run: `cargo test config::tests -- --nocapture`
Expected: PASS

---

### Task 2: Update App Initialization and Keyboard Shortcuts in `input.rs`, `core.rs`, and `ui.rs`

**Files:**
- Modify: `src/app/state.rs`
- Modify: `src/app/core.rs`
- Modify: `src/app/input.rs`
- Modify: `src/app/ui.rs`

**Interfaces:**
- Modifies: `ImageViewerApp::new`, `handle_keyboard_input`

- [ ] **Step 1: Update `ImageViewerApp::new` to load config view state**

In `src/app/core.rs`:
Store `config: Config` or `view_config` on `ImageViewerApp` and initialize:
- `show_composition: config.view.show_composition`
- `composition_type: parse_composition_type(&config.view.composition_type)`
- `show_focus_peaking: config.view.show_focus_peaking`

- [ ] **Step 2: Update `C`, `Shift+C`, and `F` shortcuts in `src/app/input.rs`**

Update `handle_keyboard_input`:
- Check `ctx.input(|i| i.key_pressed(Key::C))`:
  - Check `shift = ctx.input(|i| i.modifiers.shift)`:
    - If `shift`:
      - `self.show_composition = false;`
      - `self.set_status_message("Composición desactivada", 2.0);`
    - Else:
      - If `!self.show_composition`:
        - `self.show_composition = true;`
      - Else:
        - `self.composition_type = self.composition_type.next();`
        - `self.composition_cache.clear();`
      - `self.set_status_message(&format!("Composición: {}", self.composition_type.name()), 2.0);`
  - Save updated config `self.save_view_config()`.

- On `Key::F` toggle, save updated config `self.save_view_config()`.

- [ ] **Step 3: Update bottom help legend in `src/app/ui.rs`**

Update key help line:
`"C: Composición | Shift+C: Off | F: Focus | H: Calidad | UP/DOWN: Cambio | R/Shift+R: Rotar | 1-9: Bookmarks | 0: Limpiar"`

- [ ] **Step 4: Verify full test suite, clippy, and format**

Run:
```bash
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```
