use crate::art_brief::{self, ArtBriefInspection};
use crate::assets::{self, AssetRecord, AssetStatus};
use crate::config::Config;
use crate::discovery::discover;
use crate::model::{Severity, ValidationIssue};
use crate::{storage, AppError};
use askama::Template;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

const DEFAULT_REPORT: &str = "output/reports/art-dashboard.html";

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
pub(crate) struct DashboardEntry {
    pub art_id: String,
    pub compendium_id: Option<String>,
    pub story_id: Option<String>,
    pub placement: String,
    pub required: bool,
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
pub(crate) struct DashboardDiagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
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
}

#[derive(Debug, Serialize)]
pub(crate) struct DashboardOutput {
    output: String,
    artwork_count: usize,
    required_count: usize,
    default_policy_ready_count: usize,
    blocker_count: usize,
    orphan_count: usize,
}

impl DashboardOutput {
    pub(crate) fn from_dashboard(root: &Path, output: &Path, dashboard: &Dashboard) -> Self {
        Self {
            output: relative_to_root(root, output),
            artwork_count: dashboard.entries.len(),
            required_count: dashboard.required_count,
            default_policy_ready_count: dashboard.default_policy_ready_count,
            blocker_count: dashboard.blocker_count,
            orphan_count: dashboard.orphan_count,
        }
    }
}

pub(crate) fn resolve_output(root: &Path, output: &Path) -> Result<PathBuf, AppError> {
    if output.is_absolute()
        || output.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(AppError::command(
            "art dashboard output must be a project-relative path".into(),
        ));
    }
    let output = if output.as_os_str().is_empty() {
        PathBuf::from(DEFAULT_REPORT)
    } else {
        output.to_path_buf()
    };
    if output
        .extension()
        .is_none_or(|extension| extension != "html")
    {
        return Err(AppError::command(
            "art dashboard output must use the .html extension".into(),
        ));
    }
    Ok(root.join(output))
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
            required.insert(art_id, (requirement.story_id, story.compendium_id.clone()));
        }
    }

    let mut ids = required
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
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
    _required: bool,
    asset: Option<&AssetRecord>,
    inspection: &ArtBriefInspection,
) -> Readiness {
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
        .map(|brief| &brief.candidates)
        .expect("readiness only calls candidate_action for a valid brief");
    if candidates.len() == 1 {
        format!("compositor art select {art_id} {}", candidates[0].id)
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
                .collect()
        })
        .unwrap_or_default();
    let placement = match inspection.brief.as_ref() {
        Some(_) if !location.required => "unplaced/orphan".into(),
        Some(brief) if brief.usage == crate::art_brief::ArtUsage::Opener => "opener".into(),
        Some(brief) if !brief.source.spread_ids.is_empty() => {
            format!("spreads: {}", brief.source.spread_ids.join(", "))
        }
        Some(_) => "story art".into(),
        None if location.required => "planned art".into(),
        None => "unplaced/orphan".into(),
    };
    DashboardEntry {
        art_id,
        compendium_id: location.compendium_id,
        story_id: location.story_id,
        placement,
        required: location.required,
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

pub(crate) fn write_html(
    root: &Path,
    output: &Path,
    dashboard: &Dashboard,
) -> Result<(), AppError> {
    let template = DashboardTemplate::from_dashboard(root, output, dashboard);
    let html = template.render().map_err(|error| {
        AppError::serialization(format!("could not render art dashboard: {error}"))
    })?;
    storage::write_text_atomic(output, &html)
}

#[derive(Template)]
#[template(path = "art-dashboard.html")]
struct DashboardTemplate {
    compendiums: Vec<String>,
    stories: Vec<String>,
    readiness_labels: Vec<String>,
    story_groups: Vec<TemplateStoryGroup>,
}

struct TemplateStoryGroup {
    name: String,
    entries: Vec<TemplateEntry>,
}

struct TemplateEntry {
    art_id: String,
    compendium: String,
    story: String,
    placement: String,
    required: bool,
    art_brief: String,
    art_brief_url: String,
    lifecycle: String,
    readiness: String,
    blocker: bool,
    next_action: String,
    candidate_count: usize,
    candidates: Vec<TemplateCandidate>,
    has_candidates: bool,
    selected_file: String,
    has_selected_file: bool,
    approved_artwork: String,
    has_approved_artwork: bool,
    diagnostics: Vec<TemplateDiagnostic>,
    has_diagnostics: bool,
    card_class: String,
}

struct TemplateCandidate {
    id: String,
    file: String,
    url: String,
    selected: bool,
}

struct TemplateDiagnostic {
    severity: String,
    code: String,
    message: String,
}

impl DashboardTemplate {
    fn from_dashboard(root: &Path, output: &Path, dashboard: &Dashboard) -> Self {
        let compendiums = dashboard
            .entries
            .iter()
            .filter_map(|entry| entry.compendium_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let stories = dashboard
            .entries
            .iter()
            .filter_map(|entry| entry.story_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let readiness_labels = dashboard
            .entries
            .iter()
            .map(|entry| entry.readiness.label.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut grouped = BTreeMap::<String, Vec<TemplateEntry>>::new();
        for entry in &dashboard.entries {
            let story = entry
                .story_id
                .clone()
                .unwrap_or_else(|| "Unplaced/orphan".into());
            grouped
                .entry(story.clone())
                .or_default()
                .push(TemplateEntry::from_entry(root, output, entry, story));
        }
        Self {
            compendiums,
            stories,
            readiness_labels,
            story_groups: grouped
                .into_iter()
                .map(|(name, entries)| TemplateStoryGroup { name, entries })
                .collect(),
        }
    }
}

impl TemplateEntry {
    fn from_entry(root: &Path, output: &Path, entry: &DashboardEntry, story: String) -> Self {
        let compendium = entry
            .compendium_id
            .clone()
            .unwrap_or_else(|| "Unplaced/orphan".into());
        let selected_file = entry.selected_file.clone().unwrap_or_default();
        let approved_artwork = entry.approved_artwork.clone().unwrap_or_default();
        let candidates = entry
            .candidates
            .iter()
            .map(|candidate| TemplateCandidate {
                id: candidate.id.clone(),
                file: candidate.file.clone(),
                url: report_link(root, output, &candidate.file),
                selected: candidate.selected,
            })
            .collect::<Vec<_>>();
        let diagnostics = entry
            .diagnostics
            .iter()
            .map(|diagnostic| TemplateDiagnostic {
                severity: format!("{:?}", diagnostic.severity).to_lowercase(),
                code: diagnostic.code.clone(),
                message: diagnostic.message.clone(),
            })
            .collect::<Vec<_>>();
        let card_class = if !entry.required {
            "orphan"
        } else if entry.readiness.default_policy_ready {
            "ready"
        } else {
            "blocked"
        };
        Self {
            art_id: entry.art_id.clone(),
            compendium,
            story,
            placement: entry.placement.clone(),
            required: entry.required,
            art_brief: entry.art_brief.clone(),
            art_brief_url: report_link(root, output, &entry.art_brief),
            lifecycle: entry
                .registry_status
                .map(status_name)
                .unwrap_or("unregistered")
                .into(),
            readiness: entry.readiness.label.clone(),
            blocker: entry.required && !entry.readiness.default_policy_ready,
            next_action: entry.readiness.next_action.clone(),
            candidate_count: entry.candidate_count,
            has_candidates: !candidates.is_empty(),
            candidates,
            has_selected_file: !selected_file.is_empty(),
            selected_file,
            has_approved_artwork: !approved_artwork.is_empty(),
            approved_artwork,
            has_diagnostics: !diagnostics.is_empty(),
            diagnostics,
            card_class: card_class.into(),
        }
    }
}

fn report_link(root: &Path, output: &Path, file: &str) -> String {
    let from = output.parent().unwrap_or(root);
    relative_between(from, &root.join(file))
}

fn relative_between(from: &Path, to: &Path) -> String {
    let from = normal_components(from);
    let to = normal_components(to);
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts = vec![".."; from.len().saturating_sub(common)];
    parts.extend(to[common..].iter().map(String::as_str));
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}

fn normal_components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect()
}

fn relative_to_root(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn status_name(status: AssetStatus) -> &'static str {
    match status {
        AssetStatus::Requested => "requested",
        AssetStatus::Draft => "draft",
        AssetStatus::Review => "review",
        AssetStatus::Approved => "approved",
        AssetStatus::Rejected => "rejected",
        AssetStatus::Superseded => "superseded",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_escapes_authored_html_text() {
        let template = DashboardTemplate {
            compendiums: vec!["<compendium>".into()],
            stories: vec!["<story>".into()],
            readiness_labels: vec!["needs <selection>".into()],
            story_groups: vec![TemplateStoryGroup {
                name: "<story>".into(),
                entries: vec![TemplateEntry {
                    art_id: "<art>".into(),
                    compendium: "<compendium>".into(),
                    story: "<story>".into(),
                    placement: "<placement>".into(),
                    required: true,
                    art_brief: "<brief>".into(),
                    art_brief_url: "<url>".into(),
                    lifecycle: "requested".into(),
                    readiness: "needs <selection>".into(),
                    blocker: true,
                    next_action: "<command>".into(),
                    candidate_count: 0,
                    candidates: vec![],
                    has_candidates: false,
                    selected_file: String::new(),
                    has_selected_file: false,
                    approved_artwork: String::new(),
                    has_approved_artwork: false,
                    diagnostics: vec![],
                    has_diagnostics: false,
                    card_class: "blocked".into(),
                }],
            }],
        };
        let rendered = template.render().unwrap();
        assert!(rendered.contains("&#60;art&#62;"));
        assert!(!rendered.contains("><art>"));
    }

    #[test]
    fn computes_links_relative_to_the_report_location() {
        assert_eq!(
            relative_between(
                Path::new("/project/output/reports"),
                Path::new("/project/assets/drafts/art/a.png")
            ),
            "../../assets/drafts/art/a.png"
        );
    }
}
