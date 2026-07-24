//! Explicit one-time import of legacy `.compositor` production state.

use crate::art_brief::{self, ArtBrief, ArtFeedback};
use crate::assets::{self, ApprovedAsset, AssetRecord, AssetRegistry, AssetSelection, AssetStatus};
use crate::config::Config;
use crate::AppError;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub struct MigrationOptions {
    pub apply: bool,
}

#[derive(Debug, Serialize)]
pub struct MigrationReport {
    pub schema: &'static str,
    pub mode: &'static str,
    pub legacy_manifest: String,
    pub records: Vec<MigrationRecord>,
    pub blockers: Vec<String>,
    pub manual_follow_up: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MigrationRecord {
    pub art_id: String,
    pub outcome: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_sha256: Option<String>,
}

#[derive(Default)]
struct LegacyAsset {
    selected: bool,
    approved: Option<String>,
}

struct PlannedRecord {
    id: String,
    brief: ArtBrief,
    selection: Option<AssetSelection>,
    approved: Option<(ApprovedAsset, PathBuf)>,
}

#[derive(Debug, Deserialize)]
struct LegacyManifest {
    schema_version: u32,
    stories: BTreeMap<String, LegacyManifestStory>,
}

#[derive(Debug, Deserialize)]
struct LegacyManifestStory {
    units: Vec<LegacyManifestUnit>,
}

#[derive(Debug, Deserialize)]
struct LegacyManifestUnit {
    id: String,
    anchor: Option<String>,
    art_brief: Option<String>,
    approved_art: Option<String>,
}

pub fn run(root: &Path, options: MigrationOptions) -> Result<MigrationReport, AppError> {
    let config = migration_config(root)?;
    let manifest_path = root.join(".compositor/manifest.json");
    if !manifest_path.is_file() {
        return Err(AppError::command(format!(
            "no legacy manifest exists at {}",
            manifest_path.display()
        )));
    }
    let manifest: LegacyManifest = crate::storage::read_json(&manifest_path)?;
    let mut report = MigrationReport {
        schema: "compositor.dev/legacy-production-migration/v1",
        mode: if options.apply { "apply" } else { "dry-run" },
        legacy_manifest: relative(root, &manifest_path),
        records: Vec::new(), blockers: Vec::new(),
        manual_follow_up: vec!["Review this report, run Flow/Composition/art validation, then manually archive or remove .compositor/.".into()],
    };
    if manifest.schema_version != 2 {
        report.blockers.push(format!(
            "legacy manifest schema {} is unsupported; expected 2",
            manifest.schema_version
        ));
        return Ok(report);
    }
    let eligible = eligible_art_ids(root, &config, &mut report)?;
    let legacy = legacy_assets(&manifest, &mut report);
    let mut registry = assets::load(root)?.unwrap_or(AssetRegistry {
        schema: assets::ASSET_REGISTRY_SCHEMA.into(),
        assets: Vec::new(),
    });
    if registry.schema != assets::ASSET_REGISTRY_SCHEMA {
        report.blockers.push(format!(
            "{} has unsupported schema `{}`",
            assets::path(root).display(),
            registry.schema
        ));
    }
    let mut planned = Vec::new();
    for (id, state) in legacy {
        if !eligible.contains(&id) {
            report.records.push(MigrationRecord {
                art_id: id,
                outcome: "unresolved".into(),
                detail: "not referenced by a current Composition opener or narrative spread".into(),
                selection_sha256: None,
                approved_sha256: None,
            });
            continue;
        }
        match plan_record(
            root,
            &config,
            &id,
            state.selected,
            state.approved.as_deref(),
        ) {
            Ok(record) => planned.push(record),
            Err(error) => {
                report.blockers.push(error.to_string());
                report.records.push(MigrationRecord {
                    art_id: id,
                    outcome: "blocked".into(),
                    detail: error.to_string(),
                    selection_sha256: None,
                    approved_sha256: None,
                });
            }
        }
    }
    for record in &planned {
        if let Some(existing) = assets::record(&registry, &record.id) {
            if existing.brief != format!("art/briefs/{}.yaml", record.id) {
                report.blockers.push(format!(
                    "{} has an incompatible existing registry brief link",
                    record.id
                ));
                continue;
            }
            if let Some(selection) = &record.selection {
                if existing
                    .selection
                    .as_ref()
                    .is_some_and(|current| current != selection)
                {
                    report.blockers.push(format!(
                        "{} has an incompatible existing selected candidate",
                        record.id
                    ));
                    continue;
                }
            }
            if let Some((approved, _)) = &record.approved {
                if existing
                    .approved
                    .as_ref()
                    .is_some_and(|current| current != approved)
                {
                    report.blockers.push(format!(
                        "{} has an incompatible existing approved artifact",
                        record.id
                    ));
                    continue;
                }
            }
        }
        if let Some((approved, _)) = &record.approved {
            let target = root.join(&approved.file);
            if target.is_file() && assets::sha256(root, &approved.file)? != approved.sha256 {
                report.blockers.push(format!(
                    "{} would overwrite a different approved artifact at {}",
                    record.id, approved.file
                ));
                continue;
            }
        }
        report.records.push(MigrationRecord {
            art_id: record.id.clone(),
            outcome: if record.approved.is_some() {
                "approved"
            } else if record.selection.is_some() {
                "review"
            } else {
                "requested"
            }
            .into(),
            detail: "eligible legacy record prepared for migration".into(),
            selection_sha256: record.selection.as_ref().map(|value| value.sha256.clone()),
            approved_sha256: record.approved.as_ref().map(|value| value.0.sha256.clone()),
        });
    }
    if !report.blockers.is_empty() {
        return Ok(report);
    }
    if options.apply {
        for record in &planned {
            write_brief_if_legacy(root, &record.id, &record.brief)?;
            if let Some((approved, source)) = &record.approved {
                let target = root.join(&approved.file);
                if target.is_file() {
                    if assets::sha256(root, &approved.file)? != approved.sha256 {
                        return Err(AppError::command(format!(
                            "approved target hash changed for {}",
                            record.id
                        )));
                    }
                } else {
                    fs::create_dir_all(target.parent().ok_or_else(|| {
                        AppError::command("approved target has no parent".into())
                    })?)?;
                    fs::copy(source, &target)?;
                }
            }
            merge_registry(&mut registry, record);
        }
        registry
            .assets
            .sort_by(|left, right| left.id.cmp(&right.id));
        assets::save(root, &registry)?;
        crate::storage::write_json_atomic(
            &root.join("output/reports/legacy-production-migration.json"),
            &report,
        )?;
    }
    Ok(report)
}

fn migration_config(root: &Path) -> Result<Config, AppError> {
    let path = root.join("compositor.toml");
    let text = fs::read_to_string(&path)?;
    let mut value: toml::Value = toml::from_str(&text)
        .map_err(|error| AppError::config(format!("{}: {error}", path.display())))?;
    let table = value
        .as_table_mut()
        .ok_or_else(|| AppError::config("compositor.toml must be a table".into()))?;
    for legacy in ["state", "build", "pagination"] {
        table.remove(legacy);
    }
    let config: Config = value
        .try_into()
        .map_err(|error| AppError::config(format!("{}: {error}", path.display())))?;
    config.validate()?;
    Ok(config)
}

fn eligible_art_ids(
    root: &Path,
    config: &Config,
    report: &mut MigrationReport,
) -> Result<BTreeSet<String>, AppError> {
    let project = crate::discovery::discover(root, config)?;
    let mut ids = BTreeSet::new();
    for story in project
        .compendiums
        .iter()
        .flat_map(|compendium| &compendium.stories)
    {
        let directory = root.join(
            Path::new(&story.source)
                .parent()
                .ok_or_else(|| AppError::command("story has no parent".into()))?,
        );
        let flow_path = directory.join("story.flow.yaml");
        let composition_path = directory.join("hardcover.composition.yaml");
        if !flow_path.is_file() || !composition_path.is_file() {
            report.blockers.push(format!(
                "{} requires story.flow.yaml and hardcover.composition.yaml",
                story.id
            ));
            continue;
        }
        let source = crate::flow::load_story(&root.join(&story.source))?;
        let flow = crate::flow::load_plan(&flow_path)?;
        let composition = crate::composition::load_plan(&composition_path)?;
        let catalog = crate::composition::load_catalog(
            &root
                .join("design-systems")
                .join(&composition.edition.design_system),
        )?;
        let flow_validation = crate::flow::validate(
            &source,
            &flow,
            &crate::flow::load_design_system(
                &root
                    .join("design-systems")
                    .join(&composition.edition.design_system),
            )?,
        );
        let composition_validation = crate::composition::validate(&flow, &composition, &catalog);
        let title_validation = crate::composition::validate_story_title(&source, &composition);
        if !flow_validation.can_proceed()
            || !composition_validation.can_proceed()
            || !title_validation.can_proceed()
        {
            report.blockers.push(format!(
                "{} has an invalid Flow/Composition relationship",
                story.id
            ));
            continue;
        }
        ids.insert(composition.opener.art.id);
        ids.extend(
            composition
                .spreads
                .iter()
                .flat_map(|spread| spread.art_assets.iter().map(|asset| asset.id.clone())),
        );
    }
    Ok(ids)
}

fn legacy_assets(
    manifest: &LegacyManifest,
    report: &mut MigrationReport,
) -> BTreeMap<String, LegacyAsset> {
    let mut output = BTreeMap::new();
    for story in manifest.stories.values() {
        for unit in &story.units {
            let id = unit
                .art_brief
                .as_deref()
                .and_then(|path| Path::new(path).file_stem())
                .and_then(|value| value.to_str())
                .map(str::to_owned)
                .or_else(|| unit.anchor.clone())
                .unwrap_or_else(|| unit.id.clone());
            let entry = output.entry(id).or_insert_with(LegacyAsset::default);
            entry.selected |= unit.art_brief.is_some();
            if let Some(approved) = &unit.approved_art {
                if entry.approved.replace(approved.clone()).is_some() {
                    report
                        .blockers
                        .push(format!("legacy approved art is ambiguous for {}", unit.id));
                }
            }
        }
    }
    output
}

fn plan_record(
    root: &Path,
    config: &Config,
    id: &str,
    selected: bool,
    approved: Option<&str>,
) -> Result<PlannedRecord, AppError> {
    let path = art_brief::path(root, id);
    if !path.is_file() {
        return Err(AppError::command(format!(
            "{id}: missing brief at {}",
            path.display()
        )));
    }
    let (brief, legacy_selection) = upgrade_brief(&path, id)?;
    let selection = if selected {
        selected_candidate(root, config, &brief, legacy_selection.as_deref())?
    } else {
        None
    };
    let approved = approved
        .map(|file| approved_asset(root, config, &brief, id, file))
        .transpose()?;
    Ok(PlannedRecord {
        id: id.into(),
        brief,
        selection,
        approved,
    })
}

fn upgrade_brief(path: &Path, id: &str) -> Result<(ArtBrief, Option<String>), AppError> {
    let mut value: Value = serde_yaml::from_str(&fs::read_to_string(path)?)
        .map_err(|error| AppError::serialization(format!("{}: {error}", path.display())))?;
    let mapping = value
        .as_mapping_mut()
        .ok_or_else(|| AppError::command(format!("{id}: brief is not a YAML mapping")))?;
    let selection = mapping.remove(Value::String("selection".into()));
    let candidate_id = selection
        .as_ref()
        .and_then(|value| value.get("candidate_id"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let feedback = selection.and_then(|value| {
        value
            .get("feedback")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    mapping.insert(
        Value::String("schema_version".into()),
        Value::Number(3.into()),
    );
    if let (Some(candidate_id), Some(note)) = (candidate_id.as_deref(), feedback) {
        mapping.insert(
            Value::String("feedback".into()),
            serde_yaml::to_value(vec![ArtFeedback {
                candidate_id: candidate_id.into(),
                note,
            }])
            .map_err(|error| AppError::serialization(error.to_string()))?,
        );
    }
    let brief = serde_yaml::from_value(value)
        .map_err(|error| AppError::serialization(format!("{id}: {error}")))?;
    Ok((brief, candidate_id))
}

fn selected_candidate(
    root: &Path,
    config: &Config,
    brief: &ArtBrief,
    id: Option<&str>,
) -> Result<Option<AssetSelection>, AppError> {
    let Some(id) = id else { return Ok(None) };
    let candidate = brief
        .candidates
        .iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| {
            AppError::command(format!(
                "{}: selected candidate `{id}` is absent",
                brief.art_id
            ))
        })?;
    validate_geometry(root, config, brief, &candidate.file)?;
    Ok(Some(AssetSelection {
        candidate_id: id.into(),
        file: candidate.file.clone(),
        sha256: assets::sha256(root, &candidate.file)?,
    }))
}

fn approved_asset(
    root: &Path,
    config: &Config,
    brief: &ArtBrief,
    id: &str,
    file: &str,
) -> Result<(ApprovedAsset, PathBuf), AppError> {
    let source = root.join(file);
    if !source.is_file() {
        return Err(AppError::command(format!(
            "{id}: legacy approved artwork is missing: {file}"
        )));
    }
    validate_geometry(root, config, brief, file)?;
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("png");
    Ok((
        ApprovedAsset {
            file: format!("{}/{}.{}", config.assets.approved_directory, id, extension),
            sha256: assets::sha256(root, file)?,
        },
        source,
    ))
}

fn validate_geometry(
    root: &Path,
    config: &Config,
    brief: &ArtBrief,
    file: &str,
) -> Result<(), AppError> {
    let project = crate::discovery::discover(root, config)?;
    let story = project
        .compendiums
        .iter()
        .flat_map(|compendium| &compendium.stories)
        .find(|story| story.id == brief.source.story_id)
        .ok_or_else(|| AppError::command(format!("{}: source story is missing", brief.art_id)))?;
    let Some(layout) = story
        .units
        .iter()
        .find(|unit| unit.directives.anchor.as_deref() == Some(&brief.source.anchor_id))
        .and_then(|unit| unit.directives.art_layout.as_ref())
    else {
        return Ok(());
    };
    let expected = crate::art::geometry(config, layout);
    let actual = art_brief::candidate_geometry(&root.join(file)).map_err(AppError::command)?;
    if art_brief::geometry_matches(&expected, &actual) {
        Ok(())
    } else {
        Err(AppError::command(format!(
            "{}: {} does not match current art geometry",
            brief.art_id, file
        )))
    }
}

fn write_brief_if_legacy(root: &Path, id: &str, brief: &ArtBrief) -> Result<(), AppError> {
    let path = art_brief::path(root, id);
    let current: Value = serde_yaml::from_str(&fs::read_to_string(&path)?)
        .map_err(|error| AppError::serialization(format!("{}: {error}", path.display())))?;
    if current
        .get("schema_version")
        .and_then(Value::as_u64)
        .is_some_and(|version| version == u64::from(art_brief::ART_BRIEF_VERSION))
    {
        return Ok(());
    }
    art_brief::save(root, brief)
}

fn merge_registry(registry: &mut AssetRegistry, record: &PlannedRecord) {
    let status = if record.approved.is_some() {
        AssetStatus::Approved
    } else if record.selection.is_some() {
        AssetStatus::Review
    } else {
        AssetStatus::Requested
    };
    if let Some(existing) = assets::record_mut(registry, &record.id) {
        if existing.selection.is_none() {
            existing.selection = record.selection.clone();
        }
        if existing.approved.is_none() {
            existing.approved = record.approved.as_ref().map(|value| value.0.clone());
        }
        if existing.status == AssetStatus::Requested {
            existing.status = status;
        }
    } else {
        registry.assets.push(AssetRecord {
            id: record.id.clone(),
            brief: format!("art/briefs/{}.yaml", record.id),
            status,
            selection: record.selection.clone(),
            approved: record.approved.as_ref().map(|value| value.0.clone()),
            superseded_by: None,
        });
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
