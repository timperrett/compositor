use crate::config::Config;
use crate::model::{IllustrationRequirement, Severity, ValidationIssue, ValidationReport};
use crate::{storage, AppError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const ART_BRIEF_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtBrief {
    pub schema_version: u32,
    pub art_id: String,
    pub source: ArtBriefSource,
    #[serde(default)]
    pub context: ArtBriefContext,
    pub generation: ArtGeneration,
    #[serde(default)]
    pub candidates: Vec<ArtCandidate>,
    #[serde(default)]
    pub selection: Option<ArtSelection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtBriefSource {
    pub story_id: String,
    pub unit_ids: Vec<String>,
    pub requirement_revision: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtBriefContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art_note: Option<String>,
    #[serde(default)]
    pub canon_references: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtGeneration {
    #[serde(default = "exploration_mode")]
    pub mode: String,
    pub page_treatment: PageTreatment,
    pub prompt: String,
    #[serde(default)]
    pub exclusions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PageTreatment {
    Floating,
    Framed,
    FullBleed,
}

fn exploration_mode() -> String {
    "exploration".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtCandidate {
    pub id: String,
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtSelection {
    pub candidate_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtBriefInspection {
    pub path: String,
    pub brief: Option<ArtBrief>,
    pub requirement: Option<IllustrationRequirement>,
    pub validation: ValidationReport,
}

pub fn directory(root: &Path) -> PathBuf {
    root.join("art/briefs")
}

pub fn path(root: &Path, art_id: &str) -> PathBuf {
    directory(root).join(format!("{art_id}.yaml"))
}

pub fn ids(root: &Path) -> Result<Vec<String>, AppError> {
    let directory = directory(root);
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut ids = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml")
        })
        .filter_map(|path| {
            path.file_stem()
                .map(|name| name.to_string_lossy().to_string())
        })
        .collect::<Vec<_>>();
    ids.sort();
    Ok(ids)
}

pub fn load(root: &Path, art_id: &str) -> Result<Option<ArtBrief>, AppError> {
    let path = path(root, art_id);
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    serde_yaml::from_str(&text)
        .map(Some)
        .map_err(|error| AppError::Serialization(format!("{}: {error}", path.display())))
}

pub fn inspect(root: &Path, config: &Config, art_id: &str) -> ArtBriefInspection {
    let path = path(root, art_id);
    let requirement = storage::load_latest_requirement(root, config, art_id)
        .ok()
        .flatten();
    match load(root, art_id) {
        Ok(Some(brief)) => ArtBriefInspection {
            path: relative(root, &path),
            validation: validate(root, &brief, requirement.as_ref()),
            brief: Some(brief),
            requirement,
        },
        Ok(None) => ArtBriefInspection {
            path: relative(root, &path),
            brief: None,
            requirement,
            validation: report_issue(
                Severity::Warning,
                "missing_art_brief",
                format!("no art brief exists for `{art_id}`"),
                relative(root, &path),
                None,
                Some(art_id.into()),
            ),
        },
        Err(error) => ArtBriefInspection {
            path: relative(root, &path),
            brief: None,
            requirement,
            validation: report_issue(
                Severity::Error,
                "invalid_art_brief",
                error.to_string(),
                relative(root, &path),
                None,
                Some(art_id.into()),
            ),
        },
    }
}

pub fn validate(
    root: &Path,
    brief: &ArtBrief,
    requirement: Option<&IllustrationRequirement>,
) -> ValidationReport {
    let mut report = ValidationReport::default();
    let brief_path = relative(root, &path(root, &brief.art_id));
    if brief.schema_version != ART_BRIEF_VERSION {
        push(
            &mut report,
            Severity::Error,
            "unsupported_art_brief_version",
            format!("art brief schema_version must be {ART_BRIEF_VERSION}"),
            &brief_path,
            Some(&brief.source.story_id),
            Some(&brief.art_id),
        );
    }
    if brief.art_id.is_empty() || !crate::markdown::valid_anchor(&brief.art_id) {
        push(
            &mut report,
            Severity::Error,
            "invalid_art_id",
            "art_id must be lowercase hyphen-case".into(),
            &brief_path,
            Some(&brief.source.story_id),
            Some(&brief.art_id),
        );
    }
    if brief.generation.prompt.trim().is_empty() {
        push(
            &mut report,
            Severity::Error,
            "missing_generation_prompt",
            "generation.prompt must not be empty".into(),
            &brief_path,
            Some(&brief.source.story_id),
            Some(&brief.art_id),
        );
    }
    if brief.source.unit_ids.is_empty() {
        push(
            &mut report,
            Severity::Error,
            "missing_source_units",
            "source.unit_ids must not be empty".into(),
            &brief_path,
            Some(&brief.source.story_id),
            Some(&brief.art_id),
        );
    }
    match requirement {
        Some(requirement) => {
            if requirement.art_id != brief.art_id
                || requirement.story_id != brief.source.story_id
                || requirement.unit_ids != brief.source.unit_ids
            {
                push(
                    &mut report,
                    Severity::Error,
                    "art_brief_source_mismatch",
                    "brief source does not match the current illustration requirement".into(),
                    &brief_path,
                    Some(&brief.source.story_id),
                    Some(&brief.art_id),
                );
            }
            if requirement.revision != brief.source.requirement_revision {
                push(
                    &mut report,
                    Severity::Warning,
                    "stale_art_brief_requirement",
                    format!(
                        "brief targets requirement v{:03}, current requirement is v{:03}",
                        brief.source.requirement_revision, requirement.revision
                    ),
                    &brief_path,
                    Some(&brief.source.story_id),
                    Some(&brief.art_id),
                );
            }
        }
        None => push(
            &mut report,
            Severity::Error,
            "missing_art_requirement",
            "no current illustration requirement exists for this art ID".into(),
            &brief_path,
            Some(&brief.source.story_id),
            Some(&brief.art_id),
        ),
    }
    let mut ids = BTreeSet::new();
    for candidate in &brief.candidates {
        if candidate.id.is_empty() || !ids.insert(candidate.id.clone()) {
            push(
                &mut report,
                Severity::Error,
                "duplicate_candidate_id",
                format!("candidate ID `{}` is missing or duplicated", candidate.id),
                &brief_path,
                Some(&brief.source.story_id),
                Some(&brief.art_id),
            );
        }
        validate_file(
            root,
            &candidate.file,
            &brief_path,
            &brief.source.story_id,
            &brief.art_id,
            "candidate",
            true,
            &mut report,
        );
    }
    for reference in &brief.context.canon_references {
        validate_file(
            root,
            reference,
            &brief_path,
            &brief.source.story_id,
            &brief.art_id,
            "reference",
            false,
            &mut report,
        );
    }
    if let Some(selection) = &brief.selection {
        if !ids.contains(&selection.candidate_id) {
            push(
                &mut report,
                Severity::Error,
                "unknown_selected_candidate",
                format!(
                    "selection references unknown candidate `{}`",
                    selection.candidate_id
                ),
                &brief_path,
                Some(&brief.source.story_id),
                Some(&brief.art_id),
            );
        }
    }
    report
}

fn validate_file(
    root: &Path,
    value: &str,
    brief_path: &str,
    story_id: &str,
    art_id: &str,
    kind: &str,
    image: bool,
    report: &mut ValidationReport,
) {
    let candidate = Path::new(value);
    if candidate.is_absolute()
        || candidate.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        push(
            report,
            Severity::Error,
            "unsafe_art_brief_path",
            format!("{kind} path must be project-relative: `{value}`"),
            brief_path,
            Some(story_id),
            Some(art_id),
        );
        return;
    }
    let path = root.join(candidate);
    if !path.is_file() {
        push(
            report,
            Severity::Error,
            "missing_art_brief_file",
            format!("{kind} file does not exist: `{value}`"),
            brief_path,
            Some(story_id),
            Some(art_id),
        );
        return;
    }
    if image {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp") {
            push(
                report,
                Severity::Error,
                "unsupported_candidate_format",
                format!("candidate `{value}` must be PNG, JPG, JPEG, or WebP"),
                brief_path,
                Some(story_id),
                Some(art_id),
            );
        }
    }
}

fn report_issue(
    severity: Severity,
    code: &str,
    message: String,
    path: String,
    story_id: Option<String>,
    unit_id: Option<String>,
) -> ValidationReport {
    ValidationReport {
        issues: vec![ValidationIssue {
            severity,
            code: code.into(),
            message,
            path,
            story_id,
            unit_id,
        }],
    }
}

fn push(
    report: &mut ValidationReport,
    severity: Severity,
    code: &str,
    message: String,
    path: &str,
    story_id: Option<&str>,
    unit_id: Option<&str>,
) {
    report.issues.push(ValidationIssue {
        severity,
        code: code.into(),
        message,
        path: path.into(),
        story_id: story_id.map(str::to_owned),
        unit_id: unit_id.map(str::to_owned),
    });
}

pub fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::{ArtBrief, PageTreatment};

    const BRIEF: &str = r#"
schema_version: 1
art_id: reveal
source:
  story_id: story
  unit_ids: [reveal]
  requirement_revision: 1
generation:
  page_treatment: floating
  prompt: A moonlit library.
"#;

    #[test]
    fn accepts_supported_page_treatments() {
        let floating: ArtBrief = serde_yaml::from_str(BRIEF).unwrap();
        assert_eq!(floating.generation.page_treatment, PageTreatment::Floating);

        let framed: ArtBrief = serde_yaml::from_str(&BRIEF.replace("floating", "framed")).unwrap();
        assert_eq!(framed.generation.page_treatment, PageTreatment::Framed);

        let full_bleed: ArtBrief =
            serde_yaml::from_str(&BRIEF.replace("floating", "full-bleed")).unwrap();
        assert_eq!(
            full_bleed.generation.page_treatment,
            PageTreatment::FullBleed
        );
    }

    #[test]
    fn rejects_missing_legacy_or_unknown_page_treatments() {
        assert!(serde_yaml::from_str::<ArtBrief>(
            &BRIEF.replace("  page_treatment: floating\n", "")
        )
        .is_err());
        assert!(serde_yaml::from_str::<ArtBrief>(
            &BRIEF.replace("page_treatment: floating", "bleed_mode: contained")
        )
        .is_err());
        assert!(serde_yaml::from_str::<ArtBrief>(&BRIEF.replace("floating", "bordered")).is_err());
    }
}
