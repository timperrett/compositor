use crate::config::Config;
use crate::markdown::parse_document_at;
use crate::model::{Compendium, SourceProject, Story};
use crate::paragraph_ledger::load_document;
use crate::AppError;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn discover(root: &Path, config: &Config) -> Result<SourceProject, AppError> {
    let base = root.join(&config.source.compendiums_dir);
    if !base.is_dir() {
        return Err(AppError::config(format!(
            "compendiums directory does not exist: {}",
            base.display()
        )));
    }
    let mut directories = read_sorted(&base, |path| path.is_dir(), config)?;
    let mut compendiums = Vec::new();
    for (ordinal, directory) in directories.drain(..).enumerate() {
        let index = directory.join("index.md");
        if !index.is_file() {
            return Err(AppError::config(format!(
                "missing compendium index: {}",
                index.display()
            )));
        }
        let parsed_index = parse_document_at(&fs::read_to_string(&index)?, &index)?;
        let id = required_metadata(&parsed_index.metadata, "id", &index)?;
        let title = required_metadata(&parsed_index.metadata, "title", &index)?;
        let flat_sources = read_sorted(
            &directory,
            |path| {
                path.extension().is_some_and(|extension| extension == "md")
                    && path.file_name().is_some_and(|name| name != "index.md")
            },
            config,
        )?;
        if let Some(path) = flat_sources.first() {
            return Err(AppError::config(format!(
                "flat story sources are unsupported; move {} to a numbered directory containing story.md",
                path.display()
            )));
        }
        let mut directories = read_sorted(&directory, |path| path.is_dir(), config)?;
        let mut stories = Vec::new();
        for (story_ordinal, story_directory) in directories.drain(..).enumerate() {
            let path = story_directory.join("story.md");
            if !path.is_file() {
                return Err(AppError::config(format!(
                    "missing story source: {}",
                    path.display()
                )));
            }
            let parsed = load_document(&path)?;
            let story_id = required_metadata(&parsed.metadata, "id", &path)?;
            let story_title = required_metadata(&parsed.metadata, "title", &path)?;
            stories.push(Story {
                id: story_id,
                title: story_title,
                source: relative(root, &path),
                ordinal: story_ordinal + 1,
                compendium_id: id.clone(),
                source_hash: parsed.source_hash,
                metadata: parsed.metadata,
                units: parsed.units,
                paragraphs: parsed.paragraphs,
                paragraph_comments: parsed.paragraph_comments,
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
        .ok_or_else(|| AppError::config(format!("missing `{key}` in {}", path.display())))
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
