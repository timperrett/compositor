use super::*;
use crate::art_brief::{ArtCandidate, ArtUsage, CandidateGeometry};
use crate::assets::{ApprovedAsset, AssetRecord, AssetRegistry, AssetSelection, AssetStatus};
use crate::composition::{ArtReference, IllustrationIntent};
use crate::flow::SourceRange;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Serialize)]
struct ArtListItem {
    art_id: String,
    story_id: String,
    anchor_id: Option<String>,
    art_layout: Option<crate::model::ArtLayout>,
    geometry: Option<crate::model::ArtGeometry>,
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
}

#[derive(Debug, Serialize)]
struct ArtCoverage {
    story_id: String,
    edition_id: String,
    opener: OpenerCoverage,
    spreads: Vec<SpreadCoverage>,
}

pub(super) fn art_command(
    root: &std::path::Path,
    config: &Config,
    format: OutputFormat,
    command: ArtCommand,
) -> Result<(), AppError> {
    match command {
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
            transition_asset(root, config, format, &art_id, AssetStatus::Review)
        }
        ArtCommand::Approve { art_id } => approve_asset(root, config, format, &art_id),
        ArtCommand::Reject { art_id } => {
            transition_asset(root, config, format, &art_id, AssetStatus::Rejected)
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
            let registry = assets::load(root)?;
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
                        let asset = registry
                            .as_ref()
                            .and_then(|registry| assets::record(registry, &art_id));
                        records.push(ArtListItem {
                            art_id,
                            story_id: requirement.story_id,
                            anchor_id: requirement.anchor_id.clone(),
                            art_layout: requirement.art_layout.clone(),
                            geometry: requirement.geometry.clone(),
                            art_brief: brief.brief.as_ref().map(|_| brief.path.clone()),
                            art_brief_valid: brief.brief.is_some()
                                && brief.validation.can_proceed(),
                            candidate_count: brief
                                .brief
                                .as_ref()
                                .map(|brief| brief.candidates.len())
                                .unwrap_or(0),
                            selected_candidate: asset.and_then(|asset| {
                                asset
                                    .selection
                                    .as_ref()
                                    .map(|selection| selection.candidate_id.clone())
                            }),
                            approved_artwork: asset.and_then(|asset| {
                                asset
                                    .approved
                                    .as_ref()
                                    .map(|approved| approved.file.clone())
                            }),
                        });
                    }
                }
            }
            print_report(format, "art list", records, ValidationReport::default())
        }
        ArtCommand::Inspect { art_id } => {
            let requirement = required_art_requirement(root, config, &art_id)?;
            let brief = art_brief::inspect(root, config, &art_id);
            let registry = assets::load(root)?;
            let asset = registry
                .as_ref()
                .and_then(|registry| assets::record(registry, &art_id));
            print_report(
                format,
                "art inspect",
                serde_json::json!({ "requirement": requirement, "brief": brief, "asset": asset }),
                ValidationReport::default(),
            )
        }
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
                expected_geometry: expected_geometry.clone(),
                attempts: Vec::new(),
            }
        };
        if log.art_id != art_id
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
        let status = aggregate_coverage(&art_assets);
        spreads.push(SpreadCoverage {
            id: flow_spread.id.clone(),
            source: flow_spread.source.clone(),
            illustration: composition_spread.illustration.clone(),
            status,
            art_assets,
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
            CoverageStatus::Invalid
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

fn aggregate_coverage(assets: &[CoverageAsset]) -> CoverageStatus {
    if assets.is_empty() {
        return CoverageStatus::Missing;
    }
    if assets
        .iter()
        .any(|asset| matches!(asset.status, CoverageStatus::Invalid))
    {
        CoverageStatus::Invalid
    } else {
        CoverageStatus::Covered
    }
}

pub(super) fn required_art_requirement(
    root: &std::path::Path,
    config: &Config,
    art_id: &str,
) -> Result<crate::art::DerivedArtRequirement, AppError> {
    let project = discover(root, config)?;
    for story in project
        .compendiums
        .iter()
        .flat_map(|compendium| &compendium.stories)
    {
        if let Some(requirement) =
            crate::art::requirements_for_story(root, config, &story.id)?.remove(art_id)
        {
            return Ok(requirement);
        }
    }
    Err(AppError::Command(format!(
        "unknown art `{art_id}`; add it to the opener or a narrative spread in a Composition Plan"
    )))
}

fn register_asset(
    root: &std::path::Path,
    config: &Config,
    format: OutputFormat,
    art_id: &str,
) -> Result<(), AppError> {
    required_art_requirement(root, config, art_id)?;
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
        selection: None,
        approved: None,
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
    required_art_requirement(root, config, art_id)?;
    let mut brief = art_brief::load(root, art_id)?
        .ok_or_else(|| AppError::command(format!("no art brief exists for `{art_id}`")))?;
    let candidate = brief
        .candidates
        .iter()
        .find(|candidate| candidate.id == candidate_id)
        .ok_or_else(|| AppError::command(format!("unknown candidate `{candidate_id}`")))?;
    let file = candidate.file.clone();
    if !art_brief::validate(root, config, &brief).can_proceed() {
        return Err(AppError::Validation);
    }
    let mut registry = assets::load(root)?.ok_or_else(|| {
        AppError::command(
            "no asset registry exists; run `compositor art register <art-id>` first".into(),
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
        assets::transition(asset, AssetStatus::Draft)?;
    } else if asset.status == AssetStatus::Draft {
    } else {
        return Err(AppError::command(
            "cannot select a candidate for a terminal asset".into(),
        ));
    }
    let sha256 = assets::sha256(root, &file)?;
    asset.selection = Some(AssetSelection {
        candidate_id: candidate_id.into(),
        file,
        sha256,
    });
    if let Some(note) = feedback {
        brief.feedback.push(art_brief::ArtFeedback {
            candidate_id: candidate_id.into(),
            note,
        });
        art_brief::save(root, &brief)?;
    }
    assets::save(root, &registry)?;
    print_report(format, "art select", registry, ValidationReport::default())
}

fn transition_asset(
    root: &std::path::Path,
    config: &Config,
    format: OutputFormat,
    art_id: &str,
    next: AssetStatus,
) -> Result<(), AppError> {
    required_art_requirement(root, config, art_id)?;
    let mut registry =
        assets::load(root)?.ok_or_else(|| AppError::command("no asset registry exists".into()))?;
    let asset = assets::record_mut(&mut registry, art_id)
        .ok_or_else(|| AppError::command(format!("asset `{art_id}` is not registered")))?;
    assets::transition(asset, next)?;
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
    required_art_requirement(root, config, art_id)?;
    let inspection = art_brief::inspect(root, config, art_id);
    if !inspection.validation.can_proceed() {
        return Err(AppError::Validation);
    }
    let mut registry =
        assets::load(root)?.ok_or_else(|| AppError::command("no asset registry exists".into()))?;
    let asset = assets::record_mut(&mut registry, art_id)
        .ok_or_else(|| AppError::command(format!("asset `{art_id}` is not registered")))?;
    if asset.status != AssetStatus::Review {
        return Err(AppError::command(
            "only review assets can be approved".into(),
        ));
    }
    let selection = asset
        .selection
        .clone()
        .ok_or_else(|| AppError::command("review asset has no selected candidate".into()))?;
    let source = root.join(&selection.file);
    if assets::sha256(root, &selection.file)? != selection.sha256 {
        return Err(AppError::command(
            "selected candidate no longer matches its pinned SHA-256".into(),
        ));
    }
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
    let approved_file = relative_path(root, &target);
    let approved_sha256 = assets::sha256(root, &approved_file)?;
    assets::transition(asset, AssetStatus::Approved)?;
    asset.approved = Some(ApprovedAsset {
        file: approved_file,
        sha256: approved_sha256,
    });
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
    assets::transition(asset, AssetStatus::Superseded)?;
    asset.superseded_by = Some(successor.into());
    assets::save(root, &registry)?;
    print_report(
        format,
        "art supersede",
        registry,
        ValidationReport::default(),
    )
}
