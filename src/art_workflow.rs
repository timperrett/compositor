//! Shared on-disk artwork lifecycle operations used by both the CLI and local server.

use crate::art_brief::{self, ArtFeedback};
use crate::assets::{self, ApprovedAsset, AssetRegistry, AssetSelection, AssetStatus};
use crate::config::Config;
use crate::discovery::discover;
use crate::AppError;
use serde::Serialize;
use std::fs;
use std::path::Path;

pub(crate) fn required_art_requirement(
    root: &Path,
    config: &Config,
    art_id: &str,
) -> Result<crate::art::DerivedArtRequirement, AppError> {
    let project = discover(root, config)?;
    let mut matches = Vec::new();
    for story in project
        .compendiums
        .iter()
        .flat_map(|compendium| &compendium.stories)
    {
        if let Some(requirement) =
            crate::art::requirements_for_story(root, config, &story.id)?.remove(art_id)
        {
            matches.push(requirement);
        }
    }
    match matches.len() {
        0 => Err(AppError::command(format!(
            "unknown art `{art_id}`; add it to the opener or a narrative spread in a Composition Plan"
        ))),
        1 => Ok(matches.remove(0)),
        _ => Err(AppError::command(format!(
            "art `{art_id}` appears in multiple Composition Plans; resolve the duplicate placement before continuing"
        ))),
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct UnplacedArt {
    pub art_id: String,
    pub story_id: String,
    pub spread_id: String,
    pub composition: String,
}

pub(crate) fn unplace(root: &Path, config: &Config, art_id: &str) -> Result<UnplacedArt, AppError> {
    let requirement = required_art_requirement(root, config, art_id)?;
    let crate::art::ArtPlacement::Spread { spread_id } = &requirement.placement else {
        return Err(AppError::command(format!(
            "art `{art_id}` is opener artwork and cannot be marked not needed; replace its opener reference instead"
        )));
    };
    let project = discover(root, config)?;
    let story = project
        .compendiums
        .iter()
        .flat_map(|compendium| &compendium.stories)
        .find(|story| story.id == requirement.story_id)
        .ok_or_else(|| AppError::command(format!("unknown story `{}`", requirement.story_id)))?;
    let composition = root
        .join(
            Path::new(&story.source)
                .parent()
                .ok_or_else(|| AppError::command("story has no parent".into()))?,
        )
        .join("hardcover.composition.yaml");
    crate::composition::remove_spread_art_reference(&composition, spread_id, art_id)?;
    Ok(UnplacedArt {
        art_id: art_id.into(),
        story_id: requirement.story_id,
        spread_id: spread_id.clone(),
        composition: relative_path(root, &composition),
    })
}

pub(crate) fn select(
    root: &Path,
    config: &Config,
    art_id: &str,
    candidate_id: &str,
    feedback: Option<String>,
) -> Result<AssetRegistry, AppError> {
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
    } else if asset.status != AssetStatus::Draft {
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
        brief.feedback.push(ArtFeedback {
            candidate_id: candidate_id.into(),
            note,
        });
        art_brief::save(root, &brief)?;
    }
    assets::save(root, &registry)?;
    Ok(registry)
}

pub(crate) fn review(
    root: &Path,
    config: &Config,
    art_id: &str,
) -> Result<AssetRegistry, AppError> {
    transition(root, config, art_id, AssetStatus::Review)
}

pub(crate) fn reject(
    root: &Path,
    config: &Config,
    art_id: &str,
) -> Result<AssetRegistry, AppError> {
    transition(root, config, art_id, AssetStatus::Rejected)
}

fn transition(
    root: &Path,
    config: &Config,
    art_id: &str,
    next: AssetStatus,
) -> Result<AssetRegistry, AppError> {
    required_art_requirement(root, config, art_id)?;
    let mut registry =
        assets::load(root)?.ok_or_else(|| AppError::command("no asset registry exists".into()))?;
    let asset = assets::record_mut(&mut registry, art_id)
        .ok_or_else(|| AppError::command(format!("asset `{art_id}` is not registered")))?;
    assets::transition(asset, next)?;
    assets::save(root, &registry)?;
    Ok(registry)
}

pub(crate) fn approve(
    root: &Path,
    config: &Config,
    art_id: &str,
) -> Result<AssetRegistry, AppError> {
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
    if assets::sha256(root, &selection.file)? != selection.sha256 {
        return Err(AppError::command(
            "selected candidate no longer matches its pinned SHA-256".into(),
        ));
    }
    let source = root.join(&selection.file);
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
    Ok(registry)
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
