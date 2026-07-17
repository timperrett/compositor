use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;

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
    pub metadata: BTreeMap<String, serde_yaml::Value>,
    pub units: Vec<Unit>,
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
    pub layout: Option<String>,
    pub keep_with_next: bool,
    pub unit_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub schema_version: u32,
    pub tool_version: String,
    pub revision: u64,
    pub compendiums: BTreeMap<String, ManifestCompendium>,
    pub stories: BTreeMap<String, ManifestStory>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestCompendium {
    pub source: String,
    pub stories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestStory {
    pub source: String,
    pub source_hash: String,
    pub ordinal: usize,
    pub units: Vec<ManifestUnit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestUnit {
    pub id: String,
    pub anchor: Option<String>,
    pub ordinal: usize,
    pub content_hash: String,
    #[serde(default)]
    pub normalized_content: String,
    pub state: UnitState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UnitState {
    Active,
    Deleted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Unchanged,
    Edited,
    Inserted,
    Deleted,
    Moved,
    Split,
    Merged,
    Reordered,
    Ambiguous,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Change {
    pub kind: ChangeKind,
    pub story_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_unit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChangeSet {
    pub changes: Vec<Change>,
}

impl ChangeSet {
    pub fn has_state_changes(&self) -> bool {
        self.changes
            .iter()
            .any(|change| change.kind != ChangeKind::Unchanged)
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PagePlan {
    pub schema_version: u32,
    pub story_id: String,
    pub manifest_revision: u64,
    pub revision: u64,
    /// The pagination settings used to generate this plan. Empty means a
    /// legacy plan, which is intentionally treated as stale.
    #[serde(default)]
    pub pagination_fingerprint: String,
    pub assignments: Vec<PageAssignment>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageAssignment {
    pub pages: Vec<u32>,
    pub units: Vec<String>,
    pub layout: String,
    pub word_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resolutions {
    pub mappings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandReport<T: Serialize> {
    pub schema_version: u32,
    pub command: String,
    pub data: T,
    pub validation: ValidationReport,
}
