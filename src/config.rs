//! Configuration management for the image viewer.
use crate::scoring::{ScoreWeights, AESTHETIC_WEIGHT, EXPOSURE_WEIGHT, SHARPNESS_WEIGHT};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Current version of the config file schema.
pub const CURRENT_CONFIG_VERSION: u32 = 2;

/// Main configuration structure for the image viewer.
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
    /// Automatic rating weights (recomputed on read without regenerating thumbnails)
    #[serde(default)]
    pub scoring: ScoringConfig,
    /// Tag names by slot ("1".."9")
    #[serde(default)]
    pub tags: TagsConfig,
}

/// Persistent tag names; assignments live only in the session.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct TagsConfig {
    #[serde(default)]
    pub names: std::collections::BTreeMap<String, String>,
}

fn default_config_version() -> u32 {
    CURRENT_CONFIG_VERSION
}

fn default_image_capacity() -> usize {
    50
}

fn default_metadata_capacity() -> usize {
    50
}

fn default_image_max_mb() -> usize {
    512
}

/// Configuration for cache settings.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of images to keep in cache
    #[serde(default = "default_image_capacity")]
    pub image_capacity: usize,
    /// Maximum number of metadata entries to keep in cache
    #[serde(default = "default_metadata_capacity")]
    pub metadata_capacity: usize,
    /// Approximate memory budget for cached decoded images in MiB
    #[serde(default = "default_image_max_mb")]
    pub image_max_mb: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            image_capacity: default_image_capacity(),
            metadata_capacity: default_metadata_capacity(),
            image_max_mb: default_image_max_mb(),
        }
    }
}

fn default_loader_threads() -> usize {
    4
}

fn default_preload_distance() -> usize {
    2
}

fn default_max_cached_dimension() -> u32 {
    2048
}

/// Configuration for performance settings.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct PerformanceConfig {
    /// Number of threads to use for image loading
    #[serde(default = "default_loader_threads")]
    pub loader_threads: usize,
    /// Number of adjacent images to preload
    #[serde(default = "default_preload_distance")]
    pub preload_distance: usize,
    /// Maximum dimension for cached images
    #[serde(default = "default_max_cached_dimension")]
    pub max_cached_dimension: u32,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            loader_threads: default_loader_threads(),
            preload_distance: default_preload_distance(),
            max_cached_dimension: default_max_cached_dimension(),
        }
    }
}

/// Configuration for browsing settings.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BrowsingConfig {
    /// Display order: date_asc (default), date_desc, score_desc, score_asc.
    /// Replaces the legacy `sort_newest_first`, which is ignored.
    #[serde(default = "default_sort_order")]
    pub sort_order: String,
}

fn default_sort_order() -> String {
    "date_asc".to_string()
}

impl Default for BrowsingConfig {
    fn default() -> Self {
        Self {
            sort_order: default_sort_order(),
        }
    }
}

fn default_composition_type() -> String {
    "RuleOfThirds".to_string()
}

/// Configuration for persistent view settings.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ViewConfig {
    /// Whether to show composition lines by default
    #[serde(default)]
    pub show_composition: bool,
    /// Composition guide type name
    #[serde(default = "default_composition_type")]
    pub composition_type: String,
    /// Whether focus peaking is active by default
    #[serde(default)]
    pub show_focus_peaking: bool,
}

impl Default for ViewConfig {
    fn default() -> Self {
        Self {
            show_composition: false,
            composition_type: default_composition_type(),
            show_focus_peaking: false,
        }
    }
}

fn default_aesthetic_weight() -> f32 {
    AESTHETIC_WEIGHT
}

fn default_sharpness_weight() -> f32 {
    SHARPNESS_WEIGHT
}

fn default_exposure_weight() -> f32 {
    EXPOSURE_WEIGHT
}

/// Weights for automatic 1-10 photo rating. Normalized automatically on load.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ScoringConfig {
    #[serde(default = "default_aesthetic_weight")]
    pub aesthetic_weight: f32,
    #[serde(default = "default_sharpness_weight")]
    pub sharpness_weight: f32,
    #[serde(default = "default_exposure_weight")]
    pub exposure_weight: f32,
}

impl ScoringConfig {
    pub fn weights(&self) -> ScoreWeights {
        ScoreWeights::normalized(
            self.aesthetic_weight,
            self.sharpness_weight,
            self.exposure_weight,
        )
    }
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            aesthetic_weight: default_aesthetic_weight(),
            sharpness_weight: default_sharpness_weight(),
            exposure_weight: default_exposure_weight(),
        }
    }
}

impl Config {
    /// Clamps numeric bounds and normalizes scoring weights so invalid manual edits cannot panic.
    pub fn sanitized(mut self) -> Self {
        self.cache.image_capacity = self.cache.image_capacity.max(1);
        self.cache.metadata_capacity = self.cache.metadata_capacity.max(1);
        self.cache.image_max_mb = self.cache.image_max_mb.clamp(32, 16_384);
        self.performance.loader_threads = self.performance.loader_threads.clamp(1, 64);
        self.performance.max_cached_dimension =
            self.performance.max_cached_dimension.clamp(256, 16_384);
        let w = self.scoring.weights();
        self.scoring.aesthetic_weight = w.aesthetic;
        self.scoring.sharpness_weight = w.sharpness;
        self.scoring.exposure_weight = w.exposure;
        self
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
            scoring: ScoringConfig::default(),
            tags: TagsConfig::default(),
        }
    }
}

/// Gets the path to the configuration file in the user's config directory.
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
    match get_config_path() {
        Some(path) => load_config_from(&path),
        None => Config::default(),
    }
}

pub fn load_config_from(path: &Path) -> Config {
    match fs::read_to_string(path) {
        Ok(content) => {
            if let Ok(config) = toml::from_str::<Config>(&content) {
                if config.version == CURRENT_CONFIG_VERSION {
                    return config.sanitized();
                }
            }

            match migrate_config_str(&content) {
                Ok(migrated) => {
                    println!(
                        "Migrated config.toml at {:?} to schema version {}.",
                        path, CURRENT_CONFIG_VERSION
                    );
                    let _ = save_config_to(path, &migrated);
                    return migrated;
                }
                Err(err) => {
                    let backup = path.with_extension("toml.bak");
                    if let Err(bak_err) = fs::copy(path, &backup) {
                        eprintln!("Failed to back up corrupt config {:?}: {}", path, bak_err);
                    }
                    eprintln!(
                        "Failed to parse/migrate config.toml at {:?}: {}, backed up to {:?} and resetting to default config",
                        path, err, backup
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

    let config = Config::default();
    let _ = save_config_to(path, &config);
    config
}

/// Migrates an arbitrary or legacy TOML string to the current Config schema version.
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

    let scoring = table
        .get("scoring")
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
        scoring,
        tags,
    }
    .sanitized())
}

/// Saves the configuration atomically to `path` (temp file + sync + rename).
pub fn save_config_to(path: &Path, config: &Config) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let toml_string = toml::to_string_pretty(config)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let tmp = path.with_extension(format!("toml.tmp.{}", std::process::id()));
    {
        let mut file = File::create(&tmp)?;
        file.write_all(toml_string.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)
}

/// Saves the configuration to the user's config file.
pub fn save_config(config: &Config) {
    if let Some(path) = get_config_path() {
        if let Err(e) = save_config_to(&path, config) {
            eprintln!("Failed to save config.toml to {:?}: {}", path, e);
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
        assert_eq!(
            migrated.browsing.sort_order, "date_asc",
            "el flag legado se ignora"
        );
        assert!(!migrated.view.show_composition);
        assert_eq!(migrated.view.composition_type, "RuleOfThirds");
    }

    #[test]
    fn test_tag_names_persist_and_default_to_empty() {
        let migrated = migrate_config_str("[browsing]\nsort_order = \"score_desc\"\n").unwrap();
        assert_eq!(migrated.browsing.sort_order, "score_desc");
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

    #[test]
    fn zero_and_invalid_bounds_are_sanitized() {
        let bad = r#"
            version = 2
            [cache]
            image_capacity = 0
            metadata_capacity = 0
            image_max_mb = 1
            [performance]
            loader_threads = 0
            preload_distance = 2
            max_cached_dimension = 10
            [scoring]
            aesthetic_weight = -1.0
            sharpness_weight = 0.0
            exposure_weight = 0.0
        "#;
        let cfg = migrate_config_str(bad).unwrap();
        assert_eq!(cfg.cache.image_capacity, 1);
        assert_eq!(cfg.cache.metadata_capacity, 1);
        assert_eq!(cfg.cache.image_max_mb, 32);
        assert_eq!(cfg.performance.loader_threads, 1);
        assert_eq!(cfg.performance.max_cached_dimension, 256);
        let sum = cfg.scoring.aesthetic_weight
            + cfg.scoring.sharpness_weight
            + cfg.scoring.exposure_weight;
        assert!((sum - 1.0).abs() < 1e-5, "sum={sum}");
    }

    #[test]
    fn partial_sections_use_field_defaults() {
        let partial = r#"
            version = 2
            [view]
            show_composition = true
            [scoring]
            aesthetic_weight = 0.5
            sharpness_weight = 0.25
            exposure_weight = 0.25
        "#;
        let cfg = migrate_config_str(partial).unwrap();
        assert!(cfg.view.show_composition);
        assert_eq!(cfg.view.composition_type, "RuleOfThirds");
        assert!(!cfg.view.show_focus_peaking);
        assert!((cfg.scoring.aesthetic_weight - 0.5).abs() < 1e-5);
        assert!((cfg.scoring.sharpness_weight - 0.25).abs() < 1e-5);
    }

    #[test]
    fn corrupt_config_is_backed_up_before_reset() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("img-viewer-cfg-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, "not valid = [ toml").unwrap();

        let cfg = load_config_from(&path);
        assert_eq!(cfg.version, CURRENT_CONFIG_VERSION);
        let bak = dir.join("config.toml.bak");
        assert!(bak.exists(), "corrupt config must be backed up to .bak");
        assert_eq!(fs::read_to_string(&bak).unwrap(), "not valid = [ toml");
        let reloaded = fs::read_to_string(&path).unwrap();
        assert!(toml::from_str::<Config>(&reloaded).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }
}
