@echo off
:: Image Viewer - Rust Edition Build Script for Windows
:: Builds and optionally installs the high-performance image viewer

setlocal enabledelayedexpansion

:: Default values
set BUILD_TYPE=release
set INSTALL=false
set INSTALL_DIR=%USERPROFILE%\AppData\Local\bin
set VERBOSE=false
set NATIVE_OPT=false
set CLEAN=false

:: Colors (Windows 10+ support)
set GREEN=[92m
set RED=[91m
set YELLOW=[93m
set BLUE=[94m
set NC=[0m

echo %BLUE%=================================%NC%
echo %BLUE% Image Viewer - Windows Build    %NC%
echo %BLUE%=================================%NC%
echo.

:: Parse command line arguments
:parse_args
if "%~1"=="" goto :check_rust
if "%~1"=="--help" goto :show_help
if "%~1"=="-h" goto :show_help
if "%~1"=="--debug" (
    set BUILD_TYPE=debug
    shift
    goto :parse_args
)
if "%~1"=="--release" (
    set BUILD_TYPE=release
    shift
    goto :parse_args
)
if "%~1"=="--install" (
    set INSTALL=true
    shift
    goto :parse_args
)
if "%~1"=="--verbose" (
    set VERBOSE=true
    shift
    goto :parse_args
)
if "%~1"=="--native" (
    set NATIVE_OPT=true
    set RUSTFLAGS=-C target-cpu=native
    shift
    goto :parse_args
)
if "%~1"=="--clean" (
    set CLEAN=true
    shift
    goto :parse_args
)
echo %RED%[ERROR]%NC% Unknown option: %~1
echo Use --help for usage information
exit /b 1

:show_help
echo Usage: %0 [OPTIONS]
echo.
echo Build and install the Rust Image Viewer on Windows
echo.
echo OPTIONS:
echo     -h, --help          Show this help message
echo     --debug             Build in debug mode (faster compile, slower runtime)
echo     --release           Build in release mode (default, optimized)
echo     --install           Install binary after building
echo     --verbose           Verbose output
echo     --native            Optimize for current CPU (maximum performance)
echo     --clean             Clean build artifacts before building
echo.
echo EXAMPLES:
echo     %0                  # Build in release mode
echo     %0 --debug          # Build in debug mode  
echo     %0 --install        # Build and install
echo     %0 --native         # Build with CPU-specific optimizations
echo.
exit /b 0

:check_rust
:: Check if Rust is installed
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo %RED%[ERROR]%NC% Rust/Cargo not found! Please install Rust first:
    echo   Visit: https://rustup.rs/
    echo   Or run: winget install --id Rust.Rust
    exit /b 1
)

:: Display Rust version
for /f "tokens=2" %%i in ('cargo --version') do set RUST_VERSION=%%i
echo %GREEN%[INFO]%NC% Using Rust version: !RUST_VERSION!

:: Check for Cargo.toml
if not exist "Cargo.toml" (
    echo %RED%[ERROR]%NC% Cargo.toml not found! Please run from the project root directory.
    exit /b 1
)

:: Clean if requested
if "%CLEAN%"=="true" (
    echo %GREEN%[INFO]%NC% Cleaning build artifacts...
    cargo clean
)

:: Install Visual Studio Build Tools if needed (Windows specific)
echo %GREEN%[INFO]%NC% Checking for Visual Studio Build Tools...
where cl >nul 2>nul
if %errorlevel% neq 0 (
    echo %YELLOW%[WARN]%NC% Visual Studio Build Tools not found in PATH
    echo %YELLOW%[WARN]%NC% You may need to install Visual Studio Build Tools or run from Developer Command Prompt
)

:: Build the project
echo %GREEN%[INFO]%NC% Building img_viewer...
echo %GREEN%[INFO]%NC% Build type: %BUILD_TYPE%

if "%NATIVE_OPT%"=="true" (
    echo %GREEN%[INFO]%NC% Building with native CPU optimizations...
    set RUSTFLAGS=%RUSTFLAGS%
)

:: Construct cargo build command
set CARGO_CMD=cargo build --locked

if "%BUILD_TYPE%"=="release" (
    set CARGO_CMD=!CARGO_CMD! --release
)

if "%VERBOSE%"=="true" (
    set CARGO_CMD=!CARGO_CMD! --verbose
)

echo %GREEN%[INFO]%NC% Running: !CARGO_CMD!
!CARGO_CMD!

if %errorlevel% neq 0 (
    echo %RED%[ERROR]%NC% Build failed!
    exit /b 1
)

:: Determine binary path
set BINARY_PATH=target\%BUILD_TYPE%\img_viewer.exe

:: Check if binary was created
if not exist "%BINARY_PATH%" (
    echo %RED%[ERROR]%NC% Build failed! Binary not found at: %BINARY_PATH%
    exit /b 1
)

echo %GREEN%[INFO]%NC% Build completed successfully!
echo %GREEN%[INFO]%NC% Binary location: %BINARY_PATH%

:: Get binary size
for %%i in ("%BINARY_PATH%") do set BINARY_SIZE=%%~zi
set /a BINARY_SIZE_MB=%BINARY_SIZE% / 1048576
echo %GREEN%[INFO]%NC% Binary size: %BINARY_SIZE_MB% MB

:: Install if requested
if "%INSTALL%"=="true" (
    echo %GREEN%[INFO]%NC% Installing binary...
    
    :: Create install directory if it doesn't exist
    if not exist "%INSTALL_DIR%" (
        mkdir "%INSTALL_DIR%"
    )
    
    set INSTALL_PATH=%INSTALL_DIR%\img.exe
    
    :: Copy binary
    copy "%BINARY_PATH%" "!INSTALL_PATH!" >nul
    if %errorlevel% neq 0 (
        echo %RED%[ERROR]%NC% Failed to install binary
        exit /b 1
    )
    
    echo %GREEN%[INFO]%NC% Binary installed to: !INSTALL_PATH!
    
    :: Check if install directory is in PATH
    echo %PATH% | find /i "%INSTALL_DIR%" >nul
    if %errorlevel% neq 0 (
        echo %YELLOW%[WARN]%NC% Install directory not in PATH: %INSTALL_DIR%
        echo %YELLOW%[WARN]%NC% Add this to your PATH environment variable to run from anywhere
        echo %YELLOW%[WARN]%NC% Or run: setx PATH "%%PATH%%;%INSTALL_DIR%"
    ) else (
        echo %GREEN%[INFO]%NC% Installation successful! You can now run: img
    )
)

echo.
echo %GREEN%[INFO]%NC% Build script completed successfully!
echo %GREEN%[INFO]%NC% You can now run the image viewer: %BINARY_PATH%
exit /b 0