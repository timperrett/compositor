use crate::config::Config;
use crate::discovery::discover;
use crate::model::{ArtGeometry, Severity, ValidationIssue, ValidationReport};
use crate::AppError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const ART_BRIEF_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtBrief {
    pub schema_version: u32,
    pub art_id: String,
    pub source: ArtBriefSource,
    #[serde(default)]
    pub usage: ArtUsage,
    #[serde(default)]
    pub context: ArtBriefContext,
    pub generation: ArtGeneration,
    #[serde(default)]
    pub candidates: Vec<ArtCandidate>,
    #[serde(default)]
    pub feedback: Vec<ArtFeedback>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtBriefSource {
    pub story_id: String,
    pub anchor_id: String,
    /// Narrative Flow Plan spreads represented by this art.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spread_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ArtUsage {
    #[default]
    Story,
    Opener,
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
    Spot,
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
pub struct ArtFeedback {
    pub candidate_id: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtBriefInspection {
    pub path: String,
    pub brief: Option<ArtBrief>,
    pub validation: ValidationReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CandidateGeometry {
    pub width_px: u32,
    pub height_px: u32,
    pub aspect_ratio: f64,
}

pub fn candidate_geometry(path: &Path) -> Result<CandidateGeometry, String> {
    let (width_px, height_px) = image::image_dimensions(path).map_err(|error| error.to_string())?;
    if width_px == 0 || height_px == 0 {
        return Err("image dimensions must be non-zero".into());
    }
    Ok(CandidateGeometry {
        width_px,
        height_px,
        aspect_ratio: f64::from(width_px) / f64::from(height_px),
    })
}

/// Accept exact geometry within two source pixels of rounding. Wide landscape
/// frames may also use a cover-crop source between 2:1 and the frame ratio;
/// layout keeps the destination frame fixed and crops the source to it.
pub fn geometry_matches(expected: &ArtGeometry, actual: &CandidateGeometry) -> bool {
    let tolerance = 2.0 / f64::from(actual.height_px);
    (actual.aspect_ratio - expected.aspect_ratio).abs() <= tolerance
        || (expected.aspect_ratio >= 2.0
            && actual.aspect_ratio >= 2.0
            && actual.aspect_ratio <= expected.aspect_ratio + tolerance)
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
    let brief: ArtBrief = serde_yaml::from_str(&text)
        .map_err(|error| AppError::serialization(format!("{}: {error}", path.display())))?;
    if brief.schema_version != ART_BRIEF_VERSION {
        return Err(AppError::command(format!(
            "{} uses art brief schema {}; run the one-time migration bridge",
            path.display(),
            brief.schema_version
        )));
    }
    Ok(Some(brief))
}

pub fn save(root: &Path, brief: &ArtBrief) -> Result<(), AppError> {
    let text =
        serde_yaml::to_string(brief).map_err(|error| AppError::serialization(error.to_string()))?;
    crate::storage::write_text_atomic(&path(root, &brief.art_id), &text)
}

pub fn inspect(root: &Path, config: &Config, art_id: &str) -> ArtBriefInspection {
    let path = path(root, art_id);
    match load(root, art_id) {
        Ok(Some(brief)) => ArtBriefInspection {
            path: relative(root, &path),
            validation: validate(root, config, &brief),
            brief: Some(brief),
        },
        Ok(None) => ArtBriefInspection {
            path: relative(root, &path),
            brief: None,
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

pub fn validate(root: &Path, config: &Config, brief: &ArtBrief) -> ValidationReport {
    let mut report = ValidationReport::default();
    let brief_path = relative(root, &path(root, &brief.art_id));
    let context = BriefValidationContext {
        root,
        brief_path: &brief_path,
        story_id: &brief.source.story_id,
        art_id: &brief.art_id,
    };
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
    let mut spread_ids = BTreeSet::new();
    for spread_id in &brief.source.spread_ids {
        if spread_id.trim().is_empty() || !spread_ids.insert(spread_id) {
            push(
                &mut report,
                Severity::Error,
                "invalid_art_brief_spread_ids",
                "source.spread_ids must contain unique, non-empty spread IDs".into(),
                &brief_path,
                Some(&brief.source.story_id),
                Some(&brief.art_id),
            );
            break;
        }
    }
    if brief.usage == ArtUsage::Opener && !brief.source.spread_ids.is_empty() {
        push(
            &mut report,
            Severity::Error,
            "opener_art_spread_link_invalid",
            "opener art must not declare source.spread_ids".into(),
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
    if brief.usage == ArtUsage::Opener && brief.generation.page_treatment != PageTreatment::Floating
    {
        push(
            &mut report,
            Severity::Error,
            "opener_art_treatment_invalid",
            "opener art must use the floating page treatment".into(),
            &brief_path,
            Some(&brief.source.story_id),
            Some(&brief.art_id),
        );
    }
    if brief.source.anchor_id.is_empty() || !crate::markdown::valid_anchor(&brief.source.anchor_id)
    {
        push(
            &mut report,
            Severity::Error,
            "invalid_art_anchor",
            "source.anchor_id must be lowercase kebab-case".into(),
            &brief_path,
            Some(&brief.source.story_id),
            Some(&brief.art_id),
        );
    }
    let mut expected_geometry = None;
    match discover(root, config) {
        Ok(project) => match project
            .compendiums
            .iter()
            .flat_map(|compendium| &compendium.stories)
            .find(|story| story.id == brief.source.story_id)
        {
            Some(story) => match story
                .units
                .iter()
                .find(|unit| unit.directives.anchor.as_deref() == Some(&brief.source.anchor_id))
            {
                Some(unit) => {
                    expected_geometry = unit
                        .directives
                        .art_layout
                        .as_ref()
                        .map(|layout| crate::art::geometry(config, layout));
                }
                None => push(
                    &mut report,
                    Severity::Error,
                    "art_brief_anchor_missing",
                    "source.anchor_id does not exist in the named story".into(),
                    &brief_path,
                    Some(&brief.source.story_id),
                    Some(&brief.art_id),
                ),
            },
            None => push(
                &mut report,
                Severity::Error,
                "art_brief_story_missing",
                "source.story_id does not exist".into(),
                &brief_path,
                Some(&brief.source.story_id),
                Some(&brief.art_id),
            ),
        },
        Err(error) => push(
            &mut report,
            Severity::Error,
            "art_brief_source_unavailable",
            error.to_string(),
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
        let path = validate_file(
            &context,
            &candidate.file,
            FileRole::CandidateImage,
            &mut report,
        );
        if let (Some(path), Some(expected)) = (path, expected_geometry.as_ref()) {
            validate_candidate_geometry(&context, &candidate.file, &path, expected, &mut report);
        }
    }
    for reference in &brief.context.canon_references {
        validate_file(&context, reference, FileRole::CanonReference, &mut report);
    }
    for feedback in &brief.feedback {
        if !ids.contains(&feedback.candidate_id) {
            push(
                &mut report,
                Severity::Error,
                "unknown_feedback_candidate",
                format!(
                    "feedback references unknown candidate `{}`",
                    feedback.candidate_id
                ),
                &brief_path,
                Some(&brief.source.story_id),
                Some(&brief.art_id),
            );
        }
    }
    report
}

struct BriefValidationContext<'a> {
    root: &'a Path,
    brief_path: &'a str,
    story_id: &'a str,
    art_id: &'a str,
}

#[derive(Debug, Clone, Copy)]
enum FileRole {
    CandidateImage,
    CanonReference,
}

impl FileRole {
    const fn label(self) -> &'static str {
        match self {
            Self::CandidateImage => "candidate",
            Self::CanonReference => "reference",
        }
    }

    const fn requires_image_format(self) -> bool {
        matches!(self, Self::CandidateImage)
    }
}

fn validate_file(
    context: &BriefValidationContext<'_>,
    value: &str,
    role: FileRole,
    report: &mut ValidationReport,
) -> Option<PathBuf> {
    let kind = role.label();
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
            context.brief_path,
            Some(context.story_id),
            Some(context.art_id),
        );
        return None;
    }
    let path = context.root.join(candidate);
    if !path.is_file() {
        push(
            report,
            Severity::Error,
            "missing_art_brief_file",
            format!("{kind} file does not exist: `{value}`"),
            context.brief_path,
            Some(context.story_id),
            Some(context.art_id),
        );
        return None;
    }
    if role.requires_image_format() {
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
                context.brief_path,
                Some(context.story_id),
                Some(context.art_id),
            );
            return None;
        }
    }
    Some(path)
}

fn validate_candidate_geometry(
    context: &BriefValidationContext<'_>,
    value: &str,
    path: &Path,
    expected: &ArtGeometry,
    report: &mut ValidationReport,
) {
    match candidate_geometry(path) {
        Ok(actual) if geometry_matches(expected, &actual) => {}
        Ok(actual) => push(
            report,
            Severity::Error,
            "candidate_geometry_incompatible",
            format!(
                "candidate `{value}` is {}x{} ({:.6}:1); expected {:.6}:1 or a 2:1-to-{:.6}:1 cover-crop source for {}x{} geometry",
                actual.width_px,
                actual.height_px,
                actual.aspect_ratio,
                expected.aspect_ratio,
                expected.aspect_ratio,
                expected.width_px,
                expected.height_px,
            ),
            context.brief_path,
            Some(context.story_id),
            Some(context.art_id),
        ),
        Err(error) => push(
            report,
            Severity::Error,
            "candidate_geometry_unreadable",
            format!("candidate `{value}` could not be read: {error}"),
            context.brief_path,
            Some(context.story_id),
            Some(context.art_id),
        ),
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
    use super::{candidate_geometry, geometry_matches, ArtBrief, CandidateGeometry, PageTreatment};
    use crate::model::ArtGeometry;

    const BRIEF: &str = r#"
schema_version: 3
art_id: reveal
source:
  story_id: story
  anchor_id: reveal
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

        let spot: ArtBrief = serde_yaml::from_str(&BRIEF.replace("floating", "spot")).unwrap();
        assert_eq!(spot.generation.page_treatment, PageTreatment::Spot);
    }

    #[test]
    fn accepts_legacy_and_spread_linked_story_briefs() {
        let legacy: ArtBrief = serde_yaml::from_str(BRIEF).unwrap();
        assert!(legacy.source.spread_ids.is_empty());

        let linked: ArtBrief = serde_yaml::from_str(&BRIEF.replace(
            "  anchor_id: reveal\n",
            "  anchor_id: reveal\n  spread_ids: [spread-002, spread-003]\n",
        ))
        .unwrap();
        assert_eq!(linked.source.spread_ids, ["spread-002", "spread-003"]);
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

    #[test]
    fn accepts_exact_and_cover_crop_geometry_sources() {
        let expected = ArtGeometry {
            surface_width_in: 16.0,
            width_in: 16.0,
            height_in: 5.5,
            aspect_ratio: 16.0 / 5.5,
            width_px: 4800,
            height_px: 1650,
        };
        let rounded_match = CandidateGeometry {
            width_px: 2138,
            height_px: 735,
            aspect_ratio: 2138.0 / 735.0,
        };
        let near_exact_portrait_match = CandidateGeometry {
            width_px: 905,
            height_px: 1738,
            aspect_ratio: 905.0 / 1738.0,
        };
        let mismatch = CandidateGeometry {
            width_px: 1735,
            height_px: 906,
            aspect_ratio: 1735.0 / 906.0,
        };
        let cover_crop = CandidateGeometry {
            width_px: 1983,
            height_px: 793,
            aspect_ratio: 1983.0 / 793.0,
        };
        assert!(geometry_matches(&expected, &rounded_match));
        assert!(!geometry_matches(&expected, &mismatch));
        assert!(geometry_matches(&expected, &cover_crop));

        let portrait_expected = ArtGeometry {
            surface_width_in: 8.0,
            width_in: 5.2,
            height_in: 10.0,
            aspect_ratio: 0.52,
            width_px: 1560,
            height_px: 3000,
        };
        assert!(geometry_matches(
            &portrait_expected,
            &near_exact_portrait_match
        ));
    }

    #[test]
    fn reads_candidate_pixels_and_rejects_corrupt_images() {
        let directory = tempfile::tempdir().unwrap();
        let valid = directory.path().join("candidate.png");
        image::RgbaImage::new(2138, 735).save(&valid).unwrap();
        assert_eq!(candidate_geometry(&valid).unwrap().width_px, 2138);
        let corrupt = directory.path().join("corrupt.png");
        std::fs::write(&corrupt, "not an image").unwrap();
        assert!(candidate_geometry(&corrupt).is_err());
    }
}
