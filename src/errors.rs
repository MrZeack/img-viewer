//! Error types for the image viewer application
use std::fmt;

/// Represents all possible errors in the image viewer application
#[derive(Debug)]
pub enum AppError {
    /// Error occurred while loading or processing an image
    ImageLoadError(String),
    /// Error occurred during file system operations
    FileSystemError(String),
    /// Error occurred during configuration operations
    ConfigError(String),
    /// Error occurred during cache operations
    CacheError(String),
    /// Error occurred during metadata extraction
    MetadataError(String),
    /// Standard IO error
    IoError(std::io::Error),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AppError::ImageLoadError(msg) => write!(f, "Image load error: {}", msg),
            AppError::FileSystemError(msg) => write!(f, "File system error: {}", msg),
            AppError::ConfigError(msg) => write!(f, "Config error: {}", msg),
            AppError::CacheError(msg) => write!(f, "Cache error: {}", msg),
            AppError::MetadataError(msg) => write!(f, "Metadata error: {}", msg),
            AppError::IoError(err) => write!(f, "IO error: {}", err),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        AppError::IoError(error)
    }
}

impl From<image::ImageError> for AppError {
    fn from(error: image::ImageError) -> Self {
        AppError::ImageLoadError(error.to_string())
    }
}