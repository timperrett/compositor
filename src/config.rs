use crate::model::{BookOrientation, BuildMode};
use crate::{AppError, ConfigError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub schema_version: u32,
    pub source: SourceConfig,
    pub state: StateConfig,
    pub assets: AssetsConfig,
    pub output: OutputConfig,
    pub ordering: OrderingConfig,
    pub markdown: MarkdownConfig,
    pub build: BuildConfig,
    pub book: BookConfig,
    pub art_layout: ArtLayoutConfig,
    pub pagination: PaginationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceConfig {
    pub compendiums_dir: String,
    pub canon_dir: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StateConfig {
    pub directory: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetsConfig {
    pub directory: String,
    pub approved_directory: String,
    pub draft_directory: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    pub directory: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OrderingConfig {
    pub filename_prefix_digits: usize,
    pub ignore_prefix: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MarkdownConfig {
    pub require_story_id: bool,
    pub require_anchor_before_approval: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildConfig {
    pub default_mode: BuildMode,
    pub similarity_threshold: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BookConfig {
    pub trim_width_in: f64,
    pub trim_height_in: f64,
    pub orientation: BookOrientation,
    pub bleed_in: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ArtLayoutConfig {
    pub pixels_per_inch: f64,
    pub spread_gutter_in: f64,
    pub single_page_portrait_width_fraction: f64,
    pub single_page_landscape_width_fraction: f64,
    pub double_page_portrait_width_fraction: f64,
    pub double_page_landscape_width_fraction: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PaginationConfig {
    pub target_words_per_text_page: usize,
    pub maximum_words_per_text_page: usize,
    pub story_starts_on_recto: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            source: SourceConfig::default(),
            state: StateConfig::default(),
            assets: AssetsConfig::default(),
            output: OutputConfig::default(),
            ordering: OrderingConfig::default(),
            markdown: MarkdownConfig::default(),
            build: BuildConfig::default(),
            book: BookConfig::default(),
            art_layout: ArtLayoutConfig::default(),
            pagination: PaginationConfig::default(),
        }
    }
}
impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            compendiums_dir: "compendiums".into(),
            canon_dir: "canon".into(),
        }
    }
}
impl Default for StateConfig {
    fn default() -> Self {
        Self {
            directory: ".compositor".into(),
        }
    }
}
impl Default for AssetsConfig {
    fn default() -> Self {
        Self {
            directory: "assets".into(),
            approved_directory: "assets/approved".into(),
            draft_directory: "assets/drafts".into(),
        }
    }
}
impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            directory: "output".into(),
        }
    }
}
impl Default for OrderingConfig {
    fn default() -> Self {
        Self {
            filename_prefix_digits: 2,
            ignore_prefix: "_".into(),
        }
    }
}
impl Default for MarkdownConfig {
    fn default() -> Self {
        Self {
            require_story_id: true,
            require_anchor_before_approval: true,
        }
    }
}
impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            default_mode: BuildMode::Conservative,
            similarity_threshold: 0.82,
        }
    }
}
impl Default for BookConfig {
    fn default() -> Self {
        Self {
            trim_width_in: 8.0,
            trim_height_in: 10.0,
            orientation: BookOrientation::Portrait,
            bleed_in: 0.125,
        }
    }
}
impl Default for PaginationConfig {
    fn default() -> Self {
        Self {
            target_words_per_text_page: 90,
            maximum_words_per_text_page: 130,
            story_starts_on_recto: true,
        }
    }
}
impl Default for ArtLayoutConfig {
    fn default() -> Self {
        Self {
            pixels_per_inch: 300.0,
            spread_gutter_in: 0.0,
            single_page_portrait_width_fraction: 0.65,
            single_page_landscape_width_fraction: 1.0,
            double_page_portrait_width_fraction: 0.45,
            double_page_landscape_width_fraction: 1.0,
        }
    }
}

impl Config {
    pub fn load(root: &Path) -> Result<Self, AppError> {
        let path = root.join("compositor.toml");
        let text = fs::read_to_string(&path).map_err(|source| {
            AppError::Config(ConfigError::Read {
                path: path.clone(),
                source,
            })
        })?;
        let value: Self = toml::from_str(&text).map_err(|source| {
            AppError::Config(ConfigError::Parse {
                path: path.clone(),
                source,
            })
        })?;
        if value.schema_version != crate::model::SCHEMA_VERSION {
            return Err(AppError::config(format!(
                "unsupported schema_version {}",
                value.schema_version
            )));
        }
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), AppError> {
        let pagination = &self.pagination;
        if pagination.target_words_per_text_page == 0 {
            return Err(AppError::config(
                "pagination.target_words_per_text_page must be greater than zero".into(),
            ));
        }
        if pagination.maximum_words_per_text_page == 0 {
            return Err(AppError::config(
                "pagination.maximum_words_per_text_page must be greater than zero".into(),
            ));
        }
        if pagination.target_words_per_text_page > pagination.maximum_words_per_text_page {
            return Err(AppError::config(
                "pagination.target_words_per_text_page must not exceed pagination.maximum_words_per_text_page".into(),
            ));
        }
        if self.book.trim_width_in <= 0.0
            || self.book.trim_height_in <= 0.0
            || self.book.bleed_in < 0.0
        {
            return Err(AppError::config(
                "book dimensions must be positive and bleed must not be negative".into(),
            ));
        }
        if self.art_layout.pixels_per_inch <= 0.0
            || self.art_layout.spread_gutter_in < 0.0
            || [
                self.art_layout.single_page_portrait_width_fraction,
                self.art_layout.single_page_landscape_width_fraction,
                self.art_layout.double_page_portrait_width_fraction,
                self.art_layout.double_page_landscape_width_fraction,
            ]
            .into_iter()
            .any(|fraction| fraction <= 0.0 || fraction > 1.0)
        {
            return Err(AppError::config(
                "art_layout dimensions and width envelopes must be positive; width envelopes must not exceed 1".into(),
            ));
        }
        Ok(())
    }

    /// Identifies the settings that determine a page plan's layout.  It is
    /// persisted with the plan so a configuration-only change invalidates the
    /// affected generated artifact without rewriting source state.
    pub fn pagination_fingerprint(&self) -> String {
        let pagination = &self.pagination;
        let input = format!(
            "pagination-v4\ntarget={}\nmaximum={}\nrecto={}\nart-layout={:?}",
            pagination.target_words_per_text_page,
            pagination.maximum_words_per_text_page,
            pagination.story_starts_on_recto,
            self.art_layout,
        );
        format!("sha256:{:x}", Sha256::digest(input.as_bytes()))
    }
    pub fn state_dir(&self, root: &Path) -> PathBuf {
        root.join(&self.state.directory)
    }
    pub fn output_dir(&self, root: &Path) -> PathBuf {
        root.join(&self.output.directory)
    }
}

pub const DEFAULT_CONFIG: &str = r#"schema_version = 2

[source]
compendiums_dir = "compendiums"
canon_dir = "canon"

[state]
directory = ".compositor"

[assets]
directory = "assets"
approved_directory = "assets/approved"
draft_directory = "assets/drafts"

[output]
directory = "output"

[ordering]
filename_prefix_digits = 2
ignore_prefix = "_"

[markdown]
require_story_id = true
require_anchor_before_approval = true

[build]
default_mode = "conservative"
similarity_threshold = 0.82

[book]
trim_width_in = 8.0
trim_height_in = 10.0
orientation = "portrait"
bleed_in = 0.125

[art_layout]
pixels_per_inch = 300.0
spread_gutter_in = 0.0
single_page_portrait_width_fraction = 0.65
single_page_landscape_width_fraction = 1.0
double_page_portrait_width_fraction = 0.45
double_page_landscape_width_fraction = 1.0

[pagination]
target_words_per_text_page = 90
maximum_words_per_text_page = 130
story_starts_on_recto = true
"#;
