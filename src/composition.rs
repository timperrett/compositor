use crate::art_brief::{self, ArtUsage};
use crate::flow::StoryFlowPlan;
use crate::model::{Severity, Story, ValidationIssue, ValidationReport};
use crate::AppError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub const COMPOSITION_PLAN_SCHEMA: &str = "compositor.dev/composition-plan/v2";
const DESIGN_SYSTEM_SCHEMA: &str = "compositor.dev/design-system/v1";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompositionPlan {
    pub schema: String,
    pub story: CompositionStory,
    pub edition: Edition,
    pub opener: StoryOpener,
    pub spreads: Vec<CompositionSpread>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompositionStory {
    pub id: String,
    pub flow: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Edition {
    pub id: String,
    pub design_system: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StoryOpener {
    pub title: String,
    pub placement: OpenerPlacement,
    pub art: ArtReference,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OpenerPlacement {
    CenterPage,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompositionSpread {
    pub id: String,
    pub layout: LayoutChoice,
    pub text: TextIntent,
    pub illustration: IllustrationIntent,
    #[serde(default)]
    pub art_assets: Vec<ArtReference>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LayoutChoice {
    pub family: String,
    pub variant: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TextIntent {
    pub density: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IllustrationIntent {
    pub mode: String,
    pub focal_subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiet_region: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtReference {
    pub id: String,
    pub role: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct DesignDescriptor {
    schema: String,
    id: String,
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    version: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RolesFile {
    #[serde(default)]
    roles: BTreeMap<String, RoleDefinition>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RoleDefinition {
    #[allow(dead_code)]
    #[serde(default)]
    energy: Option<serde_yaml::Value>,
    #[serde(default)]
    text_density: DensityRule,
    #[serde(default)]
    compatible_layout_families: BTreeSet<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct DensityRule {
    #[serde(default)]
    allowed: BTreeSet<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct LayoutFamiliesFile {
    #[serde(default)]
    layout_families: BTreeMap<String, LayoutFamily>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct LayoutFamily {
    #[serde(default)]
    compatible_roles: BTreeSet<String>,
    #[serde(default)]
    variants: BTreeMap<String, LayoutVariant>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct LayoutVariant {
    #[serde(default)]
    text_density: DensityRule,
    #[serde(default)]
    requires_quiet_region: bool,
}

#[derive(Debug, Clone)]
pub struct DesignCatalog {
    pub id: String,
    roles: BTreeMap<String, RoleDefinition>,
    families: BTreeMap<String, LayoutFamily>,
}

pub fn load_plan(path: &Path) -> Result<CompositionPlan, AppError> {
    let text = fs::read_to_string(path)?;
    serde_yaml::from_str(&text)
        .map_err(|error| AppError::serialization(format!("{}: {error}", path.display())))
}

pub fn load_catalog(directory: &Path) -> Result<DesignCatalog, AppError> {
    let descriptor: DesignDescriptor = read_yaml(&directory.join("design-system.yaml"))?;
    if descriptor.schema != DESIGN_SYSTEM_SCHEMA {
        return Err(AppError::config(format!(
            "unsupported design system schema `{}`",
            descriptor.schema
        )));
    }
    let roles: RolesFile = read_yaml(&directory.join("spread-roles.yaml"))?;
    let families: LayoutFamiliesFile = read_yaml(&directory.join("layout-families.yaml"))?;
    Ok(DesignCatalog {
        id: descriptor.id,
        roles: roles.roles,
        families: families.layout_families,
    })
}

fn read_yaml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, AppError> {
    let text = fs::read_to_string(path)?;
    serde_yaml::from_str(&text)
        .map_err(|error| AppError::serialization(format!("{}: {error}", path.display())))
}

pub fn validate(
    flow: &StoryFlowPlan,
    plan: &CompositionPlan,
    design: &DesignCatalog,
) -> ValidationReport {
    let mut report = ValidationReport::default();
    let path = &plan.story.flow;
    if plan.schema != COMPOSITION_PLAN_SCHEMA {
        issue(
            &mut report,
            "COMPOSITION_SCHEMA_UNSUPPORTED",
            "unsupported composition schema",
            path,
            None,
        );
    }
    if plan.story.id != flow.story.id {
        issue(
            &mut report,
            "COMPOSITION_STORY_MISMATCH",
            "composition story does not match flow story",
            path,
            None,
        );
    }
    if plan.edition.id.trim().is_empty() {
        issue(
            &mut report,
            "COMPOSITION_EDITION_MISSING",
            "edition.id must not be empty",
            path,
            None,
        );
    }
    if plan.edition.design_system != design.id {
        issue(
            &mut report,
            "COMPOSITION_DESIGN_SYSTEM_MISMATCH",
            "composition design system does not match the loaded catalog",
            path,
            None,
        );
    }
    if plan.opener.title.trim().is_empty() {
        issue(
            &mut report,
            "OPENER_TITLE_MISSING",
            "opener.title must not be empty",
            path,
            None,
        );
    }
    if plan.opener.art.id.trim().is_empty() || plan.opener.art.role != "primary-subject" {
        issue(
            &mut report,
            "OPENER_ART_INVALID",
            "opener art must name one primary-subject asset",
            path,
            None,
        );
    }

    let flow_spreads = flow
        .spreads
        .iter()
        .map(|spread| (spread.id.as_str(), spread))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    for spread in &plan.spreads {
        if !seen.insert(spread.id.clone()) {
            issue(
                &mut report,
                "COMPOSITION_SPREAD_DUPLICATE",
                "composition spread ID is duplicated",
                path,
                Some(&spread.id),
            );
        }
        let Some(flow_spread) = flow_spreads.get(spread.id.as_str()) else {
            issue(
                &mut report,
                "COMPOSITION_SPREAD_UNKNOWN",
                "composition spread is absent from the flow plan",
                path,
                Some(&spread.id),
            );
            continue;
        };
        let Some(family) = design.families.get(&spread.layout.family) else {
            issue(
                &mut report,
                "LAYOUT_FAMILY_UNKNOWN",
                "layout family is not declared by the design system",
                path,
                Some(&spread.id),
            );
            continue;
        };
        let Some(variant) = family.variants.get(&spread.layout.variant) else {
            issue(
                &mut report,
                "LAYOUT_VARIANT_UNKNOWN",
                "layout variant is not declared by the layout family",
                path,
                Some(&spread.id),
            );
            continue;
        };
        if !family.compatible_roles.is_empty()
            && !family.compatible_roles.contains(&flow_spread.role)
        {
            issue(
                &mut report,
                "LAYOUT_ROLE_INCOMPATIBLE",
                "layout family is incompatible with the flow role",
                path,
                Some(&spread.id),
            );
        }
        if let Some(role) = design.roles.get(&flow_spread.role) {
            if !role.compatible_layout_families.is_empty()
                && !role
                    .compatible_layout_families
                    .contains(&spread.layout.family)
            {
                issue(
                    &mut report,
                    "LAYOUT_ROLE_INCOMPATIBLE",
                    "flow role disallows the chosen layout family",
                    path,
                    Some(&spread.id),
                );
            }
            if !role.text_density.allowed.is_empty()
                && !role.text_density.allowed.contains(&spread.text.density)
            {
                issue(
                    &mut report,
                    "TEXT_DENSITY_INCOMPATIBLE",
                    "text density is incompatible with the flow role",
                    path,
                    Some(&spread.id),
                );
            }
        }
        if !variant.text_density.allowed.is_empty()
            && !variant.text_density.allowed.contains(&spread.text.density)
        {
            issue(
                &mut report,
                "TEXT_DENSITY_INCOMPATIBLE",
                "text density is incompatible with the layout variant",
                path,
                Some(&spread.id),
            );
        }
        if spread.illustration.mode.trim().is_empty()
            || spread.illustration.focal_subject.trim().is_empty()
        {
            issue(
                &mut report,
                "ILLUSTRATION_METADATA_MISSING",
                "illustration mode and focal_subject are required",
                path,
                Some(&spread.id),
            );
        }
        if variant.requires_quiet_region && spread.illustration.quiet_region.is_none() {
            issue(
                &mut report,
                "ILLUSTRATION_METADATA_MISSING",
                "the layout variant requires a quiet_region",
                path,
                Some(&spread.id),
            );
        }
        let mut art_ids = BTreeSet::new();
        for asset in &spread.art_assets {
            if asset.id.trim().is_empty() || !art_ids.insert(&asset.id) {
                issue(
                    &mut report,
                    "ART_ASSET_DUPLICATE",
                    "art asset IDs must be present and unique per spread",
                    path,
                    Some(&spread.id),
                );
            }
            if !matches!(
                asset.role.as_str(),
                "background" | "primary-subject" | "supporting-detail"
            ) {
                issue(
                    &mut report,
                    "ART_ROLE_MISMATCH",
                    "art role must be background, primary-subject, or supporting-detail",
                    path,
                    Some(&spread.id),
                );
            }
        }
    }
    for spread in flow_spreads.keys() {
        if !seen.contains(*spread) {
            issue(
                &mut report,
                "COMPOSITION_SPREAD_UNASSIGNED",
                "flow spread has no composition entry",
                path,
                Some(*spread),
            );
        }
    }
    report
}

pub fn validate_art_usage(
    root: &Path,
    plan: &CompositionPlan,
) -> Result<ValidationReport, AppError> {
    let mut report = ValidationReport::default();
    validate_reference_usage(
        root,
        &plan.opener.art,
        ArtUsage::Opener,
        "OPENER_ART_USAGE_INVALID",
        "opener art must be declared with usage: opener",
        &mut report,
    )?;
    for spread in &plan.spreads {
        for asset in &spread.art_assets {
            validate_reference_usage(
                root,
                asset,
                ArtUsage::Story,
                "OPENER_ART_ON_STORY_SPREAD",
                "opener art may only appear in the composition opener",
                &mut report,
            )?;
        }
    }
    Ok(report)
}

pub fn validate_story_title(story: &Story, plan: &CompositionPlan) -> ValidationReport {
    let mut report = ValidationReport::default();
    if plan.opener.title != story.title {
        issue(
            &mut report,
            "OPENER_TITLE_MISMATCH",
            "opener.title must exactly match the story title",
            &plan.story.flow,
            None,
        );
    }
    report
}

fn validate_reference_usage(
    root: &Path,
    asset: &ArtReference,
    expected: ArtUsage,
    code: &str,
    message: &str,
    report: &mut ValidationReport,
) -> Result<(), AppError> {
    match art_brief::load(root, &asset.id)? {
        Some(brief) if brief.usage == expected => {}
        Some(_) => issue(report, code, message, "art/briefs", Some(&asset.id)),
        None => issue(
            report,
            "COMPOSITION_ART_BRIEF_MISSING",
            "composition art must have a matching art brief",
            "art/briefs",
            Some(&asset.id),
        ),
    }
    Ok(())
}

fn issue(
    report: &mut ValidationReport,
    code: &str,
    message: &str,
    path: &str,
    spread: Option<&str>,
) {
    report.issues.push(ValidationIssue {
        severity: Severity::Error,
        code: code.into(),
        message: message.into(),
        path: path.into(),
        story_id: None,
        unit_id: spread.map(str::to_owned),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{FlowSpread, FlowStory, Narrative, SourceRange, SourceRef};
    use std::fs;

    #[test]
    fn rejects_unknown_layouts_and_missing_flow_spreads() {
        let flow = StoryFlowPlan {
            schema: crate::flow::STORY_FLOW_SCHEMA.into(),
            story: FlowStory {
                id: "story".into(),
                source: None,
                source_revision: "sha256:x".into(),
            },
            spreads: vec![FlowSpread {
                id: "spread-001".into(),
                source: SourceRange {
                    from: SourceRef {
                        kind: "paragraph".into(),
                        id: "one".into(),
                    },
                    through: SourceRef {
                        kind: "paragraph".into(),
                        id: "one".into(),
                    },
                },
                role: "reveal".into(),
                energy: 4,
                narrative: Narrative {
                    purpose: "Reveal".into(),
                    reader_question: None,
                    page_turn_in: None,
                    page_turn_out: None,
                },
                constraints: Default::default(),
            }],
            notes: vec![],
        };
        let plan = CompositionPlan {
            schema: COMPOSITION_PLAN_SCHEMA.into(),
            story: CompositionStory {
                id: "story".into(),
                flow: "story.flow.yaml".into(),
            },
            edition: Edition {
                id: "hardcover".into(),
                design_system: "edgar-v1".into(),
            },
            opener: StoryOpener {
                title: "Story".into(),
                placement: OpenerPlacement::CenterPage,
                art: ArtReference {
                    id: "story-opener".into(),
                    role: "primary-subject".into(),
                },
            },
            spreads: vec![],
        };
        let catalog = DesignCatalog {
            id: "edgar-v1".into(),
            roles: BTreeMap::new(),
            families: BTreeMap::new(),
        };
        assert!(validate(&flow, &plan, &catalog)
            .issues
            .iter()
            .any(|issue| issue.code == "COMPOSITION_SPREAD_UNASSIGNED"));
    }

    #[test]
    fn rejects_opener_art_on_a_narrative_spread() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("art/briefs")).unwrap();
        fs::write(
            directory.path().join("art/briefs/opener.yaml"),
            "schema_version: 2\nart_id: opener\nsource: { story_id: story, anchor_id: story-opener }\nusage: opener\ngeneration: { page_treatment: floating, prompt: Quiet opener. }\n",
        )
        .unwrap();
        let plan = CompositionPlan {
            schema: COMPOSITION_PLAN_SCHEMA.into(),
            story: CompositionStory {
                id: "story".into(),
                flow: "story.flow.yaml".into(),
            },
            edition: Edition {
                id: "hardcover".into(),
                design_system: "edgar-v1".into(),
            },
            opener: StoryOpener {
                title: "Story".into(),
                placement: OpenerPlacement::CenterPage,
                art: ArtReference {
                    id: "opener".into(),
                    role: "primary-subject".into(),
                },
            },
            spreads: vec![CompositionSpread {
                id: "spread-001".into(),
                layout: LayoutChoice {
                    family: "environment-led".into(),
                    variant: "opening-quiet-upper-left".into(),
                },
                text: TextIntent {
                    density: "light".into(),
                },
                illustration: IllustrationIntent {
                    mode: "scene".into(),
                    focal_subject: "Story opening".into(),
                    viewpoint: None,
                    scale: None,
                    quiet_region: Some("upper-left".into()),
                },
                art_assets: vec![ArtReference {
                    id: "opener".into(),
                    role: "primary-subject".into(),
                }],
            }],
        };
        let report = validate_art_usage(directory.path(), &plan).unwrap();
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "OPENER_ART_ON_STORY_SPREAD"));
    }
}
