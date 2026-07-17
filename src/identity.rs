use crate::config::Config;
use crate::model::{
    Change, ChangeKind, ChangeSet, Manifest, ManifestUnit, Resolutions, SourceProject, Story, Unit,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct ResolvedStory {
    pub ids: Vec<String>,
    pub changes: Vec<Change>,
}

pub fn resolve_project(
    project: &SourceProject,
    previous: Option<&Manifest>,
    resolutions: &Resolutions,
    config: &Config,
) -> (BTreeMap<String, ResolvedStory>, ChangeSet) {
    let mut result = BTreeMap::new();
    let mut all_changes = Vec::new();
    for compendium in &project.compendiums {
        for story in &compendium.stories {
            let prior = previous.and_then(|manifest| manifest.stories.get(&story.id));
            let resolved =
                resolve_story(story, prior.map(|story| &story.units), resolutions, config);
            all_changes.extend(resolved.changes.clone());
            result.insert(story.id.clone(), resolved);
        }
    }
    (
        result,
        ChangeSet {
            changes: all_changes,
        },
    )
}

fn resolve_story(
    story: &Story,
    previous: Option<&Vec<ManifestUnit>>,
    resolutions: &Resolutions,
    config: &Config,
) -> ResolvedStory {
    let prior = previous.cloned().unwrap_or_default();
    let mut ids = vec![String::new(); story.units.len()];
    let mut used_prior = BTreeSet::new();
    let mut changes = Vec::new();

    for (index, unit) in story.units.iter().enumerate() {
        if let Some(anchor) = &unit.directives.anchor {
            ids[index] = anchor.clone();
            if let Some((prior_index, prior_unit)) = prior
                .iter()
                .enumerate()
                .find(|(_, old)| old.id == *anchor || old.anchor.as_deref() == Some(anchor))
            {
                used_prior.insert(prior_index);
                changes.push(classify(story, unit, prior_unit, anchor, None));
            } else {
                changes.push(inserted(story, anchor));
            }
        }
    }

    for (index, unit) in story.units.iter().enumerate() {
        if !ids[index].is_empty() {
            continue;
        }
        let provisional = provisional_id(&story.id, unit);
        if let Some((old_id, _)) = resolutions
            .mappings
            .iter()
            .find(|(_, current)| *current == &provisional)
        {
            if let Some((prior_index, prior_unit)) =
                prior.iter().enumerate().find(|(_, old)| old.id == *old_id)
            {
                ids[index] = old_id.clone();
                used_prior.insert(prior_index);
                changes.push(classify(story, unit, prior_unit, old_id, Some(1.0)));
                continue;
            }
        }
        if let Some((prior_index, prior_unit)) =
            prior.iter().enumerate().find(|(prior_index, old)| {
                !used_prior.contains(prior_index) && old.content_hash == unit.content_hash
            })
        {
            ids[index] = prior_unit.id.clone();
            used_prior.insert(prior_index);
            changes.push(classify(story, unit, prior_unit, &prior_unit.id, Some(1.0)));
        }
    }

    for (index, unit) in story.units.iter().enumerate() {
        if !ids[index].is_empty() {
            continue;
        }
        let mut candidates = prior
            .iter()
            .enumerate()
            .filter(|(prior_index, _)| !used_prior.contains(prior_index))
            .map(|(prior_index, old)| (prior_index, similarity(unit, old)))
            .filter(|(_, score)| *score >= config.build.similarity_threshold)
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        if let Some((prior_index, score)) = candidates.first().copied() {
            if candidates
                .get(1)
                .is_some_and(|candidate| (score - candidate.1).abs() < 0.05)
            {
                let provisional = provisional_id(&story.id, unit);
                ids[index] = provisional.clone();
                changes.push(Change {
                    kind: ChangeKind::Ambiguous,
                    story_id: story.id.clone(),
                    unit_id: Some(provisional),
                    previous_unit_id: None,
                    confidence: Some(score),
                    message: "multiple historical units match this unit with similar confidence"
                        .into(),
                });
            } else {
                let prior_unit = &prior[prior_index];
                ids[index] = prior_unit.id.clone();
                used_prior.insert(prior_index);
                changes.push(classify(
                    story,
                    unit,
                    prior_unit,
                    &prior_unit.id,
                    Some(score),
                ));
            }
        } else {
            let provisional = provisional_id(&story.id, unit);
            ids[index] = provisional.clone();
            changes.push(inserted(story, &provisional));
        }
    }

    for (index, prior_unit) in prior.iter().enumerate() {
        if !used_prior.contains(&index) {
            changes.push(Change {
                kind: ChangeKind::Deleted,
                story_id: story.id.clone(),
                unit_id: None,
                previous_unit_id: Some(prior_unit.id.clone()),
                confidence: None,
                message: "prior unit is absent from the current story".into(),
            });
        }
    }
    let aggregate = aggregate_changes(story, &prior, &changes, config);
    changes.extend(aggregate);
    ResolvedStory { ids, changes }
}

fn classify(
    story: &Story,
    unit: &Unit,
    prior: &ManifestUnit,
    id: &str,
    confidence: Option<f64>,
) -> Change {
    let kind = if unit.content_hash != prior.content_hash {
        ChangeKind::Edited
    } else if unit.ordinal != prior.ordinal {
        ChangeKind::Moved
    } else {
        ChangeKind::Unchanged
    };
    Change {
        kind,
        story_id: story.id.clone(),
        unit_id: Some(id.into()),
        previous_unit_id: Some(prior.id.clone()),
        confidence,
        message: match kind {
            ChangeKind::Unchanged => "unit is unchanged".into(),
            ChangeKind::Moved => "unit content is unchanged but its position moved".into(),
            _ => "unit identity is retained but its content changed".into(),
        },
    }
}

fn inserted(story: &Story, id: &str) -> Change {
    Change {
        kind: ChangeKind::Inserted,
        story_id: story.id.clone(),
        unit_id: Some(id.into()),
        previous_unit_id: None,
        confidence: None,
        message: "new unit has no prior match".into(),
    }
}

fn provisional_id(story_id: &str, unit: &Unit) -> String {
    format!(
        "{story_id}:u-{}",
        &unit
            .content_hash
            .strip_prefix("sha256:")
            .unwrap_or(&unit.content_hash)[..6]
    )
}

fn similarity(unit: &Unit, previous: &ManifestUnit) -> f64 {
    let unit_tokens = token_set(&unit.normalized_content);
    let previous_tokens = token_set(&previous.normalized_content);
    if unit_tokens.is_empty() || previous_tokens.is_empty() {
        return 0.0;
    }
    let common = unit_tokens.intersection(&previous_tokens).count() as f64;
    let union = unit_tokens.union(&previous_tokens).count() as f64;
    let token_score = common / union;
    let first_score =
        first_sentence(&unit.normalized_content) == first_sentence(&previous.normalized_content);
    let last_score =
        last_sentence(&unit.normalized_content) == last_sentence(&previous.normalized_content);
    (token_score * 0.7) + if first_score { 0.15 } else { 0.0 } + if last_score { 0.15 } else { 0.0 }
}

fn token_set(value: &str) -> BTreeSet<&str> {
    value.split_whitespace().collect()
}

fn first_sentence(value: &str) -> &str {
    value.split(['.', '!', '?']).next().unwrap_or(value).trim()
}
fn last_sentence(value: &str) -> &str {
    value.rsplit(['.', '!', '?']).next().unwrap_or(value).trim()
}

fn aggregate_changes(
    story: &Story,
    prior: &[ManifestUnit],
    existing: &[Change],
    _config: &Config,
) -> Vec<Change> {
    let mut output = Vec::new();
    let inserted = existing
        .iter()
        .filter(|change| change.kind == ChangeKind::Inserted)
        .count();
    let deleted = existing
        .iter()
        .filter(|change| change.kind == ChangeKind::Deleted)
        .count();
    if inserted >= 2 && deleted == 1 {
        output.push(Change {
            kind: ChangeKind::Split,
            story_id: story.id.clone(),
            unit_id: None,
            previous_unit_id: None,
            confidence: None,
            message: "unmatched units may be a split; add an anchor or resolution to confirm"
                .into(),
        });
    }
    if inserted == 1 && deleted >= 2 {
        output.push(Change {
            kind: ChangeKind::Merged,
            story_id: story.id.clone(),
            unit_id: None,
            previous_unit_id: None,
            confidence: None,
            message: "unmatched units may be a merge; add an anchor or resolution to confirm"
                .into(),
        });
    }
    let _ = prior;
    output
}
