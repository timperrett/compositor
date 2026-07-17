use crate::art;
use crate::config::Config;
use crate::discovery::discover;
use crate::identity::{resolve_project, ResolvedStory};
use crate::manifest::make_manifest;
use crate::model::{ChangeSet, Manifest, PagePlan, SourceProject, ValidationReport};
use crate::planning::make_plan;
use crate::storage::{self, load_manifest, load_resolutions};
use crate::validation::validate;
use crate::AppError;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct PreparedBuild {
    pub project: SourceProject,
    pub resolved: BTreeMap<String, ResolvedStory>,
    pub changes: ChangeSet,
    pub validation: ValidationReport,
    pub previous: Option<Manifest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildMode {
    Conservative,
    Rebalance,
    Fresh,
}

pub fn prepare(root: &Path, config: &Config) -> Result<PreparedBuild, AppError> {
    let project = discover(root, config)?;
    let mut validation = validate(&project);
    let previous = load_manifest(root, config)?;
    let resolutions = load_resolutions(root, config)?;
    let (resolved, changes) = resolve_project(&project, previous.as_ref(), &resolutions, config);
    validation
        .issues
        .extend(crate::validation::validate_changes(&changes, previous.as_ref()).issues);
    Ok(PreparedBuild {
        project,
        resolved,
        changes,
        validation,
        previous,
    })
}

pub fn build(
    root: &Path,
    config: &Config,
    story_filter: Option<&str>,
) -> Result<(PreparedBuild, Option<Manifest>, Vec<PagePlan>), AppError> {
    build_with_mode(root, config, story_filter, BuildMode::Conservative)
}

pub fn build_with_mode(
    root: &Path,
    config: &Config,
    story_filter: Option<&str>,
    mode: BuildMode,
) -> Result<(PreparedBuild, Option<Manifest>, Vec<PagePlan>), AppError> {
    let prepared = prepare(root, config)?;
    if !prepared.validation.can_proceed() {
        return Err(if prepared.validation.is_blocking() {
            AppError::Blocking("validation contains blocking issues".into())
        } else {
            AppError::Validation
        });
    }
    let source_changed = prepared.previous.is_none() || prepared.changes.has_state_changes();
    let source_affected = affected_stories(&prepared, story_filter)?;
    let stale_plans = stale_plan_stories(root, config, &prepared.project, story_filter)?;
    let affected = source_affected
        .into_iter()
        .chain(stale_plans)
        .collect::<std::collections::BTreeSet<_>>();
    crate::text::write_exports(root, config, &prepared.project)?;
    if !source_changed && affected.is_empty() {
        return Ok((prepared, None, Vec::new()));
    }
    let manifest = if source_changed {
        let manifest = make_manifest(
            &prepared.project,
            &prepared.resolved,
            prepared.previous.as_ref(),
            prepared
                .previous
                .as_ref()
                .map(|manifest| manifest.revision)
                .unwrap_or(0),
        );
        storage::save_manifest(root, config, &manifest)?;
        Some(manifest)
    } else {
        None
    };
    let manifest_revision = manifest
        .as_ref()
        .or(prepared.previous.as_ref())
        .expect("a build with plans always has a manifest")
        .revision;
    let mut plans = Vec::new();
    for compendium in &prepared.project.compendiums {
        for story in &compendium.stories {
            if affected.contains(&story.id) {
                let plan = make_plan(
                    root,
                    config,
                    story,
                    &prepared.resolved[&story.id],
                    manifest_revision,
                    mode == BuildMode::Conservative,
                )?;
                storage::save_plan(root, config, &plan)?;
                art::sync_requirements(root, config, story, &prepared.resolved[&story.id], &plan)?;
                plans.push(plan);
            }
        }
    }
    Ok((prepared, manifest, plans))
}

fn stale_plan_stories(
    root: &Path,
    config: &Config,
    project: &SourceProject,
    filter: Option<&str>,
) -> Result<std::collections::BTreeSet<String>, AppError> {
    let fingerprint = config.pagination_fingerprint();
    project
        .compendiums
        .iter()
        .flat_map(|compendium| compendium.stories.iter())
        .filter(|story| filter.is_none_or(|id| id == story.id))
        .filter_map(
            |story| match storage::load_latest_plan(root, config, &story.id) {
                Ok(Some(plan)) if plan.pagination_fingerprint == fingerprint => None,
                Ok(_) => Some(Ok(story.id.clone())),
                Err(error) => Some(Err(error)),
            },
        )
        .collect()
}

fn affected_stories(
    prepared: &PreparedBuild,
    filter: Option<&str>,
) -> Result<std::collections::BTreeSet<String>, AppError> {
    let available = prepared
        .project
        .compendiums
        .iter()
        .flat_map(|compendium| compendium.stories.iter().map(|story| story.id.clone()))
        .collect::<std::collections::BTreeSet<_>>();
    if let Some(story) = filter {
        if !available.contains(story) {
            return Err(AppError::Command(format!("unknown story `{story}`")));
        }
        return Ok([story.to_owned()].into_iter().collect());
    }
    let changed = prepared
        .changes
        .changes
        .iter()
        .filter(|change| change.kind != crate::model::ChangeKind::Unchanged)
        .map(|change| change.story_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    Ok(if prepared.previous.is_none() {
        available
    } else {
        changed
    })
}
