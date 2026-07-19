use crate::config::Config;
use crate::model::{
    ChangeKind, ChangeSet, Manifest, Severity, SourceProject, UnitType, ValidationIssue,
    ValidationReport,
};
use crate::storage;
use std::collections::BTreeSet;
use std::path::Path;

pub fn validate(project: &SourceProject) -> ValidationReport {
    let mut report = ValidationReport::default();
    let mut story_ids = BTreeSet::new();
    let mut anchors = BTreeSet::new();
    for compendium in &project.compendiums {
        for story in &compendium.stories {
            if !story_ids.insert(story.id.clone()) {
                issue(
                    &mut report,
                    Severity::Error,
                    "duplicate_story_id",
                    format!("duplicate story ID `{}`", story.id),
                    &story.source,
                    Some(&story.id),
                    None,
                );
            }
            for unit in &story.units {
                let is_blank = unit.directives.unit_type == Some(UnitType::Blank);
                if unit.normalized_content.is_empty() && !is_blank {
                    issue(
                        &mut report,
                        Severity::Error,
                        "empty_unit",
                        "empty content unit; use `<!-- unit: blank -->` for an intentional blank"
                            .into(),
                        &story.source,
                        Some(&story.id),
                        None,
                    );
                }
                if let Some(anchor) = &unit.directives.anchor {
                    if !anchors.insert(anchor.clone()) {
                        issue(
                            &mut report,
                            Severity::Error,
                            "duplicate_anchor",
                            format!("duplicate anchor `{anchor}`"),
                            &story.source,
                            Some(&story.id),
                            Some(anchor),
                        );
                    }
                }
            }
        }
    }
    report
}

pub fn validate_changes(changes: &ChangeSet, previous: Option<&Manifest>) -> ValidationReport {
    let mut report = ValidationReport::default();
    for change in &changes.changes {
        if change.kind == ChangeKind::Ambiguous {
            issue(
                &mut report,
                Severity::Warning,
                "ambiguous_identity",
                change.message.clone(),
                "manifest",
                Some(&change.story_id),
                change.unit_id.as_deref(),
            );
        }
        if change.kind == ChangeKind::Merged {
            let has_production_relationship = previous
                .and_then(|manifest| manifest.stories.get(&change.story_id))
                .is_some_and(|story| {
                    story
                        .units
                        .iter()
                        .any(|unit| unit.art_brief.is_some() || unit.approved_art.is_some())
                });
            issue(
                &mut report,
                if has_production_relationship {
                    Severity::Blocking
                } else {
                    Severity::Warning
                },
                "unresolved_merge",
                change.message.clone(),
                "manifest",
                Some(&change.story_id),
                None,
            );
        }
    }
    report
}

pub fn validate_state(
    root: &Path,
    config: &Config,
    project: &SourceProject,
    manifest: Option<&Manifest>,
) -> ValidationReport {
    let mut report = ValidationReport::default();
    let Some(manifest) = manifest else {
        issue(
            &mut report,
            Severity::Info,
            "missing_manifest",
            "no manifest exists; run build to create production state".into(),
            ".compositor/manifest.json",
            None,
            None,
        );
        return report;
    };
    let mut linked_assets = BTreeSet::new();
    for compendium in &project.compendiums {
        for story in &compendium.stories {
            let Some(manifest_story) = manifest.stories.get(&story.id) else {
                continue;
            };
            match storage::load_latest_plan(root, config, &story.id) {
                Ok(None) => issue(
                    &mut report,
                    Severity::Error,
                    "missing_page_assignment",
                    "story has no page plan".into(),
                    &story.source,
                    Some(&story.id),
                    None,
                ),
                Ok(Some(plan)) => {
                    for unit_id in plan
                        .assignments
                        .iter()
                        .flat_map(|assignment| &assignment.units)
                    {
                        if !manifest_story.units.iter().any(|unit| &unit.id == unit_id) {
                            issue(
                                &mut report,
                                Severity::Error,
                                "plan_missing_unit",
                                format!("page plan references missing unit `{unit_id}`"),
                                &story.source,
                                Some(&story.id),
                                Some(unit_id),
                            );
                        }
                    }
                }
                Err(error) => issue(
                    &mut report,
                    Severity::Error,
                    "plan_unreadable",
                    error.to_string(),
                    &story.source,
                    Some(&story.id),
                    None,
                ),
            }
            for unit in &manifest_story.units {
                if let Some(asset) = &unit.approved_art {
                    linked_assets.insert(asset.clone());
                    if unit.anchor.is_none() {
                        issue(
                            &mut report,
                            Severity::Blocking,
                            "approved_art_unanchored",
                            "approved artwork is attached to a provisional unit".into(),
                            &story.source,
                            Some(&story.id),
                            Some(&unit.id),
                        );
                    }
                    if !root.join(asset).is_file() {
                        issue(
                            &mut report,
                            Severity::Error,
                            "missing_approved_artwork",
                            format!("approved artwork is missing: {asset}"),
                            &story.source,
                            Some(&story.id),
                            Some(&unit.id),
                        );
                    }
                }
            }
        }
    }
    let approved = root.join(&config.assets.approved_directory);
    if let Ok(entries) = std::fs::read_dir(&approved) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if !linked_assets.contains(&relative) {
                    issue(
                        &mut report,
                        Severity::Warning,
                        "orphaned_approved_asset",
                        format!("approved asset is not linked to a unit: {relative}"),
                        &relative,
                        None,
                        None,
                    );
                }
            }
        }
    }
    report
}

fn issue(
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
