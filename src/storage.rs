use crate::config::Config;
use crate::model::{ArtifactIndex, IllustrationRequirement, Manifest, PagePlan, Resolutions};
use crate::AppError;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, AppError> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text)
        .map_err(|error| AppError::Serialization(format!("{}: {error}", path.display())))
}

pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Io(std::io::Error::other("path has no parent")))?;
    fs::create_dir_all(parent)?;
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| AppError::Serialization(error.to_string()))?
        + "\n";
    let mut temporary = NamedTempFile::new_in(parent)?;
    use std::io::Write;
    temporary.write_all(text.as_bytes())?;
    temporary.flush()?;
    temporary
        .persist(path)
        .map_err(|error| AppError::Io(error.error))?;
    Ok(())
}

pub fn manifest_path(root: &Path, config: &Config) -> PathBuf {
    config.state_dir(root).join("manifest.json")
}
pub fn resolutions_path(root: &Path, config: &Config) -> PathBuf {
    config.state_dir(root).join("resolutions.json")
}

pub fn load_manifest(root: &Path, config: &Config) -> Result<Option<Manifest>, AppError> {
    let path = manifest_path(root, config);
    if !path.exists() {
        return Ok(None);
    }
    let manifest: Manifest = read_json(&path)?;
    if manifest.schema_version != crate::model::SCHEMA_VERSION {
        return Err(AppError::Config(format!(
            "state schema {} is incompatible with {}; remove .compositor and rebuild",
            manifest.schema_version,
            crate::model::SCHEMA_VERSION
        )));
    }
    Ok(Some(manifest))
}

pub fn load_resolutions(root: &Path, config: &Config) -> Result<Resolutions, AppError> {
    let path = resolutions_path(root, config);
    if !path.exists() {
        return Ok(Resolutions::default());
    }
    read_json(&path)
}

pub fn save_resolutions(
    root: &Path,
    config: &Config,
    resolutions: &Resolutions,
) -> Result<(), AppError> {
    write_json_atomic(&resolutions_path(root, config), resolutions)
}

pub fn save_manifest(root: &Path, config: &Config, manifest: &Manifest) -> Result<(), AppError> {
    let state = config.state_dir(root);
    write_json_atomic(&manifest_path(root, config), manifest)?;
    write_json_atomic(
        &state
            .join("history/manifests")
            .join(format!("v{:03}.json", manifest.revision)),
        manifest,
    )
}

pub fn plan_directory(root: &Path, config: &Config, story_id: &str) -> PathBuf {
    config.state_dir(root).join("plans").join(story_id)
}

pub fn load_latest_plan(
    root: &Path,
    config: &Config,
    story_id: &str,
) -> Result<Option<PagePlan>, AppError> {
    let directory = plan_directory(root, config, story_id);
    if !directory.is_dir() {
        return Ok(None);
    }
    let mut paths = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    paths.sort();
    paths.last().map(|path| read_json(path)).transpose()
}

pub fn load_active_plan(
    root: &Path,
    config: &Config,
    story_id: &str,
) -> Result<Option<PagePlan>, AppError> {
    let index = load_artifact_index(root, config, "plans", story_id)?;
    let Some(file) = index.active else {
        return Ok(None);
    };
    read_json(&plan_directory(root, config, story_id).join(file)).map(Some)
}

pub fn save_plan(root: &Path, config: &Config, plan: &PagePlan) -> Result<(), AppError> {
    write_json_atomic(
        &plan_directory(root, config, &plan.story_id)
            .join(format!("v{:03}-candidate.json", plan.revision)),
        plan,
    )
}

pub fn load_plan_revision(
    root: &Path,
    config: &Config,
    story_id: &str,
    revision: u64,
) -> Result<PagePlan, AppError> {
    let directory = plan_directory(root, config, story_id);
    let prefix = format!("v{revision:03}-");
    let path = fs::read_dir(&directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
        })
        .ok_or_else(|| AppError::Command(format!("no plan {story_id} v{revision:03}")))?;
    read_json(&path)
}

pub fn approve_plan(
    root: &Path,
    config: &Config,
    story_id: &str,
    revision: u64,
) -> Result<String, AppError> {
    let mut plan = load_plan_revision(root, config, story_id, revision)?;
    plan.status = crate::model::ArtifactStatus::Approved;
    let file = format!("v{revision:03}-approved.json");
    write_json_atomic(&plan_directory(root, config, story_id).join(&file), &plan)?;
    let mut index = load_artifact_index(root, config, "plans", story_id)?;
    if let Some(active) = index.active.replace(file.clone()) {
        if !index.candidates.contains(&active) {
            index.candidates.push(active);
        }
    }
    save_artifact_index(root, config, "plans", story_id, &index)?;
    Ok(file)
}

pub fn requirement_directory(root: &Path, config: &Config, art_id: &str) -> PathBuf {
    config.state_dir(root).join("requirements").join(art_id)
}

pub fn requirement_path(root: &Path, config: &Config, art_id: &str, revision: u64) -> PathBuf {
    requirement_directory(root, config, art_id).join(format!("v{revision:03}-candidate.json"))
}

pub fn load_latest_requirement(
    root: &Path,
    config: &Config,
    art_id: &str,
) -> Result<Option<IllustrationRequirement>, AppError> {
    let directory = requirement_directory(root, config, art_id);
    if !directory.is_dir() {
        return Ok(None);
    }
    let mut paths = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    paths.sort();
    paths.last().map(|path| read_json(path)).transpose()
}

pub fn save_requirement(
    root: &Path,
    config: &Config,
    requirement: &IllustrationRequirement,
) -> Result<(), AppError> {
    write_json_atomic(
        &requirement_path(root, config, &requirement.art_id, requirement.revision),
        requirement,
    )
}

pub fn artifact_index_path(root: &Path, config: &Config, kind: &str, id: &str) -> PathBuf {
    config
        .state_dir(root)
        .join("state")
        .join(kind)
        .join(format!("{id}.json"))
}

pub fn load_artifact_index(
    root: &Path,
    config: &Config,
    kind: &str,
    id: &str,
) -> Result<ArtifactIndex, AppError> {
    let path = artifact_index_path(root, config, kind, id);
    if !path.exists() {
        return Ok(ArtifactIndex::default());
    }
    read_json(&path)
}

pub fn save_artifact_index(
    root: &Path,
    config: &Config,
    kind: &str,
    id: &str,
    index: &ArtifactIndex,
) -> Result<(), AppError> {
    write_json_atomic(&artifact_index_path(root, config, kind, id), index)
}

pub fn write_text_atomic(path: &Path, text: &str) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Io(std::io::Error::other("path has no parent")))?;
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    use std::io::Write;
    temporary.write_all(text.as_bytes())?;
    temporary.flush()?;
    temporary
        .persist(path)
        .map_err(|error| AppError::Io(error.error))?;
    Ok(())
}

pub fn write_text_if_changed(path: &Path, text: &str) -> Result<bool, AppError> {
    match fs::read_to_string(path) {
        Ok(existing) if existing == text => Ok(false),
        Ok(_) => {
            write_text_atomic(path, text)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_text_atomic(path, text)?;
            Ok(true)
        }
        Err(error) => Err(AppError::Io(error)),
    }
}
