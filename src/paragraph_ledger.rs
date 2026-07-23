use crate::markdown::{paragraph_fingerprint, parse_document_at, ParsedDocument};
use crate::AppError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const SCHEMA: &str = "compositor.dev/paragraph-ledger/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ParagraphLedger {
    pub schema: String,
    pub story: LedgerStory,
    pub paragraphs: Vec<LedgerParagraph>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LedgerStory {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LedgerParagraph {
    pub id: String,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_hash: Option<String>,
}

pub fn ledger_path(story: &Path) -> PathBuf {
    story.with_file_name("story.paragraphs.yaml")
}
pub fn annotated_path(story: &Path) -> PathBuf {
    story.with_file_name("story.annotated.md")
}

pub fn load_document(story: &Path) -> Result<ParsedDocument, AppError> {
    let source = fs::read_to_string(story)?;
    let mut parsed = parse_document_at(&source, story)?;
    let ledger_file = ledger_path(story);
    if !ledger_file.exists() {
        return Ok(parsed);
    }
    if !parsed.paragraph_comments.is_empty() {
        return Err(AppError::config(format!(
            "{} has a paragraph ledger and inline paragraph comments",
            story.display()
        )));
    }
    let ledger = read_ledger(&ledger_file)?;
    apply_ledger(story, &mut parsed, &ledger)?;
    let expected = materialize(&source, &parsed)?;
    let annotated = annotated_path(story);
    if !annotated.is_file() || fs::read_to_string(&annotated)? != expected {
        return Err(AppError::config(format!(
            "generated annotated source is stale; run `compositor source sync {} --write`",
            story.display()
        )));
    }
    parsed.source_hash = composite_revision(&source, &ledger)?;
    Ok(parsed)
}

pub fn sync(story: &Path, write: bool) -> Result<ParagraphLedger, AppError> {
    let raw = fs::read_to_string(story)?;
    let clean = strip_inline_comments(&raw);
    let mut parsed = parse_document_at(&clean, story)?;
    let story_id = required_story_id(&parsed, story)?;
    let ledger_file = ledger_path(story);
    let ledger = if ledger_file.exists() {
        let mut ledger = read_ledger(&ledger_file)?;
        reconcile_exact(story, &mut ledger, &parsed)?;
        ledger
    } else {
        let legacy = parse_document_at(&raw, story)?;
        if !legacy.paragraph_comments.is_empty() {
            if legacy
                .paragraphs
                .iter()
                .any(|paragraph| paragraph.id.is_none())
            {
                return Err(AppError::config(format!(
                    "{} has incomplete inline paragraph identifiers",
                    story.display()
                )));
            }
            for (target, source) in parsed.paragraphs.iter_mut().zip(legacy.paragraphs.iter()) {
                target.id = source.id.clone();
            }
        } else {
            assign_new_ids(&story_id, &mut parsed);
        }
        ledger_from_parsed(&story_id, &parsed)
    };
    apply_ledger(story, &mut parsed, &ledger)?;
    if write {
        fs::write(story, &clean)?;
        fs::write(
            &ledger_file,
            serde_yaml::to_string(&ledger).map_err(|e| AppError::serialization(e.to_string()))?,
        )?;
        fs::write(annotated_path(story), materialize(&clean, &parsed)?)?;
    }
    Ok(ledger)
}

pub fn resolve(
    story: &Path,
    old_id: &str,
    candidate_fingerprint: &str,
    write: bool,
) -> Result<ParagraphLedger, AppError> {
    let raw = fs::read_to_string(story)?;
    let clean = strip_inline_comments(&raw);
    let parsed = parse_document_at(&clean, story)?;
    if parsed
        .paragraphs
        .iter()
        .filter(|p| paragraph_fingerprint(&p.content) == candidate_fingerprint)
        .count()
        != 1
    {
        return Err(AppError::config(format!(
            "candidate fingerprint `{candidate_fingerprint}` must identify exactly one paragraph"
        )));
    }
    let ledger_file = ledger_path(story);
    let mut ledger = read_ledger(&ledger_file)?;
    let entry = ledger
        .paragraphs
        .iter_mut()
        .find(|entry| entry.id == old_id)
        .ok_or_else(|| AppError::config(format!("unknown paragraph id `{old_id}`")))?;
    entry.content_hash = candidate_fingerprint.to_owned();
    refresh_context(&mut ledger, &parsed);
    if write {
        let mut enriched = parsed.clone();
        apply_ledger(story, &mut enriched, &ledger)?;
        fs::write(story, &clean)?;
        fs::write(
            &ledger_file,
            serde_yaml::to_string(&ledger).map_err(|e| AppError::serialization(e.to_string()))?,
        )?;
        fs::write(annotated_path(story), materialize(&clean, &enriched)?)?;
    }
    Ok(ledger)
}

fn required_story_id(parsed: &ParsedDocument, path: &Path) -> Result<String, AppError> {
    parsed
        .metadata
        .get("id")
        .and_then(serde_yaml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| AppError::config(format!("missing `id` in {}", path.display())))
}

fn read_ledger(path: &Path) -> Result<ParagraphLedger, AppError> {
    let ledger: ParagraphLedger = serde_yaml::from_str(&fs::read_to_string(path)?)
        .map_err(|e| AppError::serialization(format!("{}: {e}", path.display())))?;
    if ledger.schema != SCHEMA {
        return Err(AppError::config(format!(
            "unsupported paragraph ledger schema in {}",
            path.display()
        )));
    }
    Ok(ledger)
}

fn apply_ledger(
    story: &Path,
    parsed: &mut ParsedDocument,
    ledger: &ParagraphLedger,
) -> Result<(), AppError> {
    if ledger.story.id != required_story_id(parsed, story)?
        || ledger.paragraphs.len() != parsed.paragraphs.len()
    {
        return Err(AppError::config(format!(
            "paragraph ledger does not match {}",
            story.display()
        )));
    }
    let mut ids = BTreeSet::new();
    for (paragraph, entry) in parsed.paragraphs.iter_mut().zip(&ledger.paragraphs) {
        if !ids.insert(&entry.id) || paragraph_fingerprint(&paragraph.content) != entry.content_hash
        {
            return Err(AppError::config(format!(
                "paragraph ledger is stale for {}",
                story.display()
            )));
        }
        paragraph.id = Some(entry.id.clone());
    }
    Ok(())
}

fn reconcile_exact(
    story: &Path,
    ledger: &mut ParagraphLedger,
    parsed: &ParsedDocument,
) -> Result<(), AppError> {
    if ledger.paragraphs.len() != parsed.paragraphs.len() {
        reconcile_count_change(ledger, parsed)?;
        return Ok(());
    }
    let current = parsed
        .paragraphs
        .iter()
        .map(|p| paragraph_fingerprint(&p.content))
        .collect::<Vec<_>>();
    let previous = ledger
        .paragraphs
        .iter()
        .map(|p| p.content_hash.clone())
        .collect::<Vec<_>>();
    if current == previous {
        return Ok(());
    }
    let mut locations = BTreeMap::<String, Vec<usize>>::new();
    for (index, fingerprint) in current.iter().enumerate() {
        locations
            .entry(fingerprint.clone())
            .or_default()
            .push(index);
    }
    let mut reordered = vec![None; current.len()];
    for entry in ledger.paragraphs.iter().cloned() {
        let Some(indices) = locations.get(&entry.content_hash) else {
            let candidates = current
                .iter()
                .filter(|fingerprint| !previous.contains(fingerprint))
                .cloned()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ");
            return Err(AppError::config(format!(
                "changed paragraph `{}` in {}; candidate fingerprints: {}; use `compositor source resolve`",
                entry.id,
                story.display(),
                candidates
            )));
        };
        let index = if indices.len() == 1 {
            indices[0]
        } else if let Some(index) = contextual_match(&entry, &current) {
            index
        } else {
            return Err(AppError::config(format!(
                "ambiguous paragraph `{}` in {}; use `compositor source resolve`",
                entry.id,
                story.display()
            )));
        };
        if reordered[index].is_some() {
            return Err(AppError::config(format!(
                "ambiguous paragraph `{}` in {}; use `compositor source resolve`",
                entry.id,
                story.display()
            )));
        }
        reordered[index] = Some(entry);
    }
    ledger.paragraphs = reordered.into_iter().collect::<Option<Vec<_>>>().unwrap();
    refresh_context(ledger, parsed);
    Ok(())
}

/// Reconcile an editorial merge, split, insertion, or deletion without
/// pretending that altered paragraphs retain a durable identity. Exact
/// paragraphs preserve their IDs; unmatched current paragraphs receive new,
/// content-derived IDs, and removed ledger records disappear. Flow validation
/// subsequently requires any affected range to be reviewed explicitly.
fn reconcile_count_change(
    ledger: &mut ParagraphLedger,
    parsed: &ParsedDocument,
) -> Result<(), AppError> {
    let current = parsed
        .paragraphs
        .iter()
        .map(|paragraph| paragraph_fingerprint(&paragraph.content))
        .collect::<Vec<_>>();
    let mut locations = BTreeMap::<String, Vec<usize>>::new();
    for (index, fingerprint) in current.iter().enumerate() {
        locations
            .entry(fingerprint.clone())
            .or_default()
            .push(index);
    }
    let mut reconciled = vec![None; current.len()];
    for entry in ledger.paragraphs.iter().cloned() {
        let Some(indices) = locations.get(&entry.content_hash) else {
            continue;
        };
        let index = if indices.len() == 1 {
            indices[0]
        } else if let Some(index) = contextual_match(&entry, &current) {
            index
        } else {
            return Err(AppError::config(format!(
                "ambiguous unchanged paragraph `{}` after count change; use `compositor source resolve`",
                entry.id
            )));
        };
        if reconciled[index].is_some() {
            return Err(AppError::config(format!(
                "ambiguous unchanged paragraph `{}` after count change; use `compositor source resolve`",
                entry.id
            )));
        }
        reconciled[index] = Some(entry);
    }
    let mut used = reconciled
        .iter()
        .filter_map(|entry| entry.as_ref().map(|entry| entry.id.clone()))
        .collect::<BTreeSet<_>>();
    for (index, paragraph) in parsed.paragraphs.iter().enumerate() {
        if reconciled[index].is_none() {
            reconciled[index] = Some(LedgerParagraph {
                id: generated_id(&ledger.story.id, &paragraph.content, &mut used),
                content_hash: current[index].clone(),
                before_hash: None,
                after_hash: None,
            });
        }
    }
    ledger.paragraphs = reconciled
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            AppError::config("could not reconcile paragraph identities after count change".into())
        })?;
    refresh_context(ledger, parsed);
    Ok(())
}

fn contextual_match(entry: &LedgerParagraph, current: &[String]) -> Option<usize> {
    let matches = current
        .iter()
        .enumerate()
        .filter(|(_, fingerprint)| *fingerprint == &entry.content_hash)
        .filter(|(index, _)| {
            entry.before_hash.as_ref() == index.checked_sub(1).and_then(|i| current.get(i))
                && entry.after_hash.as_ref() == current.get(index + 1)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    (matches.len() == 1).then_some(matches[0])
}

fn ledger_from_parsed(story_id: &str, parsed: &ParsedDocument) -> ParagraphLedger {
    let mut ledger = ParagraphLedger {
        schema: SCHEMA.to_owned(),
        story: LedgerStory {
            id: story_id.to_owned(),
        },
        paragraphs: parsed
            .paragraphs
            .iter()
            .map(|p| LedgerParagraph {
                id: p.id.clone().expect("assigned"),
                content_hash: paragraph_fingerprint(&p.content),
                before_hash: None,
                after_hash: None,
            })
            .collect(),
    };
    refresh_context(&mut ledger, parsed);
    ledger
}

fn refresh_context(ledger: &mut ParagraphLedger, parsed: &ParsedDocument) {
    let hashes = parsed
        .paragraphs
        .iter()
        .map(|p| paragraph_fingerprint(&p.content))
        .collect::<Vec<_>>();
    for (index, entry) in ledger.paragraphs.iter_mut().enumerate() {
        entry.before_hash = index.checked_sub(1).map(|i| hashes[i].clone());
        entry.after_hash = hashes.get(index + 1).cloned();
    }
}

fn assign_new_ids(story_id: &str, parsed: &mut ParsedDocument) {
    let mut used = BTreeSet::new();
    for paragraph in &mut parsed.paragraphs {
        paragraph.id = Some(generated_id(story_id, &paragraph.content, &mut used));
    }
}

fn generated_id(story_id: &str, content: &str, used: &mut BTreeSet<String>) -> String {
    let words = content
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(8)
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    let mut id = format!("{}-{}", story_id, words.join("-"));
    if words.is_empty() {
        id.push_str("paragraph");
    }
    if !used.insert(id.clone()) {
        id = format!("{}-{}", id, &paragraph_fingerprint(content)[7..15]);
        used.insert(id.clone());
    }
    id
}

fn strip_inline_comments(source: &str) -> String {
    let mut value = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("<!-- paragraph:"))
        .collect::<Vec<_>>()
        .join("\n");
    if source.ends_with('\n') {
        value.push('\n');
    }
    value
}

fn materialize(clean: &str, parsed: &ParsedDocument) -> Result<String, AppError> {
    let body_start = if let Some(after_open) = clean.strip_prefix("---\n") {
        after_open
            .find("\n---\n")
            .map(|index| index + 9)
            .unwrap_or(0)
    } else {
        0
    };
    let mut output = clean.to_owned();
    for paragraph in parsed.paragraphs.iter().rev() {
        let id = paragraph
            .id
            .as_deref()
            .ok_or_else(|| AppError::config("paragraph ledger did not assign an id".into()))?;
        output.insert_str(
            body_start + paragraph.source_start,
            &format!("<!-- paragraph: {id} -->\n"),
        );
    }
    Ok(output)
}

fn composite_revision(source: &str, ledger: &ParagraphLedger) -> Result<String, AppError> {
    let data = serde_json::to_vec(ledger).map_err(|e| AppError::serialization(e.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(source.replace("\r\n", "\n").as_bytes());
    hasher.update(b"\0");
    hasher.update(data);
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STORY: &str =
        "---\nid: ledger-test\ntitle: Ledger Test\n---\n\nFirst paragraph.\n\nSecond paragraph.\n";

    #[test]
    fn migrates_inline_ids_to_clean_source_and_generated_view() {
        let directory = tempfile::tempdir().unwrap();
        let story = directory.path().join("story.md");
        fs::write(
            &story,
            STORY
                .replace(
                    "First paragraph.",
                    "<!-- paragraph: first -->\nFirst paragraph.",
                )
                .replace(
                    "Second paragraph.",
                    "<!-- paragraph: second -->\nSecond paragraph.",
                ),
        )
        .unwrap();

        let ledger = sync(&story, true).unwrap();
        assert_eq!(ledger.paragraphs.len(), 2);
        assert!(!fs::read_to_string(&story)
            .unwrap()
            .contains("<!-- paragraph:"));
        let annotated = fs::read_to_string(annotated_path(&story)).unwrap();
        assert!(annotated.contains("<!-- paragraph: first -->"));
        let parsed = load_document(&story).unwrap();
        assert_eq!(parsed.paragraphs[0].id.as_deref(), Some("first"));
        assert_eq!(parsed.paragraphs[1].id.as_deref(), Some("second"));
        assert_eq!(sync(&story, false).unwrap(), ledger);
    }

    #[test]
    fn rejects_stale_generated_view() {
        let directory = tempfile::tempdir().unwrap();
        let story = directory.path().join("story.md");
        fs::write(&story, STORY).unwrap();
        sync(&story, true).unwrap();
        fs::write(annotated_path(&story), "stale\n").unwrap();

        assert!(load_document(&story).is_err());
    }

    #[test]
    fn resolve_rebinds_an_edited_paragraph() {
        let directory = tempfile::tempdir().unwrap();
        let story = directory.path().join("story.md");
        fs::write(&story, STORY).unwrap();
        let ledger = sync(&story, true).unwrap();
        fs::write(
            &story,
            STORY.replace("Second paragraph.", "Revised paragraph."),
        )
        .unwrap();
        assert!(sync(&story, false).is_err());

        let replacement = paragraph_fingerprint("Revised paragraph.");
        resolve(&story, &ledger.paragraphs[1].id, &replacement, true).unwrap();
        let parsed = load_document(&story).unwrap();
        assert_eq!(
            parsed.paragraphs[1].id.as_deref(),
            Some(ledger.paragraphs[1].id.as_str())
        );
    }

    #[test]
    fn count_changing_compaction_preserves_exact_neighbors_and_assigns_new_ids() {
        let directory = tempfile::tempdir().unwrap();
        let story = directory.path().join("story.md");
        fs::write(
            &story,
            "---\nid: ledger-test\ntitle: Ledger Test\n---\n\nFirst paragraph.\n\nSecond paragraph.\n\nThird paragraph.\n",
        )
        .unwrap();
        let original = sync(&story, true).unwrap();
        fs::write(
            &story,
            "---\nid: ledger-test\ntitle: Ledger Test\n---\n\nFirst paragraph. Second paragraph.\n\nThird paragraph.\n",
        )
        .unwrap();

        let reconciled = sync(&story, true).unwrap();
        assert_eq!(reconciled.paragraphs.len(), 2);
        assert_ne!(reconciled.paragraphs[0].id, original.paragraphs[0].id);
        assert_eq!(reconciled.paragraphs[1].id, original.paragraphs[2].id);
        let parsed = load_document(&story).unwrap();
        assert_eq!(parsed.paragraphs.len(), 2);
    }
}
