use crate::identity::ResolvedStory;
use crate::model::{ArtifactStatus, IllustrationRequirement, PagePlan, Story, SCHEMA_VERSION};
use crate::planning::art_needed;
use crate::storage;
use crate::{config::Config, AppError};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Synchronize requirement records and deterministic candidate brief templates
/// from a newly-created plan. Existing brief revisions are never overwritten.
pub fn sync_requirements(
    root: &Path,
    config: &Config,
    story: &Story,
    resolved: &ResolvedStory,
    plan: &PagePlan,
) -> Result<Vec<IllustrationRequirement>, AppError> {
    let mut requirements = Vec::new();
    for (unit, id) in story.units.iter().zip(&resolved.ids) {
        if !art_needed(unit) {
            continue;
        }
        let pages = plan
            .assignments
            .iter()
            .filter(|assignment| assignment.units.iter().any(|unit_id| unit_id == id))
            .flat_map(|assignment| assignment.pages.iter().copied())
            .collect::<Vec<_>>();
        let layout = plan
            .assignments
            .iter()
            .find(|assignment| assignment.units.iter().any(|unit_id| unit_id == id))
            .map(|assignment| assignment.layout.clone())
            .unwrap_or_else(|| "text-dominant".into());
        let previous = storage::load_latest_requirement(root, config, id)?;
        if let Some(previous) = previous.as_ref() {
            if previous.story_id == story.id
                && previous.unit_ids == [id.clone()]
                && previous.pages == pages
                && previous.layout == layout
                && previous.art_note == unit.directives.art
            {
                requirements.push(previous.clone());
                continue;
            }
        }
        let revision = previous
            .as_ref()
            .map(|record| record.revision + 1)
            .unwrap_or(1);
        let record = IllustrationRequirement {
            schema_version: SCHEMA_VERSION,
            art_id: id.clone(),
            story_id: story.id.clone(),
            unit_ids: vec![id.clone()],
            pages,
            layout,
            status: ArtifactStatus::NeedsReview,
            revision,
            art_note: unit.directives.art.clone(),
        };
        storage::save_requirement(root, config, &record)?;
        create_candidate_brief(root, config, &record)?;
        requirements.push(record);
    }
    Ok(requirements)
}

pub fn create_candidate_brief(
    root: &Path,
    config: &Config,
    requirement: &IllustrationRequirement,
) -> Result<(), AppError> {
    let directory = storage::brief_directory(root, config, &requirement.art_id);
    fs::create_dir_all(&directory)?;
    let path = directory.join(format!("v{:03}-candidate.md", requirement.revision));
    if path.exists() {
        return Ok(());
    }
    let note = requirement
        .art_note
        .as_deref()
        .unwrap_or("No authored art note provided.");
    let text = format!(
        "---\nart_id: {}\nstory_id: {}\nrequirement_revision: {}\nstatus: candidate\n---\n\n# Illustration brief: {}\n\n## Authored intent\n\n{}\n\n## Narrative purpose\n\n_TODO_\n\n## Visible action\n\n_TODO_\n\n## Characters and location\n\n_TODO_\n\n## Composition and text-safe region\n\nLayout: `{}` on pages {:?}.\n\n_TODO_\n\n## Continuity and technical requirements\n\n_TODO_\n",
        requirement.art_id,
        requirement.story_id,
        requirement.revision,
        requirement.art_id,
        note,
        requirement.layout,
        requirement.pages,
    );
    fs::write(path, text)?;
    Ok(())
}

pub fn requirements_for_story(
    root: &Path,
    config: &Config,
    story_id: &str,
) -> Result<BTreeMap<String, IllustrationRequirement>, AppError> {
    let base = config.state_dir(root).join("requirements");
    if !base.is_dir() {
        return Ok(BTreeMap::new());
    }
    let mut output = BTreeMap::new();
    for entry in fs::read_dir(base)?.filter_map(Result::ok) {
        let art_id = entry.file_name().to_string_lossy().to_string();
        if let Some(requirement) = storage::load_latest_requirement(root, config, &art_id)? {
            if requirement.story_id == story_id {
                output.insert(art_id, requirement);
            }
        }
    }
    Ok(output)
}
