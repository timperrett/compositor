use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Project configuration and report schema version.
pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceProject {
    pub compendiums: Vec<Compendium>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Compendium {
    pub id: String,
    pub title: String,
    pub source: String,
    pub ordinal: usize,
    pub stories: Vec<Story>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Story {
    pub id: String,
    pub title: String,
    pub source: String,
    pub ordinal: usize,
    pub compendium_id: String,
    pub source_hash: String,
    pub metadata: BTreeMap<String, serde_yaml::Value>,
    pub units: Vec<Unit>,
    pub paragraphs: Vec<SourceParagraph>,
    pub paragraph_comments: Vec<ParagraphComment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceParagraph {
    pub ordinal: usize,
    pub source_start: usize,
    pub source_end: usize,
    pub content: String,
    pub word_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_comment_start: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParagraphComment {
    pub raw_id: String,
    pub source_start: usize,
    pub source_end: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph_ordinal: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Unit {
    pub ordinal: usize,
    pub source_start: usize,
    pub source_end: usize,
    pub content: String,
    pub normalized_content: String,
    pub content_hash: String,
    pub word_count: usize,
    pub directives: Directives,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Directives {
    pub anchor: Option<String>,
    pub art: Option<String>,
    pub art_layout: Option<ArtLayout>,
    pub layout: Option<PageLayout>,
    pub keep_with_next: bool,
    pub unit_type: Option<UnitType>,
}

/// The authored layout treatment for a content unit.
///
/// The kebab-case serialization is part of the persisted plan and Markdown
/// directive contract.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PageLayout {
    Auto,
    TextDominant,
    ArtDominant,
    FullPage,
    FullSpread,
    FacingArt,
    SpotArt,
    IllustrationOnly,
}

impl PageLayout {
    /// Returns the stable spelling used by reports, persisted artifacts, and
    /// authored Markdown directives.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::TextDominant => "text-dominant",
            Self::ArtDominant => "art-dominant",
            Self::FullPage => "full-page",
            Self::FullSpread => "full-spread",
            Self::FacingArt => "facing-art",
            Self::SpotArt => "spot-art",
            Self::IllustrationOnly => "illustration-only",
        }
    }

    /// Returns whether this layout requires an illustration artifact.
    pub const fn requires_artwork(self) -> bool {
        !matches!(self, Self::Auto | Self::TextDominant)
    }
}

impl std::fmt::Display for PageLayout {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The finite set of non-layout unit categories accepted in Markdown.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum UnitType {
    Narrative,
    Transition,
    StoryOpening,
    StoryClosing,
    Blank,
    IllustrationOnly,
}

/// The physical orientation of a book trim size.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BookOrientation {
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtLayout {
    pub surface: ArtSurface,
    pub orientation: ArtOrientation,
    pub height_percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ArtSurface {
    SinglePage,
    DoublePageSpread,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ArtOrientation {
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtGeometry {
    pub surface_width_in: f64,
    pub height_in: f64,
    pub width_in: f64,
    pub aspect_ratio: f64,
    pub width_px: u32,
    pub height_px: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
    Blocking,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationReport {
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn can_proceed(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|issue| issue.severity == Severity::Error || issue.severity == Severity::Blocking)
    }
    pub fn is_blocking(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == Severity::Blocking)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandReport<T: Serialize> {
    pub schema_version: u32,
    pub command: String,
    pub data: T,
    pub validation: ValidationReport,
}
