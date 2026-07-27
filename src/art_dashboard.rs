//! Read-only readiness analysis for the local art review server and `art list`.

use crate::art_brief::{self, ArtBriefInspection};
use crate::assets::{self, AssetRecord, AssetStatus};
use crate::config::Config;
use crate::discovery::discover;
use crate::model::{Severity, ValidationIssue};
use crate::AppError;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Readiness {
    pub label: String,
    pub default_policy_ready: bool,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DashboardCandidate {
    pub id: String,
    pub file: String,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DashboardDiagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DashboardEntry {
    pub art_id: String,
    pub compendium_id: Option<String>,
    pub story_id: Option<String>,
    pub placement: String,
    pub required: bool,
    pub can_unplace: bool,
    pub art_brief: String,
    pub registry_status: Option<AssetStatus>,
    pub candidate_count: usize,
    pub candidates: Vec<DashboardCandidate>,
    pub selected_candidate: Option<String>,
    pub selected_file: Option<String>,
    pub approved_artwork: Option<String>,
    pub readiness: Readiness,
    pub diagnostics: Vec<DashboardDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Dashboard {
    pub entries: Vec<DashboardEntry>,
    pub required_count: usize,
    pub default_policy_ready_count: usize,
    pub blocker_count: usize,
    pub orphan_count: usize,
}

#[derive(Debug, Clone)]
struct EntryLocation {
    required: bool,
    compendium_id: Option<String>,
    story_id: Option<String>,
    placement: Option<crate::art::ArtPlacement>,
}

pub(crate) fn analyze(root: &Path, config: &Config) -> Result<Dashboard, AppError> {
    let project = discover(root, config)?;
    let registry = assets::load(root)?;
    let mut required = BTreeMap::new();
    let mut story_compendiums = BTreeMap::new();
    for story in project
        .compendiums
        .iter()
        .flat_map(|compendium| compendium.stories.iter())
    {
        story_compendiums.insert(story.id.clone(), story.compendium_id.clone());
        for (art_id, requirement) in crate::art::requirements_for_story(root, config, &story.id)? {
            required.insert(
                art_id,
                (
                    requirement.story_id,
                    story.compendium_id.clone(),
                    requirement.placement,
                ),
            );
        }
    }
    let mut ids = required.keys().cloned().collect::<BTreeSet<_>>();
    ids.extend(art_brief::ids(root)?);
    if let Some(registry) = registry.as_ref() {
        ids.extend(registry.assets.iter().map(|asset| asset.id.clone()));
    }
    let mut entries = ids
        .into_iter()
        .map(|art_id| {
            let inspection = art_brief::inspect(root, config, &art_id);
            let asset = registry
                .as_ref()
                .and_then(|registry| assets::record(registry, &art_id));
            let required_story = required.get(&art_id).cloned();
            let story_id = required_story
                .as_ref()
                .map(|location| location.0.clone())
                .or_else(|| {
                    inspection
                        .brief
                        .as_ref()
                        .map(|brief| brief.source.story_id.clone())
                });
            let compendium_id = required_story
                .as_ref()
                .map(|location| location.1.clone())
                .or_else(|| {
                    story_id
                        .as_ref()
                        .and_then(|story_id| story_compendiums.get(story_id).cloned())
                });
            build_entry(
                root,
                config,
                art_id,
                EntryLocation {
                    required: required_story.is_some(),
                    compendium_id,
                    story_id,
                    placement: required_story.map(|location| location.2),
                },
                asset,
                inspection,
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.story_id
            .cmp(&right.story_id)
            .then_with(|| left.art_id.cmp(&right.art_id))
    });
    let required_count = entries.iter().filter(|entry| entry.required).count();
    let default_policy_ready_count = entries
        .iter()
        .filter(|entry| entry.required && entry.readiness.default_policy_ready)
        .count();
    let blocker_count = required_count.saturating_sub(default_policy_ready_count);
    let orphan_count = entries.iter().filter(|entry| !entry.required).count();
    Ok(Dashboard {
        entries,
        required_count,
        default_policy_ready_count,
        blocker_count,
        orphan_count,
    })
}

pub(crate) fn readiness_for(
    root: &Path,
    _config: &Config,
    art_id: &str,
    required: bool,
    asset: Option<&AssetRecord>,
    inspection: &ArtBriefInspection,
) -> Readiness {
    if !required {
        return readiness("unplaced/orphan", true, "No action required".into());
    }
    let candidate_count = inspection
        .brief
        .as_ref()
        .map(|brief| brief.candidates.len())
        .unwrap_or(0);
    if inspection.brief.is_none() {
        return readiness(
            "missing brief",
            false,
            format!("compositor art inspect {art_id}"),
        );
    }
    if !inspection.validation.can_proceed() {
        return readiness(
            "invalid brief",
            false,
            format!("compositor art inspect {art_id}"),
        );
    }
    let Some(asset) = asset else {
        return readiness(
            "needs registration",
            false,
            format!("compositor art register {art_id}"),
        );
    };
    if !assets::validate_record(root, asset).can_proceed() {
        return readiness(
            "invalid record",
            false,
            format!("compositor art inspect {art_id}"),
        );
    }
    match asset.status {
        AssetStatus::Requested if candidate_count == 0 => readiness(
            "no candidate",
            false,
            format!("compositor art inspect {art_id}"),
        ),
        AssetStatus::Requested => readiness(
            "needs selection",
            false,
            candidate_action(art_id, inspection),
        ),
        AssetStatus::Draft => readiness(
            "selected draft",
            true,
            format!("compositor art review {art_id}"),
        ),
        AssetStatus::Review => readiness(
            "in review",
            true,
            format!("compositor art approve {art_id}"),
        ),
        AssetStatus::Approved => readiness("approved", true, "No action required".into()),
        AssetStatus::Rejected => readiness(
            "rejected",
            false,
            format!("compositor art inspect {art_id}"),
        ),
        AssetStatus::Superseded => readiness(
            "superseded",
            false,
            format!("compositor art inspect {art_id}"),
        ),
    }
}

fn readiness(label: &str, default_policy_ready: bool, next_action: String) -> Readiness {
    Readiness {
        label: label.into(),
        default_policy_ready,
        next_action,
    }
}

fn candidate_action(art_id: &str, inspection: &ArtBriefInspection) -> String {
    let candidates = inspection
        .brief
        .as_ref()
        .expect("a valid brief is required for candidate actions");
    if candidates.candidates.len() == 1 {
        format!(
            "compositor art select {art_id} {}",
            candidates.candidates[0].id
        )
    } else {
        format!("compositor art select {art_id} <candidate-id>")
    }
}

fn build_entry(
    root: &Path,
    config: &Config,
    art_id: String,
    location: EntryLocation,
    asset: Option<&AssetRecord>,
    inspection: ArtBriefInspection,
) -> DashboardEntry {
    let readiness = readiness_for(root, config, &art_id, location.required, asset, &inspection);
    let mut diagnostics = inspection
        .validation
        .issues
        .iter()
        .map(diagnostic)
        .collect::<Vec<_>>();
    if let Some(asset) = asset {
        diagnostics.extend(
            assets::validate_record(root, asset)
                .issues
                .iter()
                .map(diagnostic),
        );
    }
    diagnostics.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then_with(|| left.message.cmp(&right.message))
    });
    diagnostics.dedup_by(|left, right| left.code == right.code && left.message == right.message);
    let candidates: Vec<DashboardCandidate> = inspection
        .brief
        .as_ref()
        .map(|brief| {
            brief
                .candidates
                .iter()
                .map(|candidate| DashboardCandidate {
                    id: candidate.id.clone(),
                    file: candidate.file.clone(),
                    selected: asset
                        .and_then(|asset| asset.selection.as_ref())
                        .is_some_and(|selection| selection.candidate_id == candidate.id),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let placement = location
        .placement
        .as_ref()
        .map(crate::art::ArtPlacement::description)
        .unwrap_or_else(|| "unplaced/orphan".into());
    DashboardEntry {
        art_id,
        compendium_id: location.compendium_id,
        story_id: location.story_id,
        placement,
        required: location.required,
        can_unplace: location
            .placement
            .as_ref()
            .is_some_and(crate::art::ArtPlacement::can_unplace),
        art_brief: inspection.path,
        registry_status: asset.map(|asset| asset.status),
        candidate_count: candidates.len(),
        candidates,
        selected_candidate: asset
            .and_then(|asset| asset.selection.as_ref())
            .map(|selection| selection.candidate_id.clone()),
        selected_file: asset
            .and_then(|asset| asset.selection.as_ref())
            .map(|selection| selection.file.clone()),
        approved_artwork: asset
            .and_then(|asset| asset.approved.as_ref())
            .map(|approved| approved.file.clone()),
        readiness,
        diagnostics,
    }
}

fn diagnostic(issue: &ValidationIssue) -> DashboardDiagnostic {
    DashboardDiagnostic {
        severity: issue.severity.clone(),
        code: issue.code.clone(),
        message: issue.message.clone(),
    }
}
