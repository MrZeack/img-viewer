#!/bin/bash

# Image Viewer - Rust Edition Build Script
# Builds and optionally installs the high-performance image viewer

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
BUILD_TYPE="release"
INSTALL=true
INSTALL_DIR="$HOME/.local/bin"
SYSTEM_INSTALL=false
TARGET=""
VERBOSE=false
NATIVE=true
INSTALL_DEPS=false

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
    echo -e "${BLUE}=================================${NC}"
    echo -e "${BLUE} Image Viewer - Rust Build Script${NC}"
    echo -e "${BLUE}=================================${NC}"
}

# Help function
show_help() {
    cat << EOF
Usage: $0 [OPTIONS]

Build and install the Rust Image Viewer

OPTIONS:
    -h, --help          Show this help message
    -d, --debug         Build in debug mode (faster compile, slower runtime)
    -r, --release       Build in release mode (default, optimized)
    --no-install        Don't install binary after building (default: install)
    -s, --system        Install to system directory (/usr/local/bin) [requires sudo]
    --install-dir DIR   Custom installation directory (default: ~/.local/bin)
    -t, --target TARGET Cross-compile for specific target
    -v, --verbose       Verbose output
    --no-native         Don't optimize for current CPU (default: optimize)
    --install-deps      Install system dependencies
    --clean             Clean build artifacts before building

EXAMPLES:
    $0                          # Build, optimize, and install to ~/.local/bin
    $0 --debug                  # Build in debug mode
    $0 --no-install             # Build only, don't install
    $0 --system                 # Build and install system-wide
    $0 --no-native              # Build without CPU-specific optimizations
    $0 --install-deps           # Build with system dependencies installation
    $0 --target x86_64-pc-windows-gnu  # Cross-compile for Windows

TARGETS:
    x86_64-unknown-linux-gnu    # Linux x64 (default)
    x86_64-pc-windows-gnu       # Windows x64
    x86_64-apple-darwin         # macOS x64
    aarch64-apple-darwin        # macOS ARM64 (M1/M2)
    aarch64-unknown-linux-gnu   # Linux ARM64

EOF
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_help
            exit 0
            ;;
        -d|--debug)
            BUILD_TYPE="debug"
            shift
            ;;
        -r|--release)
            BUILD_TYPE="release"
            shift
            ;;
        --no-install)
            INSTALL=false
            shift
            ;;
        -s|--system)
            SYSTEM_INSTALL=true
            INSTALL_DIR="/usr/local/bin"
            shift
            ;;
        --install-dir)
            INSTALL_DIR="$2"
            shift 2
            ;;
        -t|--target)
            TARGET="$2"
            shift 2
            ;;
        -v|--verbose)
            VERBOSE=true
            shift
            ;;
        --no-native)
            NATIVE=false
            shift
            ;;
        --install-deps)
            INSTALL_DEPS=true
            shift
            ;;
        --clean)
            print_status "Cleaning build artifacts..."
            cargo clean
            shift
            ;;
        *)
            print_error "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

print_header

# Set native optimizations by default
if [[ "$NATIVE" == "true" ]]; then
    export RUSTFLAGS="-C target-cpu=native"
    print_status "Building with native CPU optimizations"
fi

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    print_error "Rust/Cargo not found! Please install Rust first:"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    echo "  source ~/.cargo/env"
    exit 1
fi

# Check Rust version
RUST_VERSION=$(rustc --version | cut -d' ' -f2)
print_status "Using Rust version: $RUST_VERSION"

# Install system dependencies
install_dependencies() {
    print_status "Checking system dependencies..."
    
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        # Linux
        if command -v apt-get &> /dev/null; then
            # Ubuntu/Debian
            print_status "Installing dependencies (Ubuntu/Debian)..."
            sudo apt-get update -qq
            sudo apt-get install -y build-essential libfontconfig1-dev pkg-config
        elif command -v dnf &> /dev/null; then
            # Fedora
            print_status "Installing dependencies (Fedora)..."
            sudo dnf install -y gcc gcc-c++ fontconfig-devel pkgconf-pkg-config
        elif command -v pacman &> /dev/null; then
            # Arch Linux
            print_status "Installing dependencies (Arch Linux)..."
            sudo pacman -S --needed base-devel fontconfig pkgconf
        else
            print_warning "Unknown Linux distribution. Please install build tools manually."
        fi
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        print_status "Checking Xcode command line tools..."
        if ! xcode-select -p &> /dev/null; then
            print_status "Installing Xcode command line tools..."
            xcode-select --install
        fi
    fi
}

# Install dependencies only if requested and not cross-compiling
if [[ "$INSTALL_DEPS" == "true" && -z "$TARGET" ]]; then
    install_dependencies
fi

# Install cross-compilation target if specified
if [[ -n "$TARGET" ]]; then
    print_status "Installing target: $TARGET"
    rustup target add "$TARGET"
fi

# Build the project
print_status "Building img_viewer..."
print_status "Build type: $BUILD_TYPE"
if [[ -n "$TARGET" ]]; then
    print_status "Target: $TARGET"
fi

# Construct cargo build command
CARGO_CMD="cargo build"

if [[ "$BUILD_TYPE" == "release" ]]; then
    CARGO_CMD="$CARGO_CMD --release"
fi

if [[ -n "$TARGET" ]]; then
    CARGO_CMD="$CARGO_CMD --target $TARGET"
fi

if [[ "$VERBOSE" == "true" ]]; then
    CARGO_CMD="$CARGO_CMD --verbose"
fi

print_status "Running: $CARGO_CMD"
eval $CARGO_CMD

# Determine binary path
if [[ -n "$TARGET" ]]; then
    BINARY_PATH="target/$TARGET/$BUILD_TYPE/img_viewer"
else
    BINARY_PATH="target/$BUILD_TYPE/img_viewer"
fi

# Add .exe extension for Windows targets
if [[ "$TARGET" == *"windows"* ]]; then
    BINARY_PATH="${BINARY_PATH}.exe"
fi

# Check if binary was created
if [[ ! -f "$BINARY_PATH" ]]; then
    print_error "Build failed! Binary not found at: $BINARY_PATH"
    exit 1
fi

print_status "Build completed successfully!"
print_status "Binary location: $BINARY_PATH"

# Get binary size
BINARY_SIZE=$(du -h "$BINARY_PATH" | cut -f1)
print_status "Binary size: $BINARY_SIZE"

# Install if requested
if [[ "$INSTALL" == "true" ]]; then
    print_status "Installing binary..."
    
    # Create install directory if it doesn't exist
    if [[ ! -d "$INSTALL_DIR" ]]; then
        mkdir -p "$INSTALL_DIR"
    fi
    
    INSTALL_PATH="$INSTALL_DIR/img"

    # Remove existing binary if it exists
    if [[ -f "$INSTALL_PATH" ]]; then
        print_status "Removing existing binary at $INSTALL_PATH"
        if [[ "$SYSTEM_INSTALL" == "true" ]]; then
            sudo rm "$INSTALL_PATH"
        else
            rm "$INSTALL_PATH"
        fi
    fi

    # Copy binary
    if [[ "$SYSTEM_INSTALL" == "true" ]]; then
        sudo cp "$BINARY_PATH" "$INSTALL_PATH"
        sudo chmod +x "$INSTALL_PATH"
    else
        cp "$BINARY_PATH" "$INSTALL_PATH"
        chmod +x "$INSTALL_PATH"
    fi
    
    print_status "Binary installed to: $INSTALL_PATH"
    
    # Check if install directory is in PATH
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        print_warning "Install directory not in PATH: $INSTALL_DIR"
        print_warning "Add this to your ~/.bashrc or ~/.zshrc:"
        echo "    export PATH=\"$INSTALL_DIR:\$PATH\""
    fi
    
    # Test installation
    if command -v img &> /dev/null; then
        print_status "Installation successful! You can now run: img"
    else
        print_warning "Binary installed but not found in PATH"
    fi
fi

# Performance tips
if [[ "$BUILD_TYPE" == "release" ]]; then
    echo
    print_status "Performance Tips:"
    if [[ "$NATIVE" == "true" ]]; then
        echo "  • Native CPU optimizations enabled for maximum performance"
    else
        echo "  • For maximum performance, remove --no-native flag"
    fi
    echo "  • Use 'cargo run --release' for direct execution"
    echo "  • Large images benefit from SSD storage"
    echo "  • Close other applications for best performance"
fi

# Cross-compilation info
if [[ -n "$TARGET" ]]; then
    echo
    print_status "Cross-compilation completed for: $TARGET"
    print_status "Binary: $BINARY_PATH"
    
    if [[ "$TARGET" == *"windows"* ]]; then
        print_status "For Windows, also copy any required DLLs"
    elif [[ "$TARGET" == *"apple"* ]]; then
        print_status "For macOS, the binary should run on the target architecture"
    fi
fi

echo
print_status "Build script completed successfully!"
print_status "You can now run the image viewer from: $BINARY_PATH"