use crate::config::Config;
use crate::identity::ResolvedStory;
use crate::model::{
    ArtSurface, ArtifactStatus, PageAssignment, PageFragment, PageLayout, PagePlan, Story, Unit,
    UnitType, SCHEMA_VERSION,
};
use crate::storage;
use crate::AppError;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(crate) struct PlanRequest<'a> {
    pub root: &'a Path,
    pub config: &'a Config,
    pub story: &'a Story,
    pub resolved: &'a ResolvedStory,
    pub manifest_revision: u64,
    pub preserve_assignments: bool,
}

pub(crate) fn make_plan(request: PlanRequest<'_>) -> Result<PagePlan, AppError> {
    let previous = storage::load_latest_plan(request.root, request.config, &request.story.id)?;
    let fingerprint = request.config.pagination_fingerprint();
    let mut warnings = Vec::new();
    let mut assignments = if let Some(previous) = previous
        .as_ref()
        .filter(|plan| request.preserve_assignments && plan.pagination_fingerprint == fingerprint)
    {
        let (mut retained, assigned) =
            retained_assignments(previous, request.story, request.resolved);
        if !retained.is_empty() {
            warnings.push("conservative planning retained unchanged page assignments; changed units were planned after the retained pages".into());
        }
        let next_page = retained
            .iter()
            .flat_map(|assignment| assignment.pages.iter())
            .max()
            .copied()
            .unwrap_or(0)
            + 1;
        retained.extend(fresh_assignments_from(
            FreshPlanningRequest {
                story: request.story,
                resolved: request.resolved,
                config: request.config,
                next_page,
                assigned_ids: &assigned,
            },
            &mut warnings,
        ));
        retained
    } else {
        fresh_assignments(
            request.story,
            request.resolved,
            request.config,
            &mut warnings,
        )
    };
    attach_art_ids(
        &mut assignments,
        request.story,
        request.resolved,
        &mut warnings,
    );
    let revision = previous.map(|plan| plan.revision + 1).unwrap_or(1);
    Ok(PagePlan {
        schema_version: SCHEMA_VERSION,
        story_id: request.story.id.clone(),
        manifest_revision: request.manifest_revision,
        revision,
        pagination_fingerprint: fingerprint,
        status: ArtifactStatus::Candidate,
        assignments,
        warnings,
    })
}

fn retained_assignments(
    previous: &PagePlan,
    story: &Story,
    resolved: &ResolvedStory,
) -> (Vec<PageAssignment>, BTreeSet<String>) {
    let layouts = story
        .units
        .iter()
        .zip(&resolved.ids)
        .map(|(unit, id)| (id.as_str(), layout_for(unit)))
        .collect::<BTreeMap<_, _>>();
    let stable = resolved
        .changes
        .iter()
        .filter(|change| matches!(change.kind, crate::model::ChangeKind::Unchanged))
        .filter_map(|change| change.unit_id.as_deref())
        .collect::<BTreeSet<_>>();
    let mut assigned = BTreeSet::new();
    let assignments = previous
        .assignments
        .iter()
        .filter(|assignment| {
            !assignment.units.is_empty()
                && assignment.units.iter().all(|id| {
                    stable.contains(id.as_str())
                        && layouts
                            .get(id.as_str())
                            .is_some_and(|layout| *layout == assignment.layout)
                })
        })
        .cloned()
        .inspect(|assignment| assigned.extend(assignment.units.iter().cloned()))
        .collect();
    (assignments, assigned)
}

fn fresh_assignments(
    story: &Story,
    resolved: &ResolvedStory,
    config: &Config,
    warnings: &mut Vec<String>,
) -> Vec<PageAssignment> {
    fresh_assignments_from(
        FreshPlanningRequest {
            story,
            resolved,
            config,
            next_page: 1,
            assigned_ids: &BTreeSet::new(),
        },
        warnings,
    )
}

struct FreshPlanningRequest<'a> {
    story: &'a Story,
    resolved: &'a ResolvedStory,
    config: &'a Config,
    next_page: u32,
    assigned_ids: &'a BTreeSet<String>,
}

fn fresh_assignments_from(
    request: FreshPlanningRequest<'_>,
    warnings: &mut Vec<String>,
) -> Vec<PageAssignment> {
    let FreshPlanningRequest {
        story,
        resolved,
        config,
        mut next_page,
        assigned_ids,
    } = request;
    let mut assignments = Vec::new();
    let mut text_fragments = Vec::new();
    let mut text_words = 0;
    let mut keep_with_next = false;

    let flush_text = |assignments: &mut Vec<PageAssignment>,
                      next_page: &mut u32,
                      fragments: &mut Vec<PageFragment>,
                      words: &mut usize| {
        if !fragments.is_empty() {
            assignments.push(text_assignment(
                *next_page,
                std::mem::take(fragments),
                std::mem::take(words),
            ));
            *next_page += 1;
        }
    };

    for (unit, id) in story.units.iter().zip(&resolved.ids) {
        if assigned_ids.contains(id) {
            continue;
        }
        let layout = layout_for(unit);
        if let Some(art_layout) = &unit.directives.art_layout {
            if layout == PageLayout::FullPage
                && matches!(art_layout.surface, ArtSurface::DoublePageSpread)
            {
                warnings.push(format!(
                    "unit {id} combines layout `full-page` with art-layout surface `double-page-spread`"
                ));
            }
            if layout == PageLayout::FullSpread
                && matches!(art_layout.surface, ArtSurface::SinglePage)
            {
                warnings.push(format!(
                    "unit {id} combines layout `full-spread` with art-layout surface `single-page`"
                ));
            }
        }
        if layout != PageLayout::TextDominant || unit.directives.art_layout.is_some() {
            if keep_with_next {
                warnings.push(format!(
                    "unit {id} requests keep-with-next, but the following unit has layout `{layout}`"
                ));
                keep_with_next = false;
            }
            flush_text(
                &mut assignments,
                &mut next_page,
                &mut text_fragments,
                &mut text_words,
            );
            let pages = if unit
                .directives
                .art_layout
                .as_ref()
                .is_some_and(|layout| matches!(layout.surface, ArtSurface::DoublePageSpread))
                || layout == PageLayout::FullSpread
            {
                vec![next_page, next_page + 1]
            } else {
                vec![next_page]
            };
            next_page += pages.len() as u32;
            assignments.push(PageAssignment {
                pages,
                units: vec![id.clone()],
                fragments: vec![whole_unit_fragment(id, unit)],
                layout,
                word_count: unit.word_count,
                art_id: None,
            });
            continue;
        }

        let fragments = text_fragments_for(
            unit,
            id,
            config.pagination.target_words_per_text_page,
            config.pagination.maximum_words_per_text_page,
            warnings,
        );
        for fragment in fragments {
            let fragment_words = fragment.end_word - fragment.start_word;
            if !text_fragments.is_empty()
                && !keep_with_next
                && should_break_before(
                    text_words,
                    fragment_words,
                    config.pagination.target_words_per_text_page,
                    config.pagination.maximum_words_per_text_page,
                )
            {
                flush_text(
                    &mut assignments,
                    &mut next_page,
                    &mut text_fragments,
                    &mut text_words,
                );
            }
            if !text_fragments.is_empty()
                && keep_with_next
                && text_words + fragment_words > config.pagination.maximum_words_per_text_page
            {
                warnings.push(format!(
                    "keep-with-next forces unit {id} beyond the {}-word maximum",
                    config.pagination.maximum_words_per_text_page
                ));
            }
            text_words += fragment_words;
            text_fragments.push(fragment);
            keep_with_next = false;
        }
        keep_with_next = unit.directives.keep_with_next;
    }
    flush_text(
        &mut assignments,
        &mut next_page,
        &mut text_fragments,
        &mut text_words,
    );
    assignments
}

fn text_fragments_for(
    unit: &Unit,
    id: &str,
    target: usize,
    maximum: usize,
    warnings: &mut Vec<String>,
) -> Vec<PageFragment> {
    if unit.word_count == 0 {
        return vec![whole_unit_fragment(id, unit)];
    }
    let boundaries = semantic_boundaries(&unit.content);
    let mut fragments = Vec::new();
    let mut start_word = 0;
    while start_word < unit.word_count {
        let maximum_end = (start_word + maximum).min(unit.word_count);
        let target_end = (start_word + target).min(unit.word_count);
        let end_word = boundaries
            .iter()
            .filter(|boundary| boundary.word_index > start_word && boundary.word_index <= maximum_end)
            .min_by_key(|boundary| {
                (
                    boundary.word_index.abs_diff(target_end),
                    Reverse(boundary.strength),
                )
            })
            .map(|boundary| boundary.word_index)
            .unwrap_or_else(|| {
                if maximum_end < unit.word_count {
                    warnings.push(format!(
                        "unit {id} has no sentence or paragraph boundary within the {maximum}-word maximum; splitting at word {maximum_end}"
                    ));
                }
                maximum_end
            });
        fragments.push(PageFragment {
            unit_id: id.into(),
            start_word,
            end_word,
        });
        start_word = end_word;
    }
    fragments
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BoundaryStrength {
    Sentence,
    Paragraph,
}

#[derive(Debug, Clone, Copy)]
struct SemanticBoundary {
    word_index: usize,
    strength: BoundaryStrength,
}

fn semantic_boundaries(value: &str) -> Vec<SemanticBoundary> {
    let visible = crate::markdown::strip_directives(value);
    let mut boundaries = Vec::new();
    let mut word_index = 0;
    let mut paragraph_end = None;
    for line in visible.lines() {
        if line.trim().is_empty() {
            if let Some(end) = paragraph_end.take() {
                promote_to_paragraph(&mut boundaries, end);
            }
            continue;
        }
        for word in line.split_whitespace() {
            word_index += 1;
            paragraph_end = Some(word_index);
            if ends_sentence(word) {
                boundaries.push(SemanticBoundary {
                    word_index,
                    strength: BoundaryStrength::Sentence,
                });
            }
        }
    }
    if let Some(end) = paragraph_end {
        promote_to_paragraph(&mut boundaries, end);
    }
    boundaries
}

fn promote_to_paragraph(boundaries: &mut Vec<SemanticBoundary>, word_index: usize) {
    if let Some(boundary) = boundaries
        .iter_mut()
        .rev()
        .find(|boundary| boundary.word_index == word_index)
    {
        boundary.strength = BoundaryStrength::Paragraph;
    } else {
        boundaries.push(SemanticBoundary {
            word_index,
            strength: BoundaryStrength::Paragraph,
        });
    }
}

fn ends_sentence(word: &str) -> bool {
    let trimmed = word.trim_end_matches(|character: char| {
        matches!(character, '"' | '\'' | ')' | ']' | '}' | '*' | '_')
    });
    matches!(trimmed.chars().last(), Some('.' | '!' | '?'))
}

fn whole_unit_fragment(id: &str, unit: &Unit) -> PageFragment {
    PageFragment {
        unit_id: id.into(),
        start_word: 0,
        end_word: unit.word_count,
    }
}

fn text_assignment(page: u32, fragments: Vec<PageFragment>, word_count: usize) -> PageAssignment {
    let mut seen = BTreeSet::new();
    let units = fragments
        .iter()
        .filter(|fragment| seen.insert(fragment.unit_id.clone()))
        .map(|fragment| fragment.unit_id.clone())
        .collect();
    PageAssignment {
        pages: vec![page],
        units,
        fragments,
        layout: PageLayout::TextDominant,
        word_count,
        art_id: None,
    }
}

fn attach_art_ids(
    assignments: &mut [PageAssignment],
    story: &Story,
    resolved: &ResolvedStory,
    warnings: &mut Vec<String>,
) {
    for assignment in assignments {
        let matching = assignment
            .units
            .iter()
            .filter_map(|id| {
                story
                    .units
                    .iter()
                    .zip(&resolved.ids)
                    .find(|(_, resolved_id)| *resolved_id == id)
                    .and_then(|(unit, _)| art_needed(unit).then(|| id.clone()))
            })
            .collect::<Vec<_>>();
        if let Some(art_id) = matching.first() {
            assignment.art_id = Some(art_id.clone());
        }
        if matching.len() > 1 {
            warnings.push(format!(
                "page {:?} contains multiple artwork requirements ({}); each remains tracked separately",
                assignment.pages,
                matching.join(", ")
            ));
        }
    }
}

pub fn art_needed(unit: &Unit) -> bool {
    unit.directives.art.is_some()
        || unit.directives.art_layout.is_some()
        || layout_for(unit).requires_artwork()
}

fn should_break_before(current: usize, incoming: usize, target: usize, maximum: usize) -> bool {
    let proposed = current + incoming;
    if proposed > maximum || current >= target {
        return true;
    }
    proposed > target && proposed - target > target - current
}

fn layout_for(unit: &Unit) -> PageLayout {
    unit.directives.layout.unwrap_or_else(|| {
        if unit.directives.unit_type == Some(UnitType::IllustrationOnly) {
            PageLayout::IllustrationOnly
        } else {
            PageLayout::TextDominant
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Directives;
    use proptest::prelude::*;

    fn unit(content: String) -> Unit {
        let word_count = content.split_whitespace().count();
        Unit {
            ordinal: 1,
            source_start: 0,
            source_end: content.len(),
            normalized_content: content.clone(),
            content_hash: crate::markdown::content_hash(&content),
            content,
            word_count,
            directives: Directives::default(),
        }
    }

    proptest! {
        #[test]
        fn fragments_cover_each_word_once_and_never_exceed_the_maximum(
            words in prop::collection::vec("[a-z]{1,12}", 0..300),
            target in 1usize..60,
            maximum in 1usize..80,
        ) {
            prop_assume!(target <= maximum);
            let source = words.join(" ");
            let unit = unit(source);
            let mut warnings = Vec::new();
            let fragments = text_fragments_for(&unit, "unit", target, maximum, &mut warnings);

            let mut next_start = 0;
            for fragment in &fragments {
                prop_assert_eq!(fragment.start_word, next_start);
                prop_assert!(fragment.end_word > fragment.start_word || unit.word_count == 0);
                prop_assert!(fragment.end_word - fragment.start_word <= maximum);
                next_start = fragment.end_word;
            }
            prop_assert_eq!(next_start, unit.word_count);
        }
    }
}
