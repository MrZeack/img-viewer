# Config Schema Migration & Auto-Update Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement explicit configuration versioning (`version`) and automatic migration for older `config.toml` files in `src/config.rs`.

**Architecture:** Add `version: u32` to `Config` struct (default `2`). When `load_config` detects an older config version or missing sections, migrate the config by preserving existing section values, filling new section defaults (`view`), updating `version = 2`, and saving back to `config.toml`.

**Tech Stack:** Rust 2021, toml, serde.

## Global Constraints

- Never execute `git commit` unless explicitly asked.
- 4-space indentation, UTF-8.
- All tests, clippy, and fmt checks must pass.

---

### Task 1: Add Version Field and Migration Handler in `src/config.rs`

**Files:**
- Modify: `src/config.rs`

**Interfaces:**
- Produces: `pub const CURRENT_CONFIG_VERSION: u32 = 2;`, updated `load_config()` function.

- [ ] **Step 1: Write failing unit test for config migration**

Add test `test_migrate_legacy_config_to_version_2` in `src/config.rs`:
```rust
#[test]
fn test_migrate_legacy_config_to_version_2() {
    let legacy_toml = r#"
        [cache]
        image_capacity = 99
        metadata_capacity = 88

        [performance]
        loader_threads = 8
        preload_distance = 4
        max_cached_dimension = 3000

        [browsing]
        sort_newest_first = false
    "#;

    let migrated = migrate_config_str(legacy_toml).expect("Should migrate legacy config");
    assert_eq!(migrated.version, CURRENT_CONFIG_VERSION);
    assert_eq!(migrated.cache.image_capacity, 99);
    assert_eq!(migrated.performance.loader_threads, 8);
    assert_eq!(migrated.browsing.sort_newest_first, false);
    assert!(!migrated.view.show_composition);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test config::tests -- --nocapture`
Expected: FAIL with `migrate_config_str not found`

- [ ] **Step 3: Implement `version` and `migrate_config_str` in `src/config.rs`**

In `src/config.rs`:
- Add `pub const CURRENT_CONFIG_VERSION: u32 = 2;`
- Add `pub version: u32` to `Config` struct (default `CURRENT_CONFIG_VERSION`).
- Implement `pub fn migrate_config_str(content: &str) -> Result<Config, toml::de::Error>`:
  - Parses TOML into a `toml::Table`.
  - Deserializes `cache`, `performance`, `browsing` if present, falling back to defaults.
  - Builds `Config` with `version: CURRENT_CONFIG_VERSION` and `view: ViewConfig::default()`.
- Update `load_config()` to attempt standard parse first; if it fails or version < 2, call `migrate_config_str`, and if successful call `save_config(&migrated)` to update `config.toml` on disk.

- [ ] **Step 4: Run `cargo test` to verify migration passes**

Run: `cargo test config::tests -- --nocapture`
Expected: PASS

- [ ] **Step 5: Verify clippy and fmt**

Run:
```bash
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```
