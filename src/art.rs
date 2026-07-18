use crate::identity::ResolvedStory;
use crate::model::{
    ArtGeometry, ArtLayout, ArtOrientation, ArtSurface, ArtifactStatus, IllustrationRequirement,
    PagePlan, Story, SCHEMA_VERSION,
};
use crate::planning::art_needed;
use crate::storage;
use crate::{config::Config, AppError};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Synchronize illustration requirements from a newly-created plan. Art briefs
/// are external, skill-authored protocol records in `art/briefs/`.
pub fn sync_requirements(
    root: &Path,
    config: &Config,
    story: &Story,
    resolved: &ResolvedStory,
    plan: &PagePlan,
) -> Result<Vec<IllustrationRequirement>, AppError> {
    let mut requirements = Vec::new();
    for (unit, id) in story.units.iter().zip(&resolved.ids) {
        if !art_needed(unit) {
            continue;
        }
        let pages = plan
            .assignments
            .iter()
            .filter(|assignment| assignment.units.iter().any(|unit_id| unit_id == id))
            .flat_map(|assignment| assignment.pages.iter().copied())
            .collect::<Vec<_>>();
        let layout = plan
            .assignments
            .iter()
            .find(|assignment| assignment.units.iter().any(|unit_id| unit_id == id))
            .map(|assignment| assignment.layout.clone())
            .unwrap_or_else(|| "text-dominant".into());
        let previous = storage::load_latest_requirement(root, config, id)?;
        if let Some(previous) = previous.as_ref() {
            if previous.story_id == story.id
                && previous.unit_ids == [id.clone()]
                && previous.pages == pages
                && previous.layout == layout
                && previous.art_note == unit.directives.art
                && previous.art_layout == unit.directives.art_layout
            {
                requirements.push(previous.clone());
                continue;
            }
        }
        let revision = previous
            .as_ref()
            .map(|record| record.revision + 1)
            .unwrap_or(1);
        let record = IllustrationRequirement {
            schema_version: SCHEMA_VERSION,
            art_id: id.clone(),
            story_id: story.id.clone(),
            unit_ids: vec![id.clone()],
            pages,
            layout,
            status: ArtifactStatus::NeedsReview,
            revision,
            art_layout: unit.directives.art_layout.clone(),
            geometry: unit
                .directives
                .art_layout
                .as_ref()
                .map(|layout| geometry(config, layout)),
            art_note: unit.directives.art.clone(),
        };
        storage::save_requirement(root, config, &record)?;
        requirements.push(record);
    }
    Ok(requirements)
}

pub fn geometry(config: &Config, layout: &ArtLayout) -> ArtGeometry {
    let envelope = match (&layout.surface, &layout.orientation) {
        (ArtSurface::SinglePage, ArtOrientation::Portrait) => {
            config.art_layout.single_page_portrait_width_fraction
        }
        (ArtSurface::SinglePage, ArtOrientation::Landscape) => {
            config.art_layout.single_page_landscape_width_fraction
        }
        (ArtSurface::DoublePageSpread, ArtOrientation::Portrait) => {
            config.art_layout.double_page_portrait_width_fraction
        }
        (ArtSurface::DoublePageSpread, ArtOrientation::Landscape) => {
            config.art_layout.double_page_landscape_width_fraction
        }
    };
    let surface_width_in = match layout.surface {
        ArtSurface::SinglePage => config.book.trim_width_in,
        ArtSurface::DoublePageSpread => {
            (config.book.trim_width_in * 2.0 - config.art_layout.spread_gutter_in).max(0.0)
        }
    };
    let height_in = config.book.trim_height_in * f64::from(layout.height_percent) / 100.0;
    let width_in = surface_width_in * envelope;
    let ppi = config.art_layout.pixels_per_inch;
    ArtGeometry {
        surface_width_in,
        height_in,
        width_in,
        aspect_ratio: width_in / height_in,
        width_px: (width_in * ppi).round() as u32,
        height_px: (height_in * ppi).round() as u32,
    }
}

pub fn requirements_for_story(
    root: &Path,
    config: &Config,
    story_id: &str,
) -> Result<BTreeMap<String, IllustrationRequirement>, AppError> {
    let base = config.state_dir(root).join("requirements");
    if !base.is_dir() {
        return Ok(BTreeMap::new());
    }
    let mut output = BTreeMap::new();
    for entry in fs::read_dir(base)?.filter_map(Result::ok) {
        let art_id = entry.file_name().to_string_lossy().to_string();
        if let Some(requirement) = storage::load_latest_requirement(root, config, &art_id)? {
            if requirement.story_id == story_id {
                output.insert(art_id, requirement);
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn computes_each_layout_envelope_and_spread_gutter() {
        let mut config = Config::default();
        config.book.trim_width_in = 8.0;
        config.book.trim_height_in = 10.0;
        config.art_layout.spread_gutter_in = 0.5;
        let cases = [
            (ArtSurface::SinglePage, ArtOrientation::Portrait, 5.2),
            (ArtSurface::SinglePage, ArtOrientation::Landscape, 8.0),
            (
                ArtSurface::DoublePageSpread,
                ArtOrientation::Portrait,
                6.975,
            ),
            (
                ArtSurface::DoublePageSpread,
                ArtOrientation::Landscape,
                15.5,
            ),
        ];
        for (surface, orientation, expected_width) in cases {
            let layout = ArtLayout {
                surface,
                orientation,
                height_percent: 50,
            };
            let result = geometry(&config, &layout);
            assert!((result.width_in - expected_width).abs() < 0.0001);
            assert_eq!(result.height_in, 5.0);
            assert_eq!(result.width_px, (expected_width * 300.0).round() as u32);
            assert_eq!(result.height_px, 1500);
            assert!((result.aspect_ratio - expected_width / 5.0).abs() < 0.0001);
        }
    }
}
