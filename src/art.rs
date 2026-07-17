use crate::identity::ResolvedStory;
use crate::model::{ArtifactStatus, IllustrationRequirement, PagePlan, Story, SCHEMA_VERSION};
use crate::planning::art_needed;
use crate::storage;
use crate::{config::Config, AppError};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Synchronize illustration requirements from a newly-created plan. Art briefs
/// are external, skill-authored protocol records in `art/briefs/`.
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
        requirements.push(record);
    }
    Ok(requirements)
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
