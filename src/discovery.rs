use crate::config::Config;
use crate::markdown::parse_document;
use crate::model::{Compendium, SourceProject, Story};
use crate::AppError;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn discover(root: &Path, config: &Config) -> Result<SourceProject, AppError> {
    let base = root.join(&config.source.compendiums_dir);
    if !base.is_dir() {
        return Err(AppError::Config(format!(
            "compendiums directory does not exist: {}",
            base.display()
        )));
    }
    let mut directories = read_sorted(&base, |path| path.is_dir(), config)?;
    let mut compendiums = Vec::new();
    for (ordinal, directory) in directories.drain(..).enumerate() {
        let index = directory.join("index.md");
        if !index.is_file() {
            return Err(AppError::Config(format!(
                "missing compendium index: {}",
                index.display()
            )));
        }
        let parsed_index = parse_document(&fs::read_to_string(&index)?)?;
        let id = required_metadata(&parsed_index.metadata, "id", &index)?;
        let title = required_metadata(&parsed_index.metadata, "title", &index)?;
        let mut files = read_sorted(
            &directory,
            |path| {
                path.extension().is_some_and(|ext| ext == "md")
                    && path.file_name().is_some_and(|name| name != "index.md")
            },
            config,
        )?;
        let mut stories = Vec::new();
        for (story_ordinal, path) in files.drain(..).enumerate() {
            let parsed = parse_document(&fs::read_to_string(&path)?)?;
            let story_id = required_metadata(&parsed.metadata, "id", &path)?;
            let story_title = required_metadata(&parsed.metadata, "title", &path)?;
            stories.push(Story {
                id: story_id,
                title: story_title,
                source: relative(root, &path),
                ordinal: story_ordinal + 1,
                compendium_id: id.clone(),
                metadata: parsed.metadata,
                units: parsed.units,
            });
        }
        compendiums.push(Compendium {
            id,
            title,
            source: relative(root, &index),
            ordinal: ordinal + 1,
            stories,
        });
    }
    Ok(SourceProject { compendiums })
}

fn read_sorted<F>(base: &Path, predicate: F, config: &Config) -> Result<Vec<PathBuf>, AppError>
where
    F: Fn(&Path) -> bool,
{
    let mut entries = fs::read_dir(base)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            !name.starts_with('.')
                && !name.starts_with(&config.ordering.ignore_prefix)
                && !name.ends_with('~')
                && predicate(path)
        })
        .collect::<Vec<_>>();
    entries.sort();
    Ok(entries)
}

fn required_metadata(
    metadata: &BTreeMap<String, serde_yaml::Value>,
    key: &str,
    path: &Path,
) -> Result<String, AppError> {
    metadata
        .get(key)
        .and_then(serde_yaml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| AppError::Config(format!("missing `{key}` in {}", path.display())))
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
