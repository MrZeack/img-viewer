# Repository Guidelines

## Project Structure & Module Organization
- Source: `src/` with `main.rs` as the entrypoint and feature modules:
  - `src/app/` (state, input, core logic, UI),
  - `cache.rs`, `image_loader.rs`, `metadata_*`, `composition.rs`, `config.rs`.
- Examples: `example/` (EXIF sample and a small demo).
- Build scripts: `build.sh` (Unix) and `build.bat` (Windows).
- Runtime folders: the app may create `best/`, `worse/`, and `deleted/` in the opened directory; a `config.toml` is created in the standard user configuration directory (e.g., `~/.config/img-viewer/` on Linux) on first run.

## Build, Test, and Development Commands
- Build (debug): `cargo build` — compile quickly for local iteration.
- Run (debug): `cargo run` — launch the viewer from source.
- Run (optimized): `cargo run --release` — better runtime performance.
- Full build/install (Unix): `./build.sh` — builds release and installs `img` to `~/.local/bin` by default. Use `--help` for options.
- Clean: `cargo clean` — remove build artifacts.

## Coding Style & Naming Conventions
- Rust 2021 edition, 4‑space indentation, UTF‑8.
- Names: `snake_case` for functions/vars, `CamelCase` for types, `SCREAMING_SNAKE_CASE` for constants. Module per file where practical.
- Formatting: run `cargo fmt` before committing.
- Linting: run `cargo clippy -- -D warnings` and address findings.

## Testing Guidelines
- Framework: standard Rust tests (`#[test]`) and integration tests under `tests/`.
- Naming: unit tests near code in `mod tests { ... }`; integration files as `tests/<feature>_test.rs`.
- Run: `cargo test` (use `-- --nocapture` to see output). No coverage tool is enforced yet.

## Commit & Pull Request Guidelines
- Commits: concise, imperative. Prefer Conventional Commits (e.g., `feat: add clean cache`). Version bumps use tags/messages like `v0.1.x`.
- PRs: include a clear description, linked issues, reproduction steps, and screenshots/GIFs for UI changes. Ensure `cargo fmt`, `clippy`, and `cargo run --release` succeed.

## Security & Configuration Tips
- File moves: actions like Like/Worse/Delete relocate files into `best/`, `worse/`, or `deleted/`. Verify destination folders and use Undo (`U`) when needed.
- Configuration: edit `config.toml` (created on first run) to tune cache sizes and preload/threads.
