# Image Viewer - Rust Edition

A high-performance image viewer written in Rust with GPU acceleration, advanced caching, and professional photography features.

## 🚀 Features

- **Blazing Fast**: Optimized for high-resolution images with zero-copy loading and background preloading.
- **GPU Accelerated**: Smooth 60+ FPS rendering and composition guides.
- **Pro Composition**: 13 professional guides including Rule of Thirds, Golden Ratio, and Dynamic Symmetry.
- **Smart Workflow**: Quick sorting (Like/Worse), bookmarks, and comprehensive EXIF metadata.
- **Cross-platform**: Native performance on Linux, macOS, and Windows.

## ⚡ Installation

Requirements: Rust (https://rustup.rs), `pkg-config` and `libheif` (macOS: `brew install pkg-config libheif`; Debian/Ubuntu: `apt install pkg-config libheif-dev libheif-plugin-libde265 libfontconfig1-dev`; the libde265 plugin decodes Canon 10-bit 4:2:2 HIF, which the ffmpeg plugin of Ubuntu 24.04 cannot).

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
**F1** or **?** opens the full shortcut list inside the app (also *Ayuda* menu and the status bar button).

- **← →** : Navigate images
- **Mouse Wheel / + / - / Space** : Zoom in/out and reset to fit
- **Z / Double-click** : Toggle 100% (1:1) pixel zoom / fit to window
- **Mouse Drag** : Pan when zoomed
- **I / Tab** : Toggle metadata & histogram sidebar
- **R / Shift+R** : Rotate 90° clockwise / counter-clockwise
- **F** : Toggle focus peaking overlay
- **H** : Cycle render quality (Normal, 3x, Original/MAX)
- **L** : Like (move to `best/`, skips if destination exists)
- **W** : Worse (move to `worse/`, skips if destination exists)
- **D** : Delete (move to `deleted/`, skips if destination exists)
- **U** : Undo moves (multi-level stack)
- **C / Shift+C** : Cycle / disable composition guides
- **↑ ↓** : Change composition type
- **1-9** : Toggle tag N on the current image (first use asks for the tag name)
- **0** : Remove the tag from the current image
- **T** : Edit tag names (saved in `config.toml`)
- **M** : Move tagged images to `<folder>/<tag name>/` (U undoes the whole batch; existing files are skipped)
- **Esc** : Exit

Drag and drop a folder or image file onto the window to open it directly.

### Display Order
**S** (or the *Orden* menu) cycles through: date ascending (default, oldest first), date descending, score descending and score ascending. Unscored photos go last, by date; the list is re-sorted once when scoring finishes. The choice is saved as `browsing.sort_order` in `config.toml` (the old `sort_newest_first` is ignored).

The folder is scanned in the background; a spinner shows progress so a large or slow folder does not look frozen.

### Grid View
- **G** : Toggle the thumbnail grid (Enter, Esc or double-click opens the selected image)
- **Arrows / PageUp / PageDown / Home / End** : Move the selection
- **+ / - / pinch** : Thumbnail size
- **L/W/D/U, 1-9, 0, M, S** : Work on the selected image as in single view

Thumbnails are generated in the background (visible first, then the rest of the folder) and stored in an index under the user cache directory (`~/Library/Caches/fyi.voza.img-viewer/thumbs/` on macOS, `~/.cache/img-viewer/thumbs/` on Linux). Reopening a folder reuses it; changed files are regenerated. It can be cleared from *Archivo → Limpiar caché de miniaturas* or by deleting that directory.

### Automatic Rating
Every photo gets a 1-10 score in the background (status bar, sidebar breakdown, grid badge).

- **Aesthetic:** NIMA (MobileNet trained on the AVA dataset) on the thumbnail, embedded in the binary (`models/`, Apache-2.0, see `models/NOTICE`).
- **Sharpness:** Laplacian variance of the sharpest blocks of an 800 px version, so a blurred background does not penalize an in-focus subject.
- **Exposure:** share of clipped highlights, crushed shadows and mean brightness.

The score is a weighted average: 70% aesthetic (NIMA mean stretched to 1-10), 15% sharpness, 15% exposure by default, configurable under `[scoring]` in `config.toml` (weights are normalized and aesthetic never drops below 50%). Components are cached with the thumbnails; the final score is recomputed from them when read, so changing weights does not regenerate anything.

### CLI Options
```bash
# Open a specific folder
img /path/to/photos

# Open a folder positioned on a specific image
img /path/to/photos/shot.ARW

# Print help or version
img --help
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
- **`app/`**: Core state, UI, grid view, histogram, tags, sorting, and background folder scanning.
- **`image_loader.rs`**: Prioritized async image loading with DCT JPEG scaling and RAW/HEIF decoding.
- **`metadata_simple.rs` & `metadata_loader.rs`**: Single-pass EXIF/container metadata extraction.
- **`thumbnails/` & `scoring/`**: Background thumbnail index and NIMA + technical quality rating.
- **`composition.rs` & `focus.rs`**: Composition guides and parallel LoG focus peaking.
- **`cache.rs`**: Generic timestamped LRU cache with configurable memory budget (`cache.image_max_mb`).

## 🤝 Contributing
1. Fork the repository.
2. Create a feature branch.
3. Ensure `make check` passes.
4. Submit a pull request.

## 📄 License
MIT, see [LICENSE](LICENSE). The bundled NIMA model is Apache-2.0 (see [models/NOTICE](models/NOTICE)).
