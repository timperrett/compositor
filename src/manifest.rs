use crate::identity::ResolvedStory;
use crate::markdown::content_hash;
use crate::model::{
    Manifest, ManifestCompendium, ManifestStory, ManifestUnit, SourceProject, UnitState,
    SCHEMA_VERSION,
};
use std::collections::BTreeMap;

pub fn make_manifest(
    project: &SourceProject,
    resolved: &BTreeMap<String, ResolvedStory>,
    previous: Option<&Manifest>,
    previous_revision: u64,
) -> Manifest {
    let mut compendiums = BTreeMap::new();
    let mut stories = BTreeMap::new();
    for compendium in &project.compendiums {
        compendiums.insert(
            compendium.id.clone(),
            ManifestCompendium {
                source: compendium.source.clone(),
                stories: compendium
                    .stories
                    .iter()
                    .map(|story| story.id.clone())
                    .collect(),
            },
        );
        for story in &compendium.stories {
            let ids = &resolved[&story.id].ids;
            let source_hash = content_hash(
                &story
                    .units
                    .iter()
                    .map(|unit| unit.content_hash.as_str())
                    .collect::<Vec<_>>()
                    .join("|"),
            );
            stories.insert(
                story.id.clone(),
                ManifestStory {
                    source: story.source.clone(),
                    source_hash,
                    ordinal: story.ordinal,
                    units: story
                        .units
                        .iter()
                        .zip(ids)
                        .map(|(unit, id)| ManifestUnit {
                            // Production relationships are keyed by stable unit
                            // identity, not ordinal position, so an edit or move
                            // cannot silently detach approved artwork.
                            art_brief: previous
                                .and_then(|manifest| manifest.stories.get(&story.id))
                                .and_then(|story| story.units.iter().find(|old| old.id == *id))
                                .and_then(|old| old.art_brief.clone()),
                            approved_art: previous
                                .and_then(|manifest| manifest.stories.get(&story.id))
                                .and_then(|story| story.units.iter().find(|old| old.id == *id))
                                .and_then(|old| old.approved_art.clone()),
                            id: id.clone(),
                            anchor: unit.directives.anchor.clone(),
                            ordinal: unit.ordinal,
                            content_hash: unit.content_hash.clone(),
                            normalized_content: unit.normalized_content.clone(),
                            state: UnitState::Active,
                        })
                        .collect(),
                },
            );
        }
    }
    Manifest {
        schema_version: SCHEMA_VERSION,
        tool_version: crate::BUILD_VERSION.into(),
        revision: previous_revision + 1,
        compendiums,
        stories,
    }
}
