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

pub fn load_config() -> Config {
    match fs::read_to_string("config.toml") {
        Ok(content) => match toml::from_str(&content) {
            Ok(config) => config,
            Err(err) => {
                eprintln!("Failed to parse config.toml: {}, using default config", err);
                create_default_config()
            }
        },
        Err(_) => {
            println!("config.toml not found, creating a default one.");
            create_default_config()
        }
    }
}

/// Creates and saves a default configuration file
fn create_default_config() -> Config {
    let config = Config::default();
    match toml::to_string_pretty(&config) {
        Ok(toml_string) => {
            if let Err(e) = fs::write("config.toml", toml_string) {
                eprintln!("Failed to write default config.toml: {}", e);
            }
        }
        Err(e) => eprintln!("Failed to serialize default config: {}", e),
    }
    config
}
