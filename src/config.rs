//! Configuration management for the image viewer
use serde::{Deserialize, Serialize};
use std::fs;

/// Main configuration structure for the image viewer
#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    /// Cache-related configuration
    pub cache: CacheConfig,
    /// Performance-related configuration
    pub performance: PerformanceConfig,
    /// Browsing-related configuration
    pub browsing: BrowsingConfig,
}

/// Configuration for cache settings
#[derive(Deserialize, Serialize, Debug)]
pub struct CacheConfig {
    /// Maximum number of images to keep in cache
    pub image_capacity: usize,
    /// Maximum number of metadata entries to keep in cache
    pub metadata_capacity: usize,
}

/// Configuration for performance settings
#[derive(Deserialize, Serialize, Debug)]
pub struct PerformanceConfig {
    /// Number of threads to use for image loading
    pub loader_threads: usize,
    /// Number of adjacent images to preload
    pub preload_distance: usize,
    /// Maximum dimension for cached images
    pub max_cached_dimension: u32,
}

/// Configuration for browsing settings
#[derive(Deserialize, Serialize, Debug)]
pub struct BrowsingConfig {
    /// Whether to sort images by newest first
    pub sort_newest_first: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cache: CacheConfig {
                image_capacity: 50,
                metadata_capacity: 50,
            },
            performance: PerformanceConfig {
                loader_threads: 4,
                preload_distance: 2,
                max_cached_dimension: 2048,
            },
            browsing: BrowsingConfig {
                sort_newest_first: true,
            },
        }
    }
}

use directories::ProjectDirs;
use std::path::PathBuf;

/// Gets the path to the configuration file in the user's config directory
fn get_config_path() -> Option<PathBuf> {
    ProjectDirs::from("fyi", "voza", "img-viewer").map(|proj_dirs| {
        let config_dir = proj_dirs.config_dir();
        if !config_dir.exists() {
            let _ = fs::create_dir_all(config_dir);
        }
        config_dir.join("config.toml")
    })
}

pub fn load_config() -> Config {
    let config_path = get_config_path();

    if let Some(path) = &config_path {
        match fs::read_to_string(path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => return config,
                Err(err) => {
                    eprintln!("Failed to parse config.toml at {:?}: {}, using default config", path, err);
                }
            },
            Err(_) => {
                println!("config.toml not found at {:?}, creating a default one.", path);
            }
        }
    }

    create_default_config(config_path)
}

/// Creates and saves a default configuration file
fn create_default_config(path: Option<PathBuf>) -> Config {
    let config = Config::default();
    if let Some(path) = path {
        match toml::to_string_pretty(&config) {
            Ok(toml_string) => {
                if let Err(e) = fs::write(path, toml_string) {
                    eprintln!("Failed to write default config.toml: {}", e);
                }
            }
            Err(e) => eprintln!("Failed to serialize default config: {}", e),
        }
    }
    config
}
