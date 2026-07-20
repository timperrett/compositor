use crate::identity::ResolvedStory;
use crate::model::{
    ArtGeometry, ArtLayout, ArtOrientation, ArtSurface, ArtifactStatus, IllustrationRequirement,
    PageLayout, PagePlan, Story, SCHEMA_VERSION,
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
            .map(|assignment| assignment.layout)
            .unwrap_or(PageLayout::TextDominant);
        let geometry = if let Some(art_layout) = unit.directives.art_layout.as_ref() {
            validate_layout(config, art_layout).map_err(AppError::command)?;
            Some(geometry(config, art_layout))
        } else {
            None
        };
        let previous = storage::load_latest_requirement(root, config, id)?;
        if let Some(previous) = previous.as_ref() {
            if previous.story_id == story.id
                && previous.unit_ids == [id.clone()]
                && previous.pages == pages
                && previous.layout == layout
                && previous.art_note == unit.directives.art
                && previous.art_layout == unit.directives.art_layout
                && previous.geometry == geometry
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
            geometry,
            art_note: unit.directives.art.clone(),
        };
        storage::save_requirement(root, config, &record)?;
        requirements.push(record);
    }
    Ok(requirements)
}

pub fn geometry(config: &Config, layout: &ArtLayout) -> ArtGeometry {
    let surface_width_in = match layout.surface {
        ArtSurface::SinglePage => config.book.trim_width_in,
        ArtSurface::DoublePageSpread => {
            (config.book.trim_width_in * 2.0 - config.art_layout.spread_gutter_in).max(0.0)
        }
    };
    let height_in = config.book.trim_height_in * f64::from(layout.height_percent) / 100.0;
    let portrait_aspect_ratio = config.book.trim_width_in.min(config.book.trim_height_in)
        / config.book.trim_width_in.max(config.book.trim_height_in);
    let width_in = match layout.orientation {
        ArtOrientation::Portrait => height_in * portrait_aspect_ratio,
        ArtOrientation::Landscape => surface_width_in,
    };
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

pub fn validate_layout(config: &Config, layout: &ArtLayout) -> Result<(), String> {
    let geometry = geometry(config, layout);
    match layout.orientation {
        ArtOrientation::Portrait if geometry.aspect_ratio >= 1.0 => Err(format!(
            "portrait art-layout derives {:.3}:1; portrait trim geometry must derive below 1:1",
            geometry.aspect_ratio
        )),
        ArtOrientation::Landscape
            if geometry.aspect_ratio < config.art_layout.minimum_landscape_aspect_ratio =>
        {
            let maximum_height_percent = ((geometry.surface_width_in
                / (config.book.trim_height_in * config.art_layout.minimum_landscape_aspect_ratio))
                * 100.0)
                .floor()
                .clamp(0.0, 100.0) as u8;
            Err(format!(
                "landscape art-layout at height={}%, on this {} surface, derives {:.3}:1; use height at most {}% or choose portrait",
                layout.height_percent,
                match layout.surface {
                    ArtSurface::SinglePage => "single-page",
                    ArtSurface::DoublePageSpread => "double-page-spread",
                },
                geometry.aspect_ratio,
                maximum_height_percent,
            ))
        }
        _ => Ok(()),
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
    fn derives_real_portrait_and_full_width_landscape_geometry() {
        let mut config = Config::default();
        config.book.trim_width_in = 8.0;
        config.book.trim_height_in = 10.0;
        config.art_layout.spread_gutter_in = 0.5;
        let portrait = geometry(
            &config,
            &ArtLayout {
                surface: ArtSurface::SinglePage,
                orientation: ArtOrientation::Portrait,
                height_percent: 100,
            },
        );
        assert_eq!(portrait.width_in, 8.0);
        assert_eq!(portrait.height_in, 10.0);
        assert_eq!(portrait.width_px, 2400);
        assert_eq!(portrait.height_px, 3000);
        assert!((portrait.aspect_ratio - 0.8).abs() < 0.0001);

        let spread = geometry(
            &config,
            &ArtLayout {
                surface: ArtSurface::DoublePageSpread,
                orientation: ArtOrientation::Landscape,
                height_percent: 55,
            },
        );
        assert_eq!(spread.width_in, 15.5);
        assert_eq!(spread.height_in, 5.5);
        assert!((spread.aspect_ratio - (15.5 / 5.5)).abs() < 0.0001);
    }

    #[test]
    fn rejects_landscape_that_is_not_landscape_enough() {
        let config = Config::default();
        let error = validate_layout(
            &config,
            &ArtLayout {
                surface: ArtSurface::SinglePage,
                orientation: ArtOrientation::Landscape,
                height_percent: 100,
            },
        )
        .unwrap_err();
        assert!(error.contains("height at most 60%"));
    }
}
