# Image Viewer - Rust Edition

A high-performance image viewer written in Rust with GPU acceleration, advanced caching, and professional photography features.

## 🚀 Features

- **Blazing Fast**: Optimized for high-resolution images with zero-copy loading and background preloading.
- **GPU Accelerated**: Smooth 60+ FPS rendering and composition guides.
- **Pro Composition**: 13 professional guides including Rule of Thirds, Golden Ratio, and Dynamic Symmetry.
- **Smart Workflow**: Quick sorting (Like/Worse), bookmarks, and comprehensive EXIF metadata.
- **Cross-platform**: Native performance on Linux, macOS, and Windows.

## ⚡ Installation

### Linux/macOS
```bash
# Clone the repository
git clone https://github.com/youruser/img-viewer.git
cd img-viewer

# Make build script executable
chmod +x build.sh

# Complete setup (installs dependencies, builds, and installs to ~/.local/bin)
./build.sh --install-deps --install
```

### Windows
```cmd
# Install Rust from https://rustup.rs/ first, then:
build.bat --native --install
```

### Manual Build
If you prefer using cargo directly:
```bash
# Build with maximum performance
RUSTFLAGS="-C target-cpu=native" cargo build --release

# Run directly
./target/release/img_viewer
```

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
- **1-9** : Save/restore bookmarks
- **0** : Clear all bookmarks
- **Esc** : Exit

### Advanced Options
```bash
# Run in specific directory
img /path/to/photos

# Run with debug output
RUST_LOG=debug img
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

## 🔧 Build Options

| Flag | Description |
|------|-------------|
| `--native` | Optimize for current CPU (Default) |
| `--release` | Build optimized binary (Default) |
| `--install` | Copy binary to local bin directory |
| `--system` | Install to /usr/local/bin (requires sudo) |
| `--install-deps` | Install required system libraries |

## 🐛 Troubleshooting

**"Permission denied"**
```bash
chmod +x build.sh
```

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
3. Ensure `cargo fmt` and `cargo clippy` pass.
4. Submit a pull request.
