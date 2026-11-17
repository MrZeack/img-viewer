# Image Viewer - Rust Edition

A high-performance image viewer written in Rust with GPU acceleration, advanced caching, and professional photography features.

## 🚀 Features

- **Blazing Fast**: Optimized for high-resolution images with zero-copy loading and background preloading.
- **GPU Accelerated**: Smooth 60+ FPS rendering and composition guides.
- **Pro Composition**: 13 professional guides including Rule of Thirds, Golden Ratio, and Dynamic Symmetry.
- **Smart Workflow**: Quick sorting (Like/Worse), bookmarks, and comprehensive EXIF metadata.
- **Cross-platform**: Native performance on Linux, macOS, and Windows.

## ⚡ Installation

Requirements: Rust (https://rustup.rs), `pkg-config` and `libheif` (macOS: `brew install pkg-config libheif`; Debian/Ubuntu: `apt install pkg-config libheif-dev libfontconfig1-dev`).

### Linux/macOS
```bash
git clone https://github.com/MrZeack/img-viewer.git
cd img-viewer
make install   # validates tools, builds optimized for this CPU, installs `img` to ~/.local/bin
```

Variables: `INSTALL_DIR=/path` changes the destination; `NATIVE=0` builds a portable binary.

### Windows
```cmd
build.bat --native --install
```

### Commands

| Command | Description |
|---|---|
| `make install` | Validate tools, build release and install `img` |
| `make build` | Portable release build in `target/release/` |
| `make check` | `cargo fmt --check`, `clippy -D warnings`, tests and operational tests |
| `make bump` | Bump version, update `CHANGELOG.md`, create commit and tag (no push). `TYPE=patch\|minor\|major` or `VERSION=X.Y.Z`; without them the type is inferred from Conventional Commits |
| `cargo run --release` | Run from source |

Release: `make bump`, then `git push origin main && git push origin vX.Y.Z`. No binaries are published; install from source with `make install`.

## 🎯 Usage

### Keyboard Shortcuts
- **← →** : Navigate images
- **Mouse Wheel** : Zoom in/out
- **Mouse Drag** : Pan when zoomed
- **L** : Like (move to `best/` folder)
- **W** : Worse (move to `worse/` folder)
- **U** : Undo last move
- **C** : Toggle composition lines
- **↑ ↓** : Change composition type
- **1-9** : Toggle tag N on the current image (first use asks for the tag name)
- **0** : Remove the tag from the current image
- **T** : Edit tag names (saved in `config.toml`)
- **M** : Move tagged images to `<folder>/<tag name>/` (U undoes the whole batch; existing files are skipped)
- **Esc** : Exit

### Display Order
**S** (or the *Orden* menu) cycles through: date ascending (default, oldest first), date descending, score descending and score ascending. Unscored photos go last, by date; the list is re-sorted once when scoring finishes. The choice is saved as `browsing.sort_order` in `config.toml` (the old `sort_newest_first` is ignored).

The folder is scanned in the background; a spinner shows progress so a large or slow folder does not look frozen.

### Grid View
- **G** : Toggle the thumbnail grid (Enter, Esc or double-click opens the selected image)
- **Arrows / PageUp / PageDown / Home / End** : Move the selection
- **+ / - / pinch** : Thumbnail size
- **L/W/D/U, 1-9, 0, M, S** : Work on the selected image as in single view

Thumbnails are generated in the background (visible first, then the rest of the folder) and stored in an index under the user cache directory (`~/Library/Caches/fyi.voza.img-viewer/thumbs/` on macOS, `~/.cache/img-viewer/thumbs/` on Linux). Reopening a folder reuses it; changed files are regenerated. Deleting that directory is safe.

### Automatic Rating
Every photo gets a 1-10 score in the background (status bar, sidebar breakdown, grid badge).

- **Aesthetic:** NIMA (MobileNet trained on the AVA dataset) on the thumbnail, embedded in the binary (`models/`, Apache-2.0, see `models/NOTICE`).
- **Sharpness:** Laplacian variance of the sharpest blocks of an 800 px version, so a blurred background does not penalize an in-focus subject.
- **Exposure:** share of clipped highlights, crushed shadows and mean brightness.

The score is the aesthetic mean stretched to the full scale, minus penalties for low sharpness or exposure. Calibrated on 309 Sony ARW files; it reflects generic AVA taste, and the exposure check penalizes intentionally low-key photos. Scores are cached with the thumbnails and recomputed when the scoring version changes.

### Advanced Options
```bash
# Run in specific directory
img /path/to/photos

# Run with debug output
RUST_LOG=debug img

# Print version
img --version
```

## 🎨 Composition Guides
1. **Rule of Thirds**
2. **Golden Ratio**
3. **Diagonal**
4. **Triangle**
5. **Golden Spiral**
6. **Center Lines**
7. **Phi Grid**
8. **Dynamic Symmetry**
9. **Armature of Rectangle**
10. **Rabatment**
11. **Harmonious Triangles**
12. **Sinister Diagonal**
13. **Aspect Ratio Guides**

## 🐛 Troubleshooting

**"Binary not in PATH"**
Ensure `~/.local/bin` is in your PATH:
```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

## 🛠️ Architecture
- **`app/`**: Core logic and UI management.
- **`image_loader.rs`**: Async loading with multi-level LRU caching.
- **`metadata_loader.rs`**: Fast EXIF extraction.
- **`composition.rs`**: GPU-rendered composition geometry.
- **`cache.rs`**: Memory-mapped image and metadata cache.

## 🤝 Contributing
1. Fork the repository.
2. Create a feature branch.
3. Ensure `make check` passes.
4. Submit a pull request.
