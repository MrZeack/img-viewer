#!/bin/bash

# Image Viewer - Rust Edition Installation Script
# Comprehensive installation script with dependency management

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Default values
INSTALL_RUST=false
INSTALL_SYSTEM=true
INSTALL_DIR="/usr/local/bin"
CREATE_DESKTOP=false
RUST_VERSION="stable"
SKIP_DEPS=false

# Function to print colored output
print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_header() {
    echo -e "${CYAN}==========================================${NC}"
    echo -e "${CYAN} Image Viewer - Complete Installation${NC}"
    echo -e "${CYAN}==========================================${NC}"
}

print_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
}

# Help function
show_help() {
    cat << EOF
Usage: $0 [OPTIONS]

Complete installation script for the Rust Image Viewer

OPTIONS:
    -h, --help           Show this help message
    --install-rust       Install Rust/Cargo if not present
    --rust-version VER   Rust version to install (default: stable)
    -s, --system         Install system-wide to /usr/local/bin [requires sudo]
    --install-dir DIR    Custom installation directory (default: ~/.local/bin)
    --desktop            Create desktop entry and file associations
    --skip-deps          Skip system dependency installation
    --native             Build with native CPU optimizations (maximum performance)
    --uninstall          Remove installed image viewer

EXAMPLES:
    $0                           # Basic installation
    $0 --install-rust            # Install Rust first, then build
    $0 --system                  # System-wide installation
    $0 --desktop                 # Install with desktop integration
    $0 --install-rust --system --desktop  # Full installation
    $0 --native                  # Maximum performance build
    $0 --uninstall               # Remove installation

EOF
}

# Detect OS and distribution
detect_os() {
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if [ -f /etc/os-release ]; then
            . /etc/os-release
            OS_ID=$ID
            OS_VERSION=$VERSION_ID
        else
            OS_ID="unknown-linux"
        fi
        PLATFORM="linux"
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        OS_ID="macos"
        OS_VERSION=$(sw_vers -productVersion)
        PLATFORM="macos"
    elif [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "cygwin" ]]; then
        OS_ID="windows"
        PLATFORM="windows"
    else
        OS_ID="unknown"
        PLATFORM="unknown"
    fi
    
    print_status "Detected OS: $OS_ID ($PLATFORM)"
}

# Install system dependencies
install_dependencies() {
    if [[ "$SKIP_DEPS" == "true" ]]; then
        print_status "Skipping dependency installation"
        return 0
    fi
    
    print_step "Installing system dependencies..."
    
    case "$OS_ID" in
        "ubuntu"|"debian"|"linuxmint")
            print_status "Installing dependencies for Ubuntu/Debian..."
            sudo apt-get update -qq
            sudo apt-get install -y \
                curl \
                build-essential \
                libfontconfig1-dev \
                pkg-config \
                libfreetype6-dev \
                libexpat1-dev \
                libxcb-composite0-dev \
                libssl-dev
            ;;
        "fedora"|"rhel"|"centos")
            print_status "Installing dependencies for Fedora/RHEL..."
            sudo dnf install -y \
                curl \
                gcc \
                gcc-c++ \
                fontconfig-devel \
                pkgconf-pkg-config \
                freetype-devel \
                expat-devel \
                libxcb-devel \
                openssl-devel
            ;;
        "arch"|"manjaro")
            print_status "Installing dependencies for Arch Linux..."
            sudo pacman -S --needed \
                curl \
                base-devel \
                fontconfig \
                pkgconf \
                freetype2 \
                expat \
                libxcb \
                openssl
            ;;
        "opensuse"|"suse")
            print_status "Installing dependencies for openSUSE..."
            sudo zypper install -y \
                curl \
                gcc \
                gcc-c++ \
                fontconfig-devel \
                pkg-config \
                freetype2-devel \
                libexpat-devel \
                libxcb-devel \
                openssl-devel
            ;;
        "macos")
            print_status "Installing Xcode command line tools..."
            if ! xcode-select -p &> /dev/null; then
                xcode-select --install
            fi
            
            # Install Homebrew if not present (optional dependencies)
            if command -v brew &> /dev/null; then
                print_status "Installing additional dependencies via Homebrew..."
                brew install pkg-config
            else
                print_warning "Homebrew not found. Basic build tools should be sufficient."
            fi
            ;;
        *)
            print_warning "Unknown OS. Please install build tools manually:"
            print_warning "  - C/C++ compiler (gcc/clang)"
            print_warning "  - pkg-config"
            print_warning "  - fontconfig development headers"
            ;;
    esac
}

# Install Rust
install_rust() {
    print_step "Installing Rust..."
    
    if command -v cargo &> /dev/null; then
        print_status "Rust already installed: $(rustc --version)"
        return 0
    fi
    
    print_status "Downloading and installing Rust ($RUST_VERSION)..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- \
        --default-toolchain $RUST_VERSION \
        --profile default \
        --no-modify-path \
        -y
    
    # Source the environment
    source $HOME/.cargo/env
    
    print_status "Rust installed successfully: $(rustc --version)"
}

# Build the application
build_application() {
    print_step "Building Image Viewer..."
    
    if [[ ! -f "Cargo.toml" ]]; then
        print_error "Cargo.toml not found! Please run from the project root directory."
        exit 1
    fi
    
    # Set build flags for maximum performance
    export CARGO_INCREMENTAL=0
    export RUSTFLAGS="-C link-arg=-s"  # Strip symbols for smaller binary
    
    if [[ "$NATIVE_OPT" == "true" ]]; then
        export RUSTFLAGS="$RUSTFLAGS -C target-cpu=native"
        print_status "Building with native CPU optimizations..."
    fi
    
    # Build in release mode
    print_status "Compiling optimized binary..."
    cargo build --release
    
    if [[ ! -f "target/release/img_viewer" ]]; then
        print_error "Build failed! Binary not found."
        exit 1
    fi
    
    # Get binary size
    BINARY_SIZE=$(du -h target/release/img_viewer | cut -f1)
    print_status "Build successful! Binary size: $BINARY_SIZE"
}

# Install binary
install_binary() {
    print_step "Installing binary..."
    
    # Create installation directory
    if [[ ! -d "$INSTALL_DIR" ]]; then
        mkdir -p "$INSTALL_DIR"
    fi
    
    INSTALL_PATH="$INSTALL_DIR/img"
    
    # Copy and make executable
    if [[ "$INSTALL_SYSTEM" == "true" ]]; then
        sudo cp target/release/img_viewer "$INSTALL_PATH"
        sudo chmod +x "$INSTALL_PATH"
    else
        cp target/release/img_viewer "$INSTALL_PATH"
        chmod +x "$INSTALL_PATH"
    fi
    
    print_status "Binary installed to: $INSTALL_PATH"
    
    # Update PATH if needed
    update_path
}

# Update PATH environment
update_path() {
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]] && [[ "$INSTALL_SYSTEM" == "false" ]]; then
        print_status "Adding $INSTALL_DIR to PATH..."
        
        # Determine shell config file
        if [[ -n "$ZSH_VERSION" ]]; then
            SHELL_CONFIG="$HOME/.zshrc"
        elif [[ -n "$BASH_VERSION" ]]; then
            SHELL_CONFIG="$HOME/.bashrc"
        else
            SHELL_CONFIG="$HOME/.profile"
        fi
        
        # Add to shell config if not already present
        if ! grep -q "$INSTALL_DIR" "$SHELL_CONFIG" 2>/dev/null; then
            echo "" >> "$SHELL_CONFIG"
            echo "# Added by img installer" >> "$SHELL_CONFIG"
            echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$SHELL_CONFIG"
            print_status "Added to $SHELL_CONFIG"
            print_warning "Please restart your shell or run: source $SHELL_CONFIG"
        fi
    fi
}

# Create desktop entry (Linux only)
create_desktop_entry() {
    if [[ "$PLATFORM" != "linux" ]]; then
        print_warning "Desktop integration only available on Linux"
        return 0
    fi
    
    print_step "Creating desktop integration..."
    
    # Create desktop entry
    DESKTOP_DIR="$HOME/.local/share/applications"
    mkdir -p "$DESKTOP_DIR"
    
    cat > "$DESKTOP_DIR/img.desktop" << EOF
[Desktop Entry]
Name=Image Viewer (Rust)
Comment=High-performance image viewer with composition guides
Exec=$INSTALL_DIR/img %f
Icon=image-viewer
Terminal=false
Type=Application
Categories=Graphics;Photography;Viewer;
MimeType=image/jpeg;image/jpg;image/png;image/bmp;image/tiff;image/webp;
StartupNotify=true
EOF
    
    # Make desktop file executable
    chmod +x "$DESKTOP_DIR/img.desktop"
    
    # Update desktop database
    if command -v update-desktop-database &> /dev/null; then
        update-desktop-database "$DESKTOP_DIR"
    fi
    
    print_status "Desktop entry created"
    
    # Create file associations
    create_file_associations
}

# Create file associations (Linux only)
create_file_associations() {
    print_status "Setting up file associations..."
    
    MIMES_DIR="$HOME/.local/share/mime/packages"
    mkdir -p "$MIMES_DIR"
    
    # Set as default for image types
    MIMETYPES=(
        "image/jpeg"
        "image/jpg"
        "image/png"
        "image/bmp"
        "image/tiff"
        "image/webp"
    )
    
    for mime in "${MIMETYPES[@]}"; do
        if command -v xdg-mime &> /dev/null; then
            xdg-mime default img.desktop "$mime"
        fi
    done
    
    print_status "File associations configured"
}

# Uninstall function
uninstall() {
    print_step "Uninstalling Image Viewer..."
    
    # Remove binary
    INSTALL_PATH="$INSTALL_DIR/img"
    if [[ -f "$INSTALL_PATH" ]]; then
        if [[ "$INSTALL_SYSTEM" == "true" ]]; then
            sudo rm "$INSTALL_PATH"
        else
            rm "$INSTALL_PATH"
        fi
        print_status "Binary removed: $INSTALL_PATH"
    fi
    
    # Remove desktop entry
    DESKTOP_FILE="$HOME/.local/share/applications/img.desktop"
    if [[ -f "$DESKTOP_FILE" ]]; then
        rm "$DESKTOP_FILE"
        print_status "Desktop entry removed"
    fi
    
    print_status "Uninstallation completed"
    exit 0
}

# Parse command line arguments
NATIVE_OPT=false

while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_help
            exit 0
            ;;
        --install-rust)
            INSTALL_RUST=true
            shift
            ;;
        --rust-version)
            RUST_VERSION="$2"
            shift 2
            ;;
        -s|--system)
            INSTALL_SYSTEM=true
            INSTALL_DIR="/usr/local/bin"
            shift
            ;;
        --install-dir)
            INSTALL_DIR="$2"
            shift 2
            ;;
        --desktop)
            CREATE_DESKTOP=true
            shift
            ;;
        --skip-deps)
            SKIP_DEPS=true
            shift
            ;;
        --native)
            NATIVE_OPT=true
            shift
            ;;
        --uninstall)
            uninstall
            ;;
        *)
            print_error "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Main installation process
print_header

# Detect operating system
detect_os

# Install Rust if requested
if [[ "$INSTALL_RUST" == "true" ]]; then
    install_rust
fi

# Check if Rust is available
if ! command -v cargo &> /dev/null; then
    print_error "Rust/Cargo not found!"
    print_error "Install Rust first: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    print_error "Or use: $0 --install-rust"
    exit 1
fi

# Install system dependencies
install_dependencies

# Build application
build_application

# Install binary
install_binary

# Create desktop integration if requested
if [[ "$CREATE_DESKTOP" == "true" ]]; then
    create_desktop_entry
fi

# Final success message
echo
print_status "✅ Installation completed successfully!"
echo
print_status "Usage:"
echo "  img                           # Run in current directory"
echo "  img /path/to/images          # Run in specific directory"
echo
print_status "Features:"
echo "  • High-performance image viewing with GPU acceleration"
echo "  • 13 professional composition guides"
echo "  • EXIF metadata display"
echo "  • Image sorting (L = Like, W = Worse)"
echo "  • Bookmarks system (1-9 keys)"
echo "  • Zoom and pan with mouse"
echo
print_status "Keyboard shortcuts:"
echo "  ← → : Navigate | Wheel: Zoom | L: Like | W: Worse | C: Composition | 1-9: Bookmarks"

if [[ "$CREATE_DESKTOP" == "true" ]]; then
    echo
    print_status "Desktop integration created - you can now:"
    echo "  • Find 'Image Viewer (Rust)' in your applications menu"
    echo "  • Right-click images and select 'Open with Image Viewer'"
    echo "  • Double-click supported images to open automatically"
fi

echo
print_status "Enjoy your high-performance image viewer! 🚀"