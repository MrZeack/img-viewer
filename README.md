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
