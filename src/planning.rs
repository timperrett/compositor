use crate::config::Config;
use crate::identity::ResolvedStory;
use crate::model::{PageAssignment, PagePlan, Story, Unit, SCHEMA_VERSION};
use crate::storage;
use crate::AppError;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub fn make_plan(
    root: &Path,
    config: &Config,
    story: &Story,
    resolved: &ResolvedStory,
    manifest_revision: u64,
) -> Result<PagePlan, AppError> {
    let previous = storage::load_latest_plan(root, config, &story.id)?;
    let fingerprint = config.pagination_fingerprint();
    let rebuild_for_config = previous
        .as_ref()
        .is_none_or(|plan| plan.pagination_fingerprint != fingerprint);
    let mut warnings = overflow_warnings(story, resolved, config);
    let assignments = if rebuild_for_config {
        fresh_assignments(story, resolved, config, 1, &BTreeSet::new(), &mut warnings)
    } else {
        let previous = previous.as_ref().expect("checked above");
        let (mut retained, assigned_ids) = retained_assignments(previous, story, resolved);
        let next_page = next_page_after(&retained, config);
        retained.extend(fresh_assignments(
            story,
            resolved,
            config,
            next_page,
            &assigned_ids,
            &mut warnings,
        ));
        retained
    };
    let revision = previous.map(|plan| plan.revision + 1).unwrap_or(1);
    Ok(PagePlan {
        schema_version: SCHEMA_VERSION,
        story_id: story.id.clone(),
        manifest_revision,
        revision,
        pagination_fingerprint: fingerprint,
        assignments,
        warnings,
    })
}

fn retained_assignments(
    previous: &PagePlan,
    story: &Story,
    resolved: &ResolvedStory,
) -> (Vec<PageAssignment>, BTreeSet<String>) {
    let source = story
        .units
        .iter()
        .zip(&resolved.ids)
        .map(|(unit, id)| (id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let mut retained_ids = BTreeSet::new();
    let assignments = previous
        .assignments
        .iter()
        .filter_map(|assignment| {
            let units = assignment
                .units
                .iter()
                .filter(|id| source.contains_key(id.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            if units.is_empty()
                || units.iter().any(|id| {
                    source
                        .get(id.as_str())
                        .is_none_or(|unit| layout_for(unit) != assignment.layout)
                })
            {
                return None;
            }
            retained_ids.extend(units.iter().cloned());
            Some(PageAssignment {
                pages: assignment.pages.clone(),
                word_count: units
                    .iter()
                    .filter_map(|id| source.get(id.as_str()))
                    .map(|unit| unit.word_count)
                    .sum(),
                units,
                layout: assignment.layout.clone(),
            })
        })
        .collect();
    (assignments, retained_ids)
}

fn fresh_assignments(
    story: &Story,
    resolved: &ResolvedStory,
    config: &Config,
    mut next_page: u32,
    assigned_ids: &BTreeSet<String>,
    warnings: &mut Vec<String>,
) -> Vec<PageAssignment> {
    if config.pagination.story_starts_on_recto && next_page.is_multiple_of(2) {
        next_page += 1;
    }
    let mut assignments = Vec::new();
    let mut text_units = Vec::new();
    let mut text_words = 0;

    let candidates = story
        .units
        .iter()
        .zip(&resolved.ids)
        .filter(|(_, id)| !assigned_ids.contains(*id))
        .collect::<Vec<_>>();
    let mut index = 0;
    while index < candidates.len() {
        let (unit, id) = candidates[index];
        if layout_for(unit) != "text-dominant" {
            flush_text(
                &mut assignments,
                &mut next_page,
                &mut text_units,
                &mut text_words,
            );
            let pages = if layout_for(unit) == "full-spread" {
                vec![next_page, next_page + 1]
            } else {
                vec![next_page]
            };
            next_page += pages.len() as u32;
            assignments.push(PageAssignment {
                pages,
                units: vec![id.clone()],
                layout: layout_for(unit).into(),
                word_count: unit.word_count,
            });
            index += 1;
            continue;
        }

        let mut block = vec![(unit, id)];
        while block
            .last()
            .is_some_and(|(last, _)| last.directives.keep_with_next)
            && index + block.len() < candidates.len()
        {
            let (next_unit, next_id) = candidates[index + block.len()];
            if layout_for(next_unit) != "text-dominant" {
                warnings.push(format!(
                    "unit {} requests keep-with-next, but the following unit has layout `{}`",
                    block.last().expect("non-empty").1,
                    layout_for(next_unit)
                ));
                break;
            }
            block.push((next_unit, next_id));
        }
        let block_len = block.len();
        let block_words = block.iter().map(|(unit, _)| unit.word_count).sum::<usize>();
        if block_words > config.pagination.maximum_words_per_text_page && block.len() > 1 {
            warnings.push(format!(
                "units {} must remain together and total {} words, exceeding the {}-word maximum",
                block
                    .iter()
                    .map(|(_, id)| id.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                block_words,
                config.pagination.maximum_words_per_text_page
            ));
        }
        if !text_units.is_empty()
            && should_break_before(
                text_words,
                block_words,
                config.pagination.target_words_per_text_page,
                config.pagination.maximum_words_per_text_page,
            )
        {
            flush_text(
                &mut assignments,
                &mut next_page,
                &mut text_units,
                &mut text_words,
            );
        }
        text_words += block_words;
        text_units.extend(block.into_iter().map(|(_, id)| id.clone()));
        index += block_len;
    }
    flush_text(
        &mut assignments,
        &mut next_page,
        &mut text_units,
        &mut text_words,
    );
    assignments
}

fn flush_text(
    assignments: &mut Vec<PageAssignment>,
    next_page: &mut u32,
    text_units: &mut Vec<String>,
    text_words: &mut usize,
) {
    if !text_units.is_empty() {
        assignments.push(PageAssignment {
            pages: vec![*next_page],
            units: std::mem::take(text_units),
            layout: "text-dominant".into(),
            word_count: std::mem::take(text_words),
        });
        *next_page += 1;
    }
}

fn should_break_before(current: usize, incoming: usize, target: usize, maximum: usize) -> bool {
    let proposed = current + incoming;
    if proposed > maximum || current >= target {
        return true;
    }
    proposed > target && proposed - target > target - current
}

fn overflow_warnings(story: &Story, resolved: &ResolvedStory, config: &Config) -> Vec<String> {
    story
        .units
        .iter()
        .zip(&resolved.ids)
        .filter(|(unit, _)| {
            layout_for(unit) == "text-dominant"
                && unit.word_count > config.pagination.maximum_words_per_text_page
        })
        .map(|(unit, id)| {
            format!(
                "unit {id} has {} words, exceeding the {}-word maximum",
                unit.word_count, config.pagination.maximum_words_per_text_page
            )
        })
        .collect()
}

fn next_page_after(assignments: &[PageAssignment], config: &Config) -> u32 {
    let next_page = assignments
        .iter()
        .flat_map(|assignment| assignment.pages.iter())
        .max()
        .copied()
        .unwrap_or(0)
        + 1;
    if config.pagination.story_starts_on_recto && next_page.is_multiple_of(2) {
        next_page + 1
    } else {
        next_page
    }
}

fn layout_for(unit: &Unit) -> &str {
    unit.directives.layout.as_deref().unwrap_or_else(|| {
        if unit.directives.unit_type.as_deref() == Some("illustration-only") {
            "illustration-only"
        } else {
            "text-dominant"
        }
    })
}
