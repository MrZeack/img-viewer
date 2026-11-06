//! Configuration management for the image viewer
use serde::{Deserialize, Serialize};
use std::fs;

/// Current version of the config file schema
pub const CURRENT_CONFIG_VERSION: u32 = 2;

/// Main configuration structure for the image viewer
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Config {
    /// Schema version for config migration
    #[serde(default = "default_config_version")]
    pub version: u32,
    /// Cache-related configuration
    #[serde(default)]
    pub cache: CacheConfig,
    /// Performance-related configuration
    #[serde(default)]
    pub performance: PerformanceConfig,
    /// Browsing-related configuration
    #[serde(default)]
    pub browsing: BrowsingConfig,
    /// View-related configuration (persistence for composition and focus peaking)
    #[serde(default)]
    pub view: ViewConfig,
    /// Tag names by slot ("1".."9")
    #[serde(default)]
    pub tags: TagsConfig,
}

/// Persistent tag names; assignments live only in the session
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct TagsConfig {
    #[serde(default)]
    pub names: std::collections::BTreeMap<String, String>,
}

fn default_config_version() -> u32 {
    CURRENT_CONFIG_VERSION
}

/// Configuration for cache settings
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of images to keep in cache
    pub image_capacity: usize,
    /// Maximum number of metadata entries to keep in cache
    pub metadata_capacity: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            image_capacity: 50,
            metadata_capacity: 50,
        }
    }
}

/// Configuration for performance settings
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct PerformanceConfig {
    /// Number of threads to use for image loading
    pub loader_threads: usize,
    /// Number of adjacent images to preload
    pub preload_distance: usize,
    /// Maximum dimension for cached images
    pub max_cached_dimension: u32,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            loader_threads: 4,
            preload_distance: 2,
            max_cached_dimension: 2048,
        }
    }
}

/// Configuration for browsing settings
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BrowsingConfig {
    /// Whether to sort images by newest first
    pub sort_newest_first: bool,
}

impl Default for BrowsingConfig {
    fn default() -> Self {
        Self {
            sort_newest_first: true,
        }
    }
}

/// Configuration for persistent view settings
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ViewConfig {
    /// Whether to show composition lines by default
    pub show_composition: bool,
    /// Composition guide type name
    pub composition_type: String,
    /// Whether focus peaking is active by default
    pub show_focus_peaking: bool,
}

impl Default for ViewConfig {
    fn default() -> Self {
        Self {
            show_composition: false,
            composition_type: "RuleOfThirds".to_string(),
            show_focus_peaking: false,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CURRENT_CONFIG_VERSION,
            cache: CacheConfig::default(),
            performance: PerformanceConfig::default(),
            browsing: BrowsingConfig::default(),
            view: ViewConfig::default(),
            tags: TagsConfig::default(),
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
            Ok(content) => {
                // Try standard parse
                if let Ok(config) = toml::from_str::<Config>(&content) {
                    if config.version == CURRENT_CONFIG_VERSION {
                        return config;
                    }
                }

                // Attempt migration from legacy schema
                match migrate_config_str(&content) {
                    Ok(migrated) => {
                        println!(
                            "Migrated config.toml at {:?} to schema version {}.",
                            path, CURRENT_CONFIG_VERSION
                        );
                        save_config(&migrated);
                        return migrated;
                    }
                    Err(err) => {
                        eprintln!(
                            "Failed to parse/migrate config.toml at {:?}: {}, resetting to default config",
                            path, err
                        );
                    }
                }
            }
            Err(_) => {
                println!(
                    "config.toml not found at {:?}, creating a default one.",
                    path
                );
            }
        }
    }

    create_default_config(config_path)
}

/// Migrates an arbitrary or legacy TOML string to the current Config schema version
pub fn migrate_config_str(content: &str) -> Result<Config, toml::de::Error> {
    let table: toml::Table = toml::from_str(content)?;

    let cache = table
        .get("cache")
        .and_then(|v| v.clone().try_into().ok())
        .unwrap_or_default();

    let performance = table
        .get("performance")
        .and_then(|v| v.clone().try_into().ok())
        .unwrap_or_default();

    let browsing = table
        .get("browsing")
        .and_then(|v| v.clone().try_into().ok())
        .unwrap_or_default();

    let view = table
        .get("view")
        .and_then(|v| v.clone().try_into().ok())
        .unwrap_or_default();

    let tags = table
        .get("tags")
        .and_then(|v| v.clone().try_into().ok())
        .unwrap_or_default();

    Ok(Config {
        version: CURRENT_CONFIG_VERSION,
        cache,
        performance,
        browsing,
        view,
        tags,
    })
}

/// Creates and saves a default configuration file
fn create_default_config(path: Option<PathBuf>) -> Config {
    let config = Config::default();
    if path.is_some() {
        save_config(&config);
    }
    config
}

/// Saves the configuration to the user's config file
pub fn save_config(config: &Config) {
    if let Some(path) = get_config_path() {
        match toml::to_string_pretty(config) {
            Ok(toml_string) => {
                if let Err(e) = fs::write(&path, toml_string) {
                    eprintln!("Failed to write config.toml to {:?}: {}", path, e);
                }
            }
            Err(e) => eprintln!("Failed to serialize config: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_view_config_default_and_serialization() {
        let config = Config::default();
        assert_eq!(config.version, CURRENT_CONFIG_VERSION);
        assert!(!config.view.show_composition);
        assert_eq!(config.view.composition_type, "RuleOfThirds");
        assert!(!config.view.show_focus_peaking);

        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("version = 2"));
    }

    #[test]
    fn test_migrate_legacy_config_to_version_2() {
        let legacy_toml = r#"
            [cache]
            image_capacity = 99
            metadata_capacity = 88

            [performance]
            loader_threads = 8
            preload_distance = 4
            max_cached_dimension = 3000

            [browsing]
            sort_newest_first = false
        "#;

        let migrated = migrate_config_str(legacy_toml).expect("Should parse legacy config cleanly");
        assert_eq!(migrated.version, CURRENT_CONFIG_VERSION);
        assert_eq!(migrated.cache.image_capacity, 99);
        assert_eq!(migrated.cache.metadata_capacity, 88);
        assert_eq!(migrated.performance.loader_threads, 8);
        assert_eq!(migrated.performance.max_cached_dimension, 3000);
        assert!(!migrated.browsing.sort_newest_first);
        assert!(!migrated.view.show_composition);
        assert_eq!(migrated.view.composition_type, "RuleOfThirds");
    }

    #[test]
    fn test_tag_names_persist_and_default_to_empty() {
        let migrated = migrate_config_str("[browsing]\nsort_newest_first = true\n").unwrap();
        assert!(migrated.tags.names.is_empty());

        let mut config = Config::default();
        config
            .tags
            .names
            .insert("3".to_string(), "Familia".to_string());
        let toml_str = toml::to_string(&config).unwrap();
        let reloaded = migrate_config_str(&toml_str).unwrap();
        assert_eq!(
            reloaded.tags.names.get("3").map(String::as_str),
            Some("Familia")
        );
    }
}
