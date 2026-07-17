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
                            id: id.clone(),
                            anchor: unit.directives.anchor.clone(),
                            ordinal: unit.ordinal,
                            content_hash: unit.content_hash.clone(),
                            normalized_content: unit.normalized_content.clone(),
                            state: UnitState::Active,
                            asset_path: None,
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
