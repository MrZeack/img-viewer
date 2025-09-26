use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    pub cache: CacheConfig,
    pub performance: PerformanceConfig,
    pub browsing: BrowsingConfig,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct CacheConfig {
    pub image_capacity: usize,
    pub metadata_capacity: usize,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct PerformanceConfig {
    pub loader_threads: usize,
    pub preload_distance: usize,
    pub max_cached_dimension: u32,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct BrowsingConfig {
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
        Ok(content) => toml::from_str(&content).unwrap_or_else(|err| {
            eprintln!("Failed to parse config.toml: {}, using default config", err);
            Config::default()
        }),
        Err(_) => {
            println!("config.toml not found, creating a default one.");
            let config = Config::default();
            if let Ok(toml_string) = toml::to_string_pretty(&config) {
                if fs::write("config.toml", toml_string).is_err() {
                    eprintln!("Failed to write default config.toml");
                }
            } else {
                eprintln!("Failed to serialize default config");
            }
            config
        }
    }
}
