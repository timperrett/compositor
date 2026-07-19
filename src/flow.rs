use crate::markdown::valid_anchor;
use crate::model::{Severity, Story, ValidationIssue, ValidationReport};
use crate::AppError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub const STORY_FLOW_SCHEMA: &str = "compositor.dev/story-flow/v1";
const DESIGN_SYSTEM_SCHEMA: &str = "compositor.dev/design-system/v1";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StoryFlowPlan {
    pub schema: String,
    pub story: FlowStory,
    pub spreads: Vec<FlowSpread>,
    #[serde(default)]
    pub notes: Vec<FlowNote>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FlowStory {
    pub id: String,
    #[serde(default)]
    pub source: Option<String>,
    pub source_revision: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FlowSpread {
    pub id: String,
    pub source: SourceRange,
    pub role: String,
    pub energy: u8,
    pub narrative: Narrative,
    #[serde(default)]
    pub constraints: FlowConstraints,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceRange {
    pub from: SourceRef,
    pub through: SourceRef,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceRef {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Narrative {
    pub purpose: String,
    #[serde(default)]
    pub reader_question: Option<String>,
    #[serde(default)]
    pub page_turn_in: Option<String>,
    #[serde(default)]
    pub page_turn_out: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FlowConstraints {
    #[serde(default)]
    pub max_words: Option<usize>,
    #[serde(default)]
    pub must_keep_together: Vec<SourceRef>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FlowNote {
    pub code: String,
    pub severity: String,
    pub spread: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct DesignSystemDescriptor {
    schema: String,
    id: String,
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    version: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RolesFile {
    roles: BTreeMap<String, RoleRule>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RoleRule {
    energy: EnergyRange,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnergyRange {
    min: u8,
    max: u8,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ValidationRulesFile {
    #[serde(default)]
    page_turns: Vec<String>,
    #[serde(default)]
    pacing: PacingRules,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct PacingRules {
    high_energy_threshold: u8,
    max_consecutive_high_energy: usize,
}

impl Default for PacingRules {
    fn default() -> Self {
        Self {
            high_energy_threshold: 4,
            max_consecutive_high_energy: 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DesignSystem {
    pub id: String,
    roles: BTreeMap<String, RoleRule>,
    page_turns: BTreeSet<String>,
    pacing: PacingRules,
}

pub fn load_plan(path: &Path) -> Result<StoryFlowPlan, AppError> {
    let text = fs::read_to_string(path)?;
    serde_yaml::from_str(&text)
        .map_err(|error| AppError::Serialization(format!("{}: {error}", path.display())))
}

pub fn load_story(path: &Path) -> Result<Story, AppError> {
    let parsed = crate::markdown::parse_document(&fs::read_to_string(path)?)?;
    let id = metadata_string(&parsed.metadata, "id", path)?;
    let title = metadata_string(&parsed.metadata, "title", path)?;
    Ok(Story {
        id,
        title,
        source: path.to_string_lossy().replace('\\', "/"),
        ordinal: 1,
        compendium_id: String::new(),
        source_hash: parsed.source_hash,
        metadata: parsed.metadata,
        units: parsed.units,
        paragraphs: parsed.paragraphs,
        paragraph_comments: parsed.paragraph_comments,
    })
}

fn metadata_string(
    metadata: &BTreeMap<String, serde_yaml::Value>,
    key: &str,
    path: &Path,
) -> Result<String, AppError> {
    metadata
        .get(key)
        .and_then(serde_yaml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| AppError::Config(format!("missing `{key}` in {}", path.display())))
}

pub fn load_design_system(directory: &Path) -> Result<DesignSystem, AppError> {
    let descriptor: DesignSystemDescriptor = read_yaml(&directory.join("design-system.yaml"))?;
    if descriptor.schema != DESIGN_SYSTEM_SCHEMA {
        return Err(AppError::Config(format!(
            "unsupported design system schema `{}`",
            descriptor.schema
        )));
    }
    let roles: RolesFile = read_yaml(&directory.join("spread-roles.yaml"))?;
    let rules: ValidationRulesFile = read_yaml(&directory.join("validation-rules.yaml"))?;
    Ok(DesignSystem {
        id: descriptor.id,
        roles: roles.roles,
        page_turns: rules.page_turns.into_iter().collect(),
        pacing: rules.pacing,
    })
}

fn read_yaml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, AppError> {
    let text = fs::read_to_string(path)?;
    serde_yaml::from_str(&text)
        .map_err(|error| AppError::Serialization(format!("{}: {error}", path.display())))
}

pub fn validate(story: &Story, plan: &StoryFlowPlan, design: &DesignSystem) -> ValidationReport {
    let mut report = source_report(story);
    let path = plan.story.source.as_deref().unwrap_or(&story.source);
    if plan.schema != STORY_FLOW_SCHEMA {
        issue(
            &mut report,
            Severity::Error,
            "FLOW_SCHEMA_UNSUPPORTED",
            format!("expected schema `{STORY_FLOW_SCHEMA}`"),
            path,
            story,
            None,
        );
    }
    if plan.story.id != story.id {
        issue(
            &mut report,
            Severity::Error,
            "FLOW_STORY_MISMATCH",
            format!(
                "plan targets `{}`, source story is `{}`",
                plan.story.id, story.id
            ),
            path,
            story,
            None,
        );
    }
    if plan.story.source_revision != story.source_hash {
        issue(
            &mut report,
            Severity::Error,
            "FLOW_SOURCE_STALE",
            "plan source_revision does not match the current story".into(),
            path,
            story,
            None,
        );
    }

    let ids = story
        .paragraphs
        .iter()
        .filter_map(|paragraph| paragraph.id.as_ref().map(|id| (id.as_str(), paragraph)))
        .collect::<BTreeMap<_, _>>();
    let mut assigned = BTreeMap::<usize, String>::new();
    let mut previous_out = None;
    let mut high_run = 0usize;
    let mut energies = Vec::new();

    for (index, spread) in plan.spreads.iter().enumerate() {
        let expected = format!("spread-{:03}", index + 1);
        if spread.id != expected {
            issue(
                &mut report,
                Severity::Error,
                "FLOW_SPREAD_ID_INVALID",
                format!("expected sequential spread id `{expected}`"),
                path,
                story,
                Some(&spread.id),
            );
        }
        let Some(role) = design.roles.get(&spread.role) else {
            issue(
                &mut report,
                Severity::Error,
                "FLOW_ROLE_UNKNOWN",
                format!("unknown role `{}`", spread.role),
                path,
                story,
                Some(&spread.id),
            );
            continue;
        };
        if !(role.energy.min..=role.energy.max).contains(&spread.energy) {
            issue(
                &mut report,
                Severity::Error,
                "FLOW_ENERGY_INVALID",
                format!(
                    "role `{}` permits energy {} through {}",
                    spread.role, role.energy.min, role.energy.max
                ),
                path,
                story,
                Some(&spread.id),
            );
        }
        energies.push(spread.energy);
        high_run = if spread.energy >= design.pacing.high_energy_threshold {
            high_run + 1
        } else {
            0
        };
        if high_run > design.pacing.max_consecutive_high_energy {
            issue(
                &mut report,
                Severity::Warning,
                "ENERGY_CLUSTER",
                "too many consecutive high-energy spreads".into(),
                path,
                story,
                Some(&spread.id),
            );
        }
        if spread.role == "reveal" && index == 0 {
            issue(
                &mut report,
                Severity::Warning,
                "FLOW_REVEAL_WITHOUT_SETUP",
                "a reveal spread needs an earlier setup spread".into(),
                path,
                story,
                Some(&spread.id),
            );
        }
        validate_turn(
            &mut report,
            design,
            spread.narrative.page_turn_in.as_deref(),
            path,
            story,
            &spread.id,
        );
        validate_turn(
            &mut report,
            design,
            spread.narrative.page_turn_out.as_deref(),
            path,
            story,
            &spread.id,
        );
        if let (Some(previous), Some(current)) = (
            previous_out.as_deref(),
            spread.narrative.page_turn_in.as_deref(),
        ) {
            if previous != current {
                issue(
                    &mut report,
                    Severity::Warning,
                    "FLOW_PAGE_TURN_INVALID",
                    format!(
                        "page turn in `{current}` does not match preceding turn out `{previous}`"
                    ),
                    path,
                    story,
                    Some(&spread.id),
                );
            }
        }
        previous_out = spread.narrative.page_turn_out.clone();

        let from = resolve_ref(
            &mut report,
            &spread.source.from,
            &ids,
            path,
            story,
            &spread.id,
        );
        let through = resolve_ref(
            &mut report,
            &spread.source.through,
            &ids,
            path,
            story,
            &spread.id,
        );
        let (Some(from), Some(through)) = (from, through) else {
            continue;
        };
        if from.ordinal > through.ordinal {
            issue(
                &mut report,
                Severity::Error,
                "SOURCE_RANGE_INVALID",
                "source range runs backward".into(),
                path,
                story,
                Some(&spread.id),
            );
            continue;
        }
        let word_count = story.paragraphs[from.ordinal - 1..through.ordinal]
            .iter()
            .map(|paragraph| paragraph.word_count)
            .sum::<usize>();
        if spread
            .constraints
            .max_words
            .is_some_and(|maximum| word_count > maximum)
        {
            issue(
                &mut report,
                Severity::Warning,
                "FLOW_WORD_COUNT_HIGH",
                format!("spread contains {word_count} words"),
                path,
                story,
                Some(&spread.id),
            );
        }
        for reference in &spread.constraints.must_keep_together {
            if let Some(paragraph) =
                resolve_ref(&mut report, reference, &ids, path, story, &spread.id)
            {
                if !(from.ordinal..=through.ordinal).contains(&paragraph.ordinal) {
                    issue(
                        &mut report,
                        Severity::Error,
                        "FLOW_KEEP_TOGETHER_INVALID",
                        format!("`{}` is outside this spread's source range", reference.id),
                        path,
                        story,
                        Some(&spread.id),
                    );
                }
            }
        }
        for paragraph in &story.paragraphs[from.ordinal - 1..through.ordinal] {
            if let Some(previous) = assigned.insert(paragraph.ordinal, spread.id.clone()) {
                issue(
                    &mut report,
                    Severity::Error,
                    "SOURCE_BLOCK_ASSIGNED_MULTIPLE",
                    format!(
                        "paragraph `{}` is assigned by both `{previous}` and `{}`",
                        paragraph.id.as_deref().unwrap_or("<unidentified>"),
                        spread.id
                    ),
                    path,
                    story,
                    Some(&spread.id),
                );
            }
        }
    }
    for paragraph in &story.paragraphs {
        if !assigned.contains_key(&paragraph.ordinal) {
            issue(
                &mut report,
                Severity::Error,
                "SOURCE_BLOCK_UNASSIGNED",
                format!(
                    "paragraph {} is not assigned to a spread",
                    paragraph.ordinal
                ),
                path,
                story,
                paragraph.id.as_deref(),
            );
        }
    }
    if energies.len() > 1 && energies.iter().all(|energy| *energy == energies[0]) {
        issue(
            &mut report,
            Severity::Warning,
            "ENERGY_FLAT",
            "all spread energy values are the same".into(),
            path,
            story,
            None,
        );
    }
    report
}

pub fn source_report(story: &Story) -> ValidationReport {
    let mut report = ValidationReport::default();
    let mut seen = BTreeSet::new();
    for comment in &story.paragraph_comments {
        if !valid_anchor(&comment.raw_id) {
            issue(
                &mut report,
                Severity::Error,
                "SOURCE_ID_MALFORMED",
                format!("invalid paragraph identifier `{}`", comment.raw_id),
                &story.source,
                story,
                None,
            );
        } else if !seen.insert(comment.raw_id.clone()) {
            issue(
                &mut report,
                Severity::Error,
                "SOURCE_ID_DUPLICATE",
                format!("duplicate paragraph identifier `{}`", comment.raw_id),
                &story.source,
                story,
                Some(&comment.raw_id),
            );
        }
        if comment.paragraph_ordinal.is_none() {
            issue(
                &mut report,
                Severity::Error,
                "SOURCE_ID_ORPHANED",
                format!(
                    "paragraph identifier `{}` has no following prose paragraph",
                    comment.raw_id
                ),
                &story.source,
                story,
                Some(&comment.raw_id),
            );
        }
        if positional(&comment.raw_id) {
            issue(
                &mut report,
                Severity::Warning,
                "SOURCE_ID_POSITIONAL",
                format!(
                    "paragraph identifier `{}` appears positional",
                    comment.raw_id
                ),
                &story.source,
                story,
                Some(&comment.raw_id),
            );
        }
    }
    for paragraph in &story.paragraphs {
        if paragraph.id.is_none() {
            issue(
                &mut report,
                Severity::Error,
                "SOURCE_BLOCK_UNASSIGNED",
                format!(
                    "prose paragraph {} has no durable identifier",
                    paragraph.ordinal
                ),
                &story.source,
                story,
                None,
            );
        }
    }
    report
}

fn positional(id: &str) -> bool {
    ["paragraph", "page", "section", "part", "version"]
        .iter()
        .any(|prefix| {
            id.strip_prefix(prefix)
                .and_then(|suffix| suffix.strip_prefix('-').or(Some(suffix)))
                .is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
                })
        })
}

fn resolve_ref<'a>(
    report: &mut ValidationReport,
    reference: &SourceRef,
    ids: &BTreeMap<&str, &'a crate::model::SourceParagraph>,
    path: &str,
    story: &Story,
    spread: &str,
) -> Option<&'a crate::model::SourceParagraph> {
    if reference.kind != "paragraph" {
        issue(
            report,
            Severity::Error,
            "SOURCE_REFERENCE_UNKNOWN",
            format!("unsupported source type `{}`", reference.kind),
            path,
            story,
            Some(spread),
        );
        return None;
    }
    ids.get(reference.id.as_str()).copied().or_else(|| {
        issue(
            report,
            Severity::Error,
            "SOURCE_REFERENCE_UNKNOWN",
            format!("unknown paragraph `{}`", reference.id),
            path,
            story,
            Some(spread),
        );
        None
    })
}

fn validate_turn(
    report: &mut ValidationReport,
    design: &DesignSystem,
    turn: Option<&str>,
    path: &str,
    story: &Story,
    spread: &str,
) {
    if let Some(turn) = turn {
        if !design.page_turns.contains(turn) {
            issue(
                report,
                Severity::Error,
                "FLOW_PAGE_TURN_INVALID",
                format!("unknown page-turn intent `{turn}`"),
                path,
                story,
                Some(spread),
            );
        }
    }
}

fn issue(
    report: &mut ValidationReport,
    severity: Severity,
    code: &str,
    message: String,
    path: &str,
    story: &Story,
    unit_id: Option<&str>,
) {
    report.issues.push(ValidationIssue {
        severity,
        code: code.into(),
        message,
        path: path.into(),
        story_id: Some(story.id.clone()),
        unit_id: unit_id.map(str::to_owned),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::parse_document;
    use crate::model::Story;

    fn story() -> Story {
        let parsed = parse_document(
            "---\nid: map\ntitle: Map\n---\n<!-- paragraph: opening-rain -->\n\nRain whispered.\n\n<!-- paragraph: edgar-finds-box -->\n\nEdgar found the box.\n\n<!-- paragraph: map-revealed -->\n\nThe map shone.",
        )
        .unwrap();
        Story {
            id: "map".into(),
            title: "Map".into(),
            source: "compendiums/magic/01-map/story.md".into(),
            ordinal: 1,
            compendium_id: "magic".into(),
            source_hash: parsed.source_hash,
            metadata: parsed.metadata,
            units: parsed.units,
            paragraphs: parsed.paragraphs,
            paragraph_comments: parsed.paragraph_comments,
        }
    }

    fn design() -> DesignSystem {
        DesignSystem {
            id: "edgar-v1".into(),
            roles: BTreeMap::from([
                (
                    "opening-wonder".into(),
                    RoleRule {
                        energy: EnergyRange { min: 1, max: 3 },
                    },
                ),
                (
                    "discovery".into(),
                    RoleRule {
                        energy: EnergyRange { min: 2, max: 4 },
                    },
                ),
                (
                    "reveal".into(),
                    RoleRule {
                        energy: EnergyRange { min: 4, max: 5 },
                    },
                ),
            ]),
            page_turns: ["discovery".into(), "reveal".into()].into_iter().collect(),
            pacing: PacingRules::default(),
        }
    }

    fn reference(id: &str) -> SourceRef {
        SourceRef {
            kind: "paragraph".into(),
            id: id.into(),
        }
    }

    fn spread(id: &str, from: &str, through: &str, role: &str, energy: u8) -> FlowSpread {
        FlowSpread {
            id: id.into(),
            source: SourceRange {
                from: reference(from),
                through: reference(through),
            },
            role: role.into(),
            energy,
            narrative: Narrative {
                purpose: "Test the story flow.".into(),
                reader_question: None,
                page_turn_in: None,
                page_turn_out: None,
            },
            constraints: FlowConstraints::default(),
        }
    }

    #[test]
    fn validates_complete_ordered_flow_coverage() {
        let story = story();
        let plan = StoryFlowPlan {
            schema: STORY_FLOW_SCHEMA.into(),
            story: FlowStory {
                id: story.id.clone(),
                source: Some(story.source.clone()),
                source_revision: story.source_hash.clone(),
            },
            spreads: vec![
                spread(
                    "spread-001",
                    "opening-rain",
                    "edgar-finds-box",
                    "opening-wonder",
                    2,
                ),
                spread("spread-002", "map-revealed", "map-revealed", "reveal", 4),
            ],
            notes: Vec::new(),
        };
        assert!(validate(&story, &plan, &design()).can_proceed());
    }

    #[test]
    fn reports_duplicate_assignment_and_unknown_references() {
        let story = story();
        let plan = StoryFlowPlan {
            schema: STORY_FLOW_SCHEMA.into(),
            story: FlowStory {
                id: story.id.clone(),
                source: None,
                source_revision: story.source_hash.clone(),
            },
            spreads: vec![
                spread(
                    "spread-001",
                    "opening-rain",
                    "edgar-finds-box",
                    "opening-wonder",
                    2,
                ),
                spread("spread-002", "edgar-finds-box", "missing", "discovery", 3),
            ],
            notes: Vec::new(),
        };
        let report = validate(&story, &plan, &design());
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "SOURCE_REFERENCE_UNKNOWN"));
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "SOURCE_BLOCK_UNASSIGNED"));
    }
}
