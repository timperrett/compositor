use crate::model::BookOrientation;
use crate::{AppError, ConfigError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub source: SourceConfig,
    pub assets: AssetsConfig,
    pub output: OutputConfig,
    pub ordering: OrderingConfig,
    pub markdown: MarkdownConfig,
    pub book: BookConfig,
    pub art_layout: ArtLayoutConfig,
    pub editorial: EditorialConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceConfig {
    pub compendiums_dir: String,
    pub canon_dir: String,
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
    /// The smallest allowed derived aspect ratio for a landscape frame.
    /// Authors select only `orientation`; this is a project-level policy.
    pub minimum_landscape_aspect_ratio: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct EditorialConfig {
    pub paragraph_economy: Option<ParagraphEconomyConfig>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParagraphEconomyConfig {
    pub minimum_words: usize,
    pub max_paragraphs_per_100_words: f64,
    pub short_paragraph_max_words: usize,
    pub max_consecutive_short_paragraphs: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            source: SourceConfig::default(),
            assets: AssetsConfig::default(),
            output: OutputConfig::default(),
            ordering: OrderingConfig::default(),
            markdown: MarkdownConfig::default(),
            book: BookConfig::default(),
            art_layout: ArtLayoutConfig::default(),
            editorial: EditorialConfig::default(),
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
impl Default for ArtLayoutConfig {
    fn default() -> Self {
        Self {
            pixels_per_inch: 300.0,
            spread_gutter_in: 0.0,
            minimum_landscape_aspect_ratio: 4.0 / 3.0,
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

    pub(crate) fn validate(&self) -> Result<(), AppError> {
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
            || self.art_layout.minimum_landscape_aspect_ratio <= 1.0
        {
            return Err(AppError::config(
                "art_layout pixels per inch must be positive, the spread gutter must not be negative, and the minimum landscape aspect ratio must be greater than 1".into(),
            ));
        }
        if let Some(economy) = &self.editorial.paragraph_economy {
            if economy.minimum_words == 0
                || economy.max_paragraphs_per_100_words <= 0.0
                || economy.short_paragraph_max_words == 0
                || economy.max_consecutive_short_paragraphs == 0
            {
                return Err(AppError::config(
                    "editorial.paragraph_economy values must all be greater than zero".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn output_dir(&self, root: &Path) -> PathBuf {
        root.join(&self.output.directory)
    }
}

pub const DEFAULT_CONFIG: &str = r#"schema_version = 2

[source]
compendiums_dir = "compendiums"
canon_dir = "canon"

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

[book]
trim_width_in = 8.0
trim_height_in = 10.0
orientation = "portrait"
bleed_in = 0.125

[art_layout]
pixels_per_inch = 300.0
spread_gutter_in = 0.0
minimum_landscape_aspect_ratio = 1.3333333333333333

"#;
