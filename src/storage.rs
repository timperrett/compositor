use crate::config::Config;
use crate::model::{Manifest, PagePlan, Resolutions};
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
    read_json(&path).map(Some)
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

pub fn save_plan(root: &Path, config: &Config, plan: &PagePlan) -> Result<(), AppError> {
    write_json_atomic(
        &plan_directory(root, config, &plan.story_id).join(format!("v{:03}.json", plan.revision)),
        plan,
    )
}
