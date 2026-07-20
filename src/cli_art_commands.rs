use super::*;
use crate::art_brief::{ArtCandidate, ArtUsage, CandidateGeometry};
use crate::assets::{AssetRecord, AssetRegistry, AssetStatus};
use crate::composition::{ArtReference, IllustrationIntent};
use crate::flow::{SourceRange, StoryFlowPlan};
use crate::model::Story;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

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

#[derive(Debug, Serialize)]
struct CandidateIngestOutput {
    art_id: String,
    revision: String,
    attempt: u32,
    accepted: bool,
    expected_geometry: crate::model::ArtGeometry,
    actual_geometry: CandidateGeometry,
    file: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderAttempts {
    schema: String,
    art_id: String,
    requirement_revision: u64,
    expected_geometry: crate::model::ArtGeometry,
    #[serde(default)]
    attempts: Vec<RenderAttempt>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderAttempt {
    attempt: u32,
    file: String,
    status: String,
    reason: String,
    actual_geometry: CandidateGeometry,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
enum CoverageStatus {
    Covered,
    Missing,
    Invalid,
    NeedsMapping,
}

#[derive(Debug, Serialize)]
struct CoverageAsset {
    id: String,
    role: String,
    status: CoverageStatus,
    registry_status: Option<AssetStatus>,
    brief_path: Option<String>,
    spread_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct OpenerCoverage {
    art: CoverageAsset,
}

#[derive(Debug, Serialize)]
struct SpreadCoverage {
    id: String,
    source: SourceRange,
    illustration: IllustrationIntent,
    status: CoverageStatus,
    art_assets: Vec<CoverageAsset>,
    legacy_candidates: Vec<String>,
}

#[derive(Debug, Serialize)]
struct LegacyCoverage {
    art_id: String,
    anchor_id: String,
    candidate_spread_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct ArtCoverage {
    story_id: String,
    edition_id: String,
    opener: OpenerCoverage,
    spreads: Vec<SpreadCoverage>,
    legacy_briefs: Vec<LegacyCoverage>,
}

pub(super) fn art_command(
    root: &std::path::Path,
    config: &Config,
    format: OutputFormat,
    command: ArtCommand,
) -> Result<(), AppError> {
    match command {
        ArtCommand::MigrateBriefs { write } => migrate_briefs(root, format, write),
        ArtCommand::Registry { write } => migrate_registry(root, format, write),
        ArtCommand::Register { art_id } => register_asset(root, config, format, &art_id),
        ArtCommand::IngestCandidate {
            art_id,
            source,
            revision,
            attempt,
        } => ingest_candidate(root, config, format, &art_id, &source, &revision, attempt),
        ArtCommand::Select {
            art_id,
            candidate_id,
            feedback,
        } => select_candidate(root, config, format, &art_id, &candidate_id, feedback),
        ArtCommand::Review { art_id } => {
            transition_asset(root, format, &art_id, AssetStatus::Review)
        }
        ArtCommand::ApproveAsset { art_id } => approve_asset(root, config, format, &art_id),
        ArtCommand::Reject { art_id } => {
            transition_asset(root, format, &art_id, AssetStatus::Rejected)
        }
        ArtCommand::Supersede { art_id, successor } => {
            supersede_asset(root, format, &art_id, &successor)
        }
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
        ArtCommand::Coverage { story, edition } => {
            art_coverage(root, config, format, &story, &edition)
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
            let validation = match assets::load(root)? {
                Some(registry) => ValidationReport {
                    issues: validation
                        .issues
                        .into_iter()
                        .chain(assets::validate(root, &registry).issues)
                        .collect(),
                },
                None => validation,
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

fn ingest_candidate(
    root: &Path,
    config: &Config,
    format: OutputFormat,
    art_id: &str,
    source: &Path,
    revision: &str,
    attempt: u32,
) -> Result<(), AppError> {
    if !valid_revision(revision) || !(1..=3).contains(&attempt) {
        return Err(AppError::command(
            "revision must use rNN format and attempt must be between 1 and 3".into(),
        ));
    }
    if !source.is_file() {
        return Err(AppError::command(format!(
            "candidate source does not exist: {}",
            source.display()
        )));
    }
    let requirement = required_art_requirement(root, config, art_id)?;
    let expected_geometry = requirement
        .geometry
        .clone()
        .ok_or_else(|| AppError::command(format!("art `{art_id}` has no computed geometry")))?;
    let actual_geometry = art_brief::candidate_geometry(source)
        .map_err(|error| AppError::command(format!("candidate source is unreadable: {error}")))?;
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|extension| matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp"))
        .ok_or_else(|| {
            AppError::command("candidate source must be PNG, JPG, JPEG, or WebP".into())
        })?;
    let revision_dir = root.join("assets/drafts").join(art_id).join(revision);
    if art_brief::geometry_matches(&expected_geometry, &actual_geometry) {
        let mut brief = art_brief::load(root, art_id)?
            .ok_or_else(|| AppError::command(format!("no art brief exists for `{art_id}`")))?;
        let candidate_id = next_candidate_id(&brief.candidates)?;
        let file =
            format!("assets/drafts/{art_id}/{revision}/candidate-{candidate_id}.{extension}");
        let destination = root.join(&file);
        if destination.exists() {
            return Err(AppError::command(format!(
                "candidate destination already exists: {}",
                destination.display()
            )));
        }
        fs::create_dir_all(&revision_dir)?;
        fs::copy(source, &destination)?;
        brief.candidates.push(ArtCandidate {
            id: candidate_id,
            file: file.clone(),
            prompt: None,
        });
        if !art_brief::validate(root, config, &brief).can_proceed() {
            fs::remove_file(&destination)?;
            return Err(AppError::Validation);
        }
        art_brief::save(root, &brief)?;
        print_report(
            format,
            "art ingest-candidate",
            CandidateIngestOutput {
                art_id: art_id.into(),
                revision: revision.into(),
                attempt,
                accepted: true,
                expected_geometry,
                actual_geometry,
                file,
            },
            ValidationReport::default(),
        )
    } else {
        let rejected = revision_dir.join("rejected");
        let filename = format!("attempt-{attempt}.{extension}");
        let destination = rejected.join(&filename);
        if destination.exists() {
            return Err(AppError::command(format!(
                "rejected destination already exists: {}",
                destination.display()
            )));
        }
        fs::create_dir_all(&rejected)?;
        fs::copy(source, &destination)?;
        let log_path = revision_dir.join("render-attempts.yaml");
        let mut log = if log_path.is_file() {
            serde_yaml::from_str::<RenderAttempts>(&fs::read_to_string(&log_path)?).map_err(
                |error| AppError::serialization(format!("{}: {error}", log_path.display())),
            )?
        } else {
            RenderAttempts {
                schema: "compositor.dev/render-attempts/v1".into(),
                art_id: art_id.into(),
                requirement_revision: requirement.revision,
                expected_geometry: expected_geometry.clone(),
                attempts: Vec::new(),
            }
        };
        if log.art_id != art_id
            || log.requirement_revision != requirement.revision
            || log.expected_geometry != expected_geometry
            || log.attempts.iter().any(|entry| entry.attempt == attempt)
        {
            return Err(AppError::command(
                "render attempt metadata conflicts with this request".into(),
            ));
        }
        let file = relative_path(root, &destination);
        log.attempts.push(RenderAttempt {
            attempt,
            file: file.clone(),
            status: "rejected".into(),
            reason: "aspect-ratio-incompatible".into(),
            actual_geometry: actual_geometry.clone(),
        });
        storage::write_text_atomic(
            &log_path,
            &serde_yaml::to_string(&log)
                .map_err(|error| AppError::serialization(error.to_string()))?,
        )?;
        print_report(
            format,
            "art ingest-candidate",
            CandidateIngestOutput {
                art_id: art_id.into(),
                revision: revision.into(),
                attempt,
                accepted: false,
                expected_geometry,
                actual_geometry,
                file,
            },
            ValidationReport::default(),
        )
    }
}

fn valid_revision(value: &str) -> bool {
    value.len() >= 3
        && value.starts_with('r')
        && value[1..]
            .chars()
            .all(|character| character.is_ascii_digit())
}

fn next_candidate_id(candidates: &[ArtCandidate]) -> Result<String, AppError> {
    for letter in b'a'..=b'z' {
        let candidate = char::from(letter).to_string();
        if !candidates.iter().any(|existing| existing.id == candidate) {
            return Ok(candidate);
        }
    }
    Err(AppError::command(
        "candidate IDs a through z are exhausted; create a new art brief".into(),
    ))
}

fn art_coverage(
    root: &Path,
    config: &Config,
    format: OutputFormat,
    story_id: &str,
    edition_id: &str,
) -> Result<(), AppError> {
    let project = discover(root, config)?;
    let story = project
        .compendiums
        .iter()
        .flat_map(|compendium| &compendium.stories)
        .find(|story| story.id == story_id)
        .cloned()
        .ok_or_else(|| AppError::command(format!("unknown story `{story_id}`")))?;
    let story_path = root.join(&story.source);
    let directory = story_path.parent().ok_or_else(|| {
        AppError::command(format!("story has no parent directory: {}", story.source))
    })?;
    let flow_path = directory.join("story.flow.yaml");
    let composition_path = directory.join(format!("{edition_id}.composition.yaml"));
    let flow = flow::load_plan(&flow_path)?;
    let composition = composition::load_plan(&composition_path)?;
    let registry = assets::load(root)?;

    let opener = OpenerCoverage {
        art: coverage_asset(
            root,
            registry.as_ref(),
            &composition.opener.art,
            ArtUsage::Opener,
            None,
        )?,
    };
    let legacy = legacy_coverage(root, &story, &flow)?;
    let mut spreads = Vec::new();
    for flow_spread in &flow.spreads {
        let composition_spread = composition
            .spreads
            .iter()
            .find(|spread| spread.id == flow_spread.id)
            .ok_or_else(|| {
                AppError::command(format!("missing composition for {}", flow_spread.id))
            })?;
        let art_assets = composition_spread
            .art_assets
            .iter()
            .map(|asset| {
                coverage_asset(
                    root,
                    registry.as_ref(),
                    asset,
                    ArtUsage::Story,
                    Some(&flow_spread.id),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let legacy_candidates = legacy
            .iter()
            .filter(|entry| entry.candidate_spread_id.as_deref() == Some(&flow_spread.id))
            .map(|entry| entry.art_id.clone())
            .collect::<Vec<_>>();
        let status = aggregate_coverage(&art_assets, !legacy_candidates.is_empty());
        spreads.push(SpreadCoverage {
            id: flow_spread.id.clone(),
            source: flow_spread.source.clone(),
            illustration: composition_spread.illustration.clone(),
            status,
            art_assets,
            legacy_candidates,
        });
    }
    let mut validation = composition::validate_art_usage(root, &flow, &composition)?;
    validation
        .issues
        .extend(composition::validate_story_title(&story, &composition).issues);
    let output = ArtCoverage {
        story_id: story.id,
        edition_id: edition_id.into(),
        opener,
        spreads,
        legacy_briefs: legacy,
    };
    print_report(format, "art coverage", output, validation.clone())?;
    if validation.can_proceed() {
        Ok(())
    } else {
        Err(AppError::Validation)
    }
}

fn coverage_asset(
    root: &Path,
    registry: Option<&AssetRegistry>,
    asset: &ArtReference,
    expected_usage: ArtUsage,
    spread_id: Option<&str>,
) -> Result<CoverageAsset, AppError> {
    let registry_status = registry
        .and_then(|registry| assets::record(registry, &asset.id))
        .map(|record| record.status);
    let brief = art_brief::load(root, &asset.id)?;
    let brief_path = brief
        .as_ref()
        .map(|_| format!("art/briefs/{}.yaml", asset.id));
    let spread_ids = brief
        .as_ref()
        .map(|brief| brief.source.spread_ids.clone())
        .unwrap_or_default();
    let status = match brief {
        None => CoverageStatus::Invalid,
        Some(brief) if registry_status.is_none() || brief.usage != expected_usage => {
            CoverageStatus::Invalid
        }
        Some(brief)
            if expected_usage == ArtUsage::Opener && !brief.source.spread_ids.is_empty() =>
        {
            CoverageStatus::Invalid
        }
        Some(brief) if spread_id.is_some() && brief.source.spread_ids.is_empty() => {
            CoverageStatus::NeedsMapping
        }
        Some(brief)
            if spread_id.is_some_and(|spread_id| {
                !brief.source.spread_ids.iter().any(|id| id == spread_id)
            }) =>
        {
            CoverageStatus::Invalid
        }
        Some(_) => CoverageStatus::Covered,
    };
    Ok(CoverageAsset {
        id: asset.id.clone(),
        role: asset.role.clone(),
        status,
        registry_status,
        brief_path,
        spread_ids,
    })
}

fn aggregate_coverage(assets: &[CoverageAsset], has_legacy_candidate: bool) -> CoverageStatus {
    if assets.is_empty() {
        return if has_legacy_candidate {
            CoverageStatus::NeedsMapping
        } else {
            CoverageStatus::Missing
        };
    }
    if assets
        .iter()
        .any(|asset| matches!(asset.status, CoverageStatus::Invalid))
    {
        CoverageStatus::Invalid
    } else if assets
        .iter()
        .any(|asset| matches!(asset.status, CoverageStatus::NeedsMapping))
    {
        CoverageStatus::NeedsMapping
    } else {
        CoverageStatus::Covered
    }
}

fn legacy_coverage(
    root: &Path,
    story: &Story,
    flow: &StoryFlowPlan,
) -> Result<Vec<LegacyCoverage>, AppError> {
    let mut entries = Vec::new();
    for art_id in art_brief::ids(root)? {
        let Some(brief) = art_brief::load(root, &art_id)? else {
            continue;
        };
        if brief.usage == ArtUsage::Story
            && brief.source.story_id == story.id
            && brief.source.spread_ids.is_empty()
        {
            entries.push(LegacyCoverage {
                art_id,
                anchor_id: brief.source.anchor_id.clone(),
                candidate_spread_id: anchor_candidate_spread(story, flow, &brief.source.anchor_id),
            });
        }
    }
    Ok(entries)
}

fn anchor_candidate_spread(story: &Story, flow: &StoryFlowPlan, anchor_id: &str) -> Option<String> {
    let unit = story
        .units
        .iter()
        .find(|unit| unit.directives.anchor.as_deref() == Some(anchor_id))?;
    let paragraph_id = story
        .paragraphs
        .iter()
        .find(|paragraph| paragraph.source_start >= unit.source_start)?
        .id
        .as_deref()?;
    let positions = story
        .paragraphs
        .iter()
        .enumerate()
        .filter_map(|(index, paragraph)| paragraph.id.as_deref().map(|id| (id, index)))
        .collect::<BTreeMap<_, _>>();
    let position = *positions.get(paragraph_id)?;
    let candidates = flow
        .spreads
        .iter()
        .filter(|spread| {
            let Some(from) = positions.get(spread.source.from.id.as_str()) else {
                return false;
            };
            let Some(through) = positions.get(spread.source.through.id.as_str()) else {
                return false;
            };
            *from <= position && position <= *through
        })
        .map(|spread| spread.id.clone())
        .collect::<Vec<_>>();
    (candidates.len() == 1).then(|| candidates[0].clone())
}

fn migrate_briefs(
    root: &std::path::Path,
    format: OutputFormat,
    write: bool,
) -> Result<(), AppError> {
    let mut migrated = Vec::new();
    for id in art_brief::ids(root)? {
        let path = art_brief::path(root, &id);
        let text = fs::read_to_string(&path)?;
        let mut value: serde_yaml::Value = serde_yaml::from_str(&text)
            .map_err(|error| AppError::serialization(format!("{}: {error}", path.display())))?;
        let mapping = value
            .as_mapping_mut()
            .ok_or_else(|| AppError::command(format!("brief `{id}` is not a YAML mapping")))?;
        let story_id = mapping
            .get(serde_yaml::Value::String("source".into()))
            .and_then(serde_yaml::Value::as_mapping)
            .and_then(|source| source.get(serde_yaml::Value::String("story_id".into())))
            .and_then(serde_yaml::Value::as_str)
            .ok_or_else(|| AppError::command(format!("brief `{id}` has no source.story_id")))?
            .to_owned();
        mapping.insert(
            serde_yaml::Value::String("schema_version".into()),
            serde_yaml::Value::Number(2.into()),
        );
        let mut source = serde_yaml::Mapping::new();
        source.insert(
            serde_yaml::Value::String("story_id".into()),
            serde_yaml::Value::String(story_id),
        );
        source.insert(
            serde_yaml::Value::String("anchor_id".into()),
            serde_yaml::Value::String(id.clone()),
        );
        mapping.insert(
            serde_yaml::Value::String("source".into()),
            serde_yaml::Value::Mapping(source),
        );
        if write {
            storage::write_text_atomic(
                &path,
                &serde_yaml::to_string(&value)
                    .map_err(|error| AppError::serialization(error.to_string()))?,
            )?;
        }
        migrated.push(relative_path(root, &path));
    }
    print_report(
        format,
        "art migrate-briefs",
        migrated,
        ValidationReport::default(),
    )
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

fn migrate_registry(
    root: &std::path::Path,
    format: OutputFormat,
    write: bool,
) -> Result<(), AppError> {
    let mut registry = assets::load(root)?.unwrap_or(AssetRegistry {
        schema: assets::ASSET_REGISTRY_SCHEMA.into(),
        assets: Vec::new(),
    });
    registry.schema = assets::ASSET_REGISTRY_SCHEMA.into();
    for id in art_brief::ids(root)? {
        if assets::record(&registry, &id).is_some() {
            continue;
        }
        let brief = art_brief::load(root, &id)?
            .ok_or_else(|| AppError::command(format!("missing brief `{id}`")))?;
        let (status, file) = match brief.selection.as_ref().and_then(|selection| {
            brief
                .candidates
                .iter()
                .find(|candidate| candidate.id == selection.candidate_id)
        }) {
            Some(candidate) => (AssetStatus::Draft, Some(candidate.file.clone())),
            None => (AssetStatus::Requested, None),
        };
        registry.assets.push(AssetRecord {
            id: id.clone(),
            brief: format!("art/briefs/{id}.yaml"),
            status,
            file,
            superseded_by: None,
        });
    }
    registry
        .assets
        .sort_by(|left, right| left.id.cmp(&right.id));
    if write {
        assets::save(root, &registry)?;
    }
    print_report(
        format,
        "art registry",
        registry,
        ValidationReport::default(),
    )
}

fn register_asset(
    root: &std::path::Path,
    config: &Config,
    format: OutputFormat,
    art_id: &str,
) -> Result<(), AppError> {
    let inspection = art_brief::inspect(root, config, art_id);
    if !inspection.validation.can_proceed() {
        return Err(AppError::Validation);
    }
    let mut registry = assets::load(root)?.unwrap_or(AssetRegistry {
        schema: assets::ASSET_REGISTRY_SCHEMA.into(),
        assets: Vec::new(),
    });
    if assets::record(&registry, art_id).is_some() {
        return Err(AppError::command(format!(
            "asset `{art_id}` is already registered"
        )));
    }
    registry.assets.push(AssetRecord {
        id: art_id.into(),
        brief: format!("art/briefs/{art_id}.yaml"),
        status: AssetStatus::Requested,
        file: None,
        superseded_by: None,
    });
    registry
        .assets
        .sort_by(|left, right| left.id.cmp(&right.id));
    assets::save(root, &registry)?;
    print_report(
        format,
        "art register",
        registry,
        ValidationReport::default(),
    )
}

fn select_candidate(
    root: &std::path::Path,
    config: &Config,
    format: OutputFormat,
    art_id: &str,
    candidate_id: &str,
    feedback: Option<String>,
) -> Result<(), AppError> {
    let mut brief = art_brief::load(root, art_id)?
        .ok_or_else(|| AppError::command(format!("no art brief exists for `{art_id}`")))?;
    let candidate = brief
        .candidates
        .iter()
        .find(|candidate| candidate.id == candidate_id)
        .ok_or_else(|| AppError::command(format!("unknown candidate `{candidate_id}`")))?;
    let file = candidate.file.clone();
    brief.selection = Some(art_brief::ArtSelection {
        candidate_id: candidate_id.into(),
        feedback,
    });
    if !art_brief::validate(root, config, &brief).can_proceed() {
        return Err(AppError::Validation);
    }
    let mut registry = assets::load(root)?.ok_or_else(|| {
        AppError::command(
            "no asset registry exists; run `compositor art registry --write` first".into(),
        )
    })?;
    let asset = assets::record_mut(&mut registry, art_id)
        .ok_or_else(|| AppError::command(format!("asset `{art_id}` is not registered")))?;
    if asset.status == AssetStatus::Approved {
        return Err(AppError::command(
            "approved assets are immutable; create a replacement asset instead".into(),
        ));
    }
    if matches!(asset.status, AssetStatus::Review | AssetStatus::Requested) {
        assets::transition(asset, AssetStatus::Draft, Some(file))?;
    } else if asset.status == AssetStatus::Draft {
        asset.file = Some(file);
    } else {
        return Err(AppError::command(
            "cannot select a candidate for a terminal asset".into(),
        ));
    }
    art_brief::save(root, &brief)?;
    assets::save(root, &registry)?;
    print_report(format, "art select", registry, ValidationReport::default())
}

fn transition_asset(
    root: &std::path::Path,
    format: OutputFormat,
    art_id: &str,
    next: AssetStatus,
) -> Result<(), AppError> {
    let mut registry =
        assets::load(root)?.ok_or_else(|| AppError::command("no asset registry exists".into()))?;
    let asset = assets::record_mut(&mut registry, art_id)
        .ok_or_else(|| AppError::command(format!("asset `{art_id}` is not registered")))?;
    assets::transition(asset, next, None)?;
    assets::save(root, &registry)?;
    print_report(
        format,
        "art lifecycle",
        registry,
        ValidationReport::default(),
    )
}

fn approve_asset(
    root: &std::path::Path,
    config: &Config,
    format: OutputFormat,
    art_id: &str,
) -> Result<(), AppError> {
    let mut registry =
        assets::load(root)?.ok_or_else(|| AppError::command("no asset registry exists".into()))?;
    let asset = assets::record_mut(&mut registry, art_id)
        .ok_or_else(|| AppError::command(format!("asset `{art_id}` is not registered")))?;
    if asset.status != AssetStatus::Review {
        return Err(AppError::command(
            "only review assets can be approved".into(),
        ));
    }
    let source = asset
        .file
        .as_ref()
        .ok_or_else(|| AppError::command("review asset has no file".into()))?;
    let source = root.join(source);
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("png");
    let target = root
        .join(&config.assets.approved_directory)
        .join(format!("{art_id}.{extension}"));
    if target.exists() {
        return Err(AppError::command(format!(
            "approved asset already exists: {}",
            target.display()
        )));
    }
    fs::create_dir_all(
        target
            .parent()
            .ok_or_else(|| AppError::command("approved path has no parent".into()))?,
    )?;
    fs::copy(&source, &target)?;
    assets::transition(
        asset,
        AssetStatus::Approved,
        Some(relative_path(root, &target)),
    )?;
    assets::save(root, &registry)?;
    print_report(format, "art approve", registry, ValidationReport::default())
}

fn supersede_asset(
    root: &std::path::Path,
    format: OutputFormat,
    art_id: &str,
    successor: &str,
) -> Result<(), AppError> {
    let mut registry =
        assets::load(root)?.ok_or_else(|| AppError::command("no asset registry exists".into()))?;
    if assets::record(&registry, successor).is_none() {
        return Err(AppError::command(format!(
            "successor asset `{successor}` is not registered"
        )));
    }
    let asset = assets::record_mut(&mut registry, art_id)
        .ok_or_else(|| AppError::command(format!("asset `{art_id}` is not registered")))?;
    assets::transition(asset, AssetStatus::Superseded, None)?;
    asset.superseded_by = Some(successor.into());
    assets::save(root, &registry)?;
    print_report(
        format,
        "art supersede",
        registry,
        ValidationReport::default(),
    )
}
