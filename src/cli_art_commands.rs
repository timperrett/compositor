use super::*;

#[derive(Debug, Serialize)]
struct ArtListItem {
    art_id: String,
    story_id: String,
    pages: Vec<u32>,
    layout: String,
    art_layout: Option<crate::model::ArtLayout>,
    geometry: Option<crate::model::ArtGeometry>,
    requirement_revision: u64,
    requirement_status: crate::model::ArtifactStatus,
    art_brief: Option<String>,
    art_brief_valid: bool,
    candidate_count: usize,
    selected_candidate: Option<String>,
    approved_artwork: Option<String>,
}

pub(super) fn art_command(
    root: &std::path::Path,
    config: &Config,
    format: OutputFormat,
    command: ArtCommand,
) -> Result<(), AppError> {
    match command {
        ArtCommand::List { story } => {
            let project = discover(root, config)?;
            if let Some(story_id) = story.as_deref() {
                let exists = project
                    .compendiums
                    .iter()
                    .flat_map(|compendium| &compendium.stories)
                    .any(|candidate| candidate.id == story_id);
                if !exists {
                    return Err(AppError::Command(format!("unknown story `{story_id}`")));
                }
            }
            let manifest = storage::load_manifest(root, config)?;
            let mut records = Vec::new();
            for compendium in &project.compendiums {
                for source_story in &compendium.stories {
                    if story.as_deref().is_some_and(|id| id != source_story.id) {
                        continue;
                    }
                    for (art_id, requirement) in
                        crate::art::requirements_for_story(root, config, &source_story.id)?
                    {
                        let brief = art_brief::inspect(root, config, &art_id);
                        let unit = manifest.as_ref().and_then(|manifest| {
                            manifest
                                .stories
                                .get(&source_story.id)
                                .and_then(|story| story.units.iter().find(|unit| unit.id == art_id))
                        });
                        records.push(ArtListItem {
                            art_id,
                            story_id: requirement.story_id,
                            pages: requirement.pages,
                            layout: requirement.layout.to_string(),
                            art_layout: requirement.art_layout.clone(),
                            geometry: requirement.geometry.clone(),
                            requirement_revision: requirement.revision,
                            requirement_status: requirement.status,
                            art_brief: brief.brief.as_ref().map(|_| brief.path.clone()),
                            art_brief_valid: brief.brief.is_some()
                                && brief.validation.can_proceed(),
                            candidate_count: brief
                                .brief
                                .as_ref()
                                .map(|brief| brief.candidates.len())
                                .unwrap_or(0),
                            selected_candidate: brief.brief.as_ref().and_then(|brief| {
                                brief
                                    .selection
                                    .as_ref()
                                    .map(|selection| selection.candidate_id.clone())
                            }),
                            approved_artwork: unit.and_then(|unit| unit.approved_art.clone()),
                        });
                    }
                }
            }
            print_report(format, "art list", records, ValidationReport::default())
        }
        ArtCommand::Inspect { art_id } => inspect_art(root, config, format, &art_id),
        ArtCommand::Brief { art_id } => {
            required_art_requirement(root, config, &art_id)?;
            let output = art_brief::inspect(root, config, &art_id);
            print_report(format, "art brief", output, ValidationReport::default())
        }
        ArtCommand::Validate { story, strict } => {
            let project = discover(root, config)?;
            let mut ids = std::collections::BTreeSet::new();
            for compendium in &project.compendiums {
                for source_story in &compendium.stories {
                    if story.as_deref().is_none_or(|id| id == source_story.id) {
                        ids.extend(
                            crate::art::requirements_for_story(root, config, &source_story.id)?
                                .into_keys(),
                        );
                    }
                }
            }
            for art_id in art_brief::ids(root)? {
                let inspection = art_brief::inspect(root, config, &art_id);
                if story.as_deref().is_none_or(|story_id| {
                    inspection
                        .brief
                        .as_ref()
                        .is_some_and(|brief| brief.source.story_id == story_id)
                }) {
                    ids.insert(art_id);
                }
            }
            let records = ids
                .into_iter()
                .map(|art_id| art_brief::inspect(root, config, &art_id))
                .collect::<Vec<_>>();
            let validation = ValidationReport {
                issues: records
                    .iter()
                    .flat_map(|record| record.validation.issues.clone())
                    .collect(),
            };
            print_report(format, "art validate", records, validation.clone())?;
            if validation.is_blocking() {
                Err(AppError::Blocking(
                    "art validation contains blocking issues".into(),
                ))
            } else if strict && !validation.issues.is_empty() || !validation.can_proceed() {
                Err(AppError::Validation)
            } else {
                Ok(())
            }
        }
        ArtCommand::Attach {
            art_id,
            path,
            selected,
        } => {
            required_art_requirement(root, config, &art_id)?;
            if selected == path.is_some() {
                return Err(AppError::Command(
                    "provide an approved asset path or use --selected".into(),
                ));
            }
            let (relative, brief) = if selected {
                let inspection = art_brief::inspect(root, config, &art_id);
                if !inspection.validation.can_proceed() {
                    return Err(AppError::Validation);
                }
                let brief = inspection.brief.ok_or_else(|| {
                    AppError::Command(format!("no art brief exists for `{art_id}`"))
                })?;
                let selected = brief.selection.as_ref().ok_or_else(|| {
                    AppError::Command(format!("art brief `{art_id}` has no selected candidate"))
                })?;
                let candidate = brief
                    .candidates
                    .iter()
                    .find(|candidate| candidate.id == selected.candidate_id)
                    .ok_or_else(|| {
                        AppError::Command(format!(
                            "selected candidate `{}` does not exist",
                            selected.candidate_id
                        ))
                    })?;
                let source = root.join(&candidate.file);
                let extension = source
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .unwrap_or("png");
                let target = root
                    .join(&config.assets.approved_directory)
                    .join(format!("{art_id}-{}.{}", candidate.id, extension));
                if target.exists() {
                    return Err(AppError::Command(format!(
                        "approved artwork already exists: {}",
                        target.display()
                    )));
                }
                let parent = target.parent().ok_or_else(|| {
                    AppError::command("approved artwork path has no parent directory".into())
                })?;
                fs::create_dir_all(parent)?;
                fs::copy(source, &target)?;
                (relative_path(root, &target), Some(inspection.path))
            } else {
                (
                    validate_approved_asset(
                        root,
                        config,
                        path.as_ref().ok_or_else(|| {
                            AppError::command("approved artwork path is required".into())
                        })?,
                    )?,
                    None,
                )
            };
            set_art_relationship(root, config, &art_id, brief, Some(relative.clone()))?;
            print_report(format, "art attach", relative, ValidationReport::default())
        }
    }
}

pub(super) fn required_art_requirement(
    root: &std::path::Path,
    config: &Config,
    art_id: &str,
) -> Result<crate::model::IllustrationRequirement, AppError> {
    storage::load_latest_requirement(root, config, art_id)?.ok_or_else(|| {
        AppError::Command(format!(
            "unknown art `{art_id}`; run build to generate its requirement"
        ))
    })
}

#[derive(Debug, Serialize)]
pub(super) struct StatusOutput {
    changes: ChangeSet,
    active_plans: Vec<String>,
    candidate_plans: Vec<String>,
    missing_art_briefs: Vec<String>,
    artwork_requirements: Vec<String>,
}

pub(super) fn status_output(
    root: &std::path::Path,
    config: &Config,
    prepared: &build::PreparedBuild,
) -> Result<StatusOutput, AppError> {
    let mut active_plans = Vec::new();
    let mut candidate_plans = Vec::new();
    let mut missing_art_briefs = Vec::new();
    let mut artwork_requirements = Vec::new();
    for compendium in &prepared.project.compendiums {
        for story in &compendium.stories {
            let plan_index = storage::load_artifact_index(root, config, "plans", &story.id)?;
            if let Some(active) = plan_index.active {
                active_plans.push(format!("{}:{active}", story.id));
            } else if let Some(plan) = storage::load_latest_plan(root, config, &story.id)? {
                candidate_plans.push(format!("{}:v{:03}", story.id, plan.revision));
            }
            for (art_id, requirement) in
                crate::art::requirements_for_story(root, config, &story.id)?
            {
                artwork_requirements.push(format!(
                    "{art_id}:v{:03}:{:?}",
                    requirement.revision, requirement.status
                ));
                if art_brief::load(root, &art_id)?.is_none() {
                    missing_art_briefs.push(art_id);
                }
            }
        }
    }
    Ok(StatusOutput {
        changes: prepared.changes.clone(),
        active_plans,
        candidate_plans,
        missing_art_briefs,
        artwork_requirements,
    })
}
