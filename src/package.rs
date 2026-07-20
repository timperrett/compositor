use crate::art_brief::{self, ArtUsage};
use crate::assets::{self, AssetRegistry, AssetStatus};
use crate::composition::{ArtReference, CompositionPlan};
use crate::config::Config;
use crate::flow::{SourceRef, StoryFlowPlan};
use crate::model::{ArtGeometry, ArtLayout, Severity, Story, ValidationIssue, ValidationReport};
use crate::AppError;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct PackagePolicy {
    pub minimum: AssetStatus,
    pub strict: bool,
}

struct AssetResolution<'a> {
    story: &'a Story,
    config: &'a Config,
    expected_usage: ArtUsage,
    expected_spread_id: Option<&'a str>,
    destination: &'a Path,
    policy: PackagePolicy,
}

#[derive(Debug, Serialize)]
struct SpreadManifest<'a> {
    schema: &'static str,
    id: &'a str,
    number: usize,
    role: &'a str,
    energy: u8,
    layout: &'a crate::composition::LayoutChoice,
    text: TextManifest,
    illustration: &'a crate::composition::IllustrationIntent,
    art: Vec<ArtManifest>,
}
#[derive(Debug, Serialize)]
struct TextManifest {
    file: &'static str,
    word_count: usize,
    density: String,
}
#[derive(Debug, Serialize)]
struct ArtManifest {
    id: String,
    role: String,
    status: String,
    source: Option<String>,
    file: Option<String>,
    resolved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    art_layout: Option<ArtLayout>,
    #[serde(skip_serializing_if = "Option::is_none")]
    geometry: Option<ArtGeometry>,
}

#[derive(Debug, Serialize)]
struct OpenerManifest<'a> {
    schema: &'static str,
    title: &'a str,
    placement: &'a crate::composition::OpenerPlacement,
    art: ArtManifest,
}

#[allow(clippy::too_many_arguments)]
pub fn build(
    root: &Path,
    config: &Config,
    story: &Story,
    flow: &StoryFlowPlan,
    composition: &CompositionPlan,
    registry: &AssetRegistry,
    output: &Path,
    policy: PackagePolicy,
) -> Result<ValidationReport, AppError> {
    let mut report = assets::validate(root, registry);
    if policy.strict && !report.can_proceed() {
        return Err(AppError::Validation);
    }
    let output_parent = output.parent().unwrap_or(root);
    fs::create_dir_all(output_parent)?;
    let temporary = tempfile::Builder::new()
        .prefix(".compositor-package-")
        .tempdir_in(output_parent)?;
    let package_root = temporary.path();
    let opener_directory = package_root.join("opener");
    fs::create_dir_all(opener_directory.join("art"))?;
    fs::write(
        opener_directory.join("title.txt"),
        format!("{}\n", composition.opener.title),
    )?;
    let opener_art = resolve_asset(
        root,
        registry,
        &composition.opener.art,
        AssetResolution {
            story,
            config,
            expected_usage: ArtUsage::Opener,
            expected_spread_id: None,
            destination: &opener_directory,
            policy,
        },
        &mut report,
    )?;
    fs::write(
        opener_directory.join("opener.yaml"),
        serde_yaml::to_string(&OpenerManifest {
            schema: "compositor.dev/story-opener/v1",
            title: &composition.opener.title,
            placement: &composition.opener.placement,
            art: opener_art,
        })
        .map_err(|error| AppError::serialization(error.to_string()))?,
    )?;
    let mut guide = format!(
        "<!doctype html><html><body><h1>Assembly guide</h1><section><h2>Story opener — {}</h2><p>center-page</p><p>opener</p></section>",
        composition.opener.title
    );
    let mut entries = Vec::new();
    for (index, flow_spread) in flow.spreads.iter().enumerate() {
        let composition_spread = composition
            .spreads
            .iter()
            .find(|spread| spread.id == flow_spread.id)
            .ok_or_else(|| {
                AppError::command(format!("missing composition for {}", flow_spread.id))
            })?;
        let directory = format!("spreads/{:03}-{}", index + 1, flow_spread.role);
        let spread_directory = package_root.join(&directory);
        fs::create_dir_all(spread_directory.join("art"))?;
        let paragraphs =
            source_paragraphs(story, &flow_spread.source.from, &flow_spread.source.through)?;
        let text = paragraphs
            .iter()
            .map(|paragraph| {
                format!(
                    "<!-- source: paragraph:{} -->\n\n{}",
                    paragraph.id.as_deref().unwrap_or("unknown"),
                    paragraph.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        fs::write(spread_directory.join("text.md"), &text)?;
        fs::write(
            spread_directory.join("text.txt"),
            render_spread_text(&paragraphs),
        )?;
        let word_count = paragraphs
            .iter()
            .map(|paragraph| paragraph.word_count)
            .sum();
        let art = composition_spread
            .art_assets
            .iter()
            .map(|asset| {
                resolve_asset(
                    root,
                    registry,
                    asset,
                    AssetResolution {
                        story,
                        config,
                        expected_usage: ArtUsage::Story,
                        expected_spread_id: Some(&flow_spread.id),
                        destination: &spread_directory,
                        policy,
                    },
                    &mut report,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let manifest = SpreadManifest {
            schema: "compositor.dev/resolved-spread/v2",
            id: &flow_spread.id,
            number: index + 1,
            role: &flow_spread.role,
            energy: flow_spread.energy,
            layout: &composition_spread.layout,
            text: TextManifest {
                file: "text.txt",
                word_count,
                density: composition_spread.text.density.clone(),
            },
            illustration: &composition_spread.illustration,
            art,
        };
        fs::write(
            spread_directory.join("spread.yaml"),
            serde_yaml::to_string(&manifest)
                .map_err(|error| AppError::serialization(error.to_string()))?,
        )?;
        guide.push_str(&format!(
            "<section><h2>Spread {} — {}</h2><p>{}</p><p>{}</p></section>",
            index + 1,
            flow_spread.role,
            composition_spread.layout.family,
            directory
        ));
        entries.push(serde_json::json!({"id": flow_spread.id, "directory": directory, "role": flow_spread.role}));
    }
    guide.push_str("</body></html>");
    fs::write(package_root.join("assembly-guide.html"), guide)?;
    fs::write(
        package_root.join("diagnostics.yaml"),
        serde_yaml::to_string(&report)
            .map_err(|error| AppError::serialization(error.to_string()))?,
    )?;
    let root_manifest = serde_json::json!({"schema":"compositor.dev/production-package/v1","story":{"id":story.id,"title":story.title,"source_revision":story.source_hash},"edition":composition.edition,"opener":{"directory":"opener","title":composition.opener.title,"placement":composition.opener.placement},"build":{"asset_policy":format!("{:?}",policy.minimum).to_lowercase(),"strict_art":policy.strict},"spreads":{"count":entries.len(),"entries":entries}});
    fs::write(
        package_root.join("manifest.yaml"),
        serde_yaml::to_string(&root_manifest)
            .map_err(|error| AppError::serialization(error.to_string()))?,
    )?;
    if policy.strict && !report.can_proceed() {
        return Err(AppError::Validation);
    }
    if output.exists() {
        fs::remove_dir_all(output)?;
    }
    fs::rename(package_root, output)?;
    Ok(report)
}

fn render_spread_text(paragraphs: &[&crate::model::SourceParagraph]) -> String {
    let text = paragraphs
        .iter()
        .map(|paragraph| crate::text::plain_text(&paragraph.content))
        .filter(|paragraph| !paragraph.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.is_empty() {
        String::new()
    } else {
        format!("{text}\n")
    }
}

fn source_paragraphs<'a>(
    story: &'a Story,
    from: &SourceRef,
    through: &SourceRef,
) -> Result<Vec<&'a crate::model::SourceParagraph>, AppError> {
    if from.kind != "paragraph" || through.kind != "paragraph" {
        return Err(AppError::command(
            "packages require paragraph source ranges".into(),
        ));
    }
    let start = story
        .paragraphs
        .iter()
        .position(|paragraph| paragraph.id.as_deref() == Some(&from.id))
        .ok_or_else(|| AppError::command(format!("unknown paragraph `{}`", from.id)))?;
    let end = story
        .paragraphs
        .iter()
        .position(|paragraph| paragraph.id.as_deref() == Some(&through.id))
        .ok_or_else(|| AppError::command(format!("unknown paragraph `{}`", through.id)))?;
    if end < start {
        return Err(AppError::command("reversed source range".into()));
    }
    Ok(story.paragraphs[start..=end].iter().collect())
}

fn resolve_asset(
    root: &Path,
    registry: &AssetRegistry,
    asset: &ArtReference,
    resolution: AssetResolution<'_>,
    report: &mut ValidationReport,
) -> Result<ArtManifest, AppError> {
    let Some(record) = assets::record(registry, &asset.id) else {
        return Ok(unresolved(
            asset,
            "ART_ASSET_UNKNOWN",
            "asset is not in the registry",
            report,
        ));
    };
    let Some(brief) = art_brief::load(root, &asset.id)? else {
        return Ok(unresolved(
            asset,
            "ART_BRIEF_MISSING",
            "asset has no art brief",
            report,
        ));
    };
    if brief.usage != resolution.expected_usage {
        return Ok(unresolved(
            asset,
            "ART_USAGE_MISMATCH",
            "art usage does not match this package location",
            report,
        ));
    }
    if let Some(spread_id) = resolution.expected_spread_id {
        if !brief.source.spread_ids.iter().any(|id| id == spread_id) {
            return Ok(unresolved(
                asset,
                "ART_SPREAD_LINK_MISSING",
                "story art is not linked to this narrative spread",
                report,
            ));
        }
    }
    if !assets::allowed(record.status, resolution.policy.minimum) {
        return Ok(unresolved(
            asset,
            "ART_STATUS_BELOW_POLICY",
            "asset status is below the build policy",
            report,
        ));
    }
    let Some(source) = record.file.as_deref() else {
        return Ok(unresolved(
            asset,
            "ART_FILE_MISSING",
            "asset has no file",
            report,
        ));
    };
    let source_path = root.join(source);
    if !source_path.is_file() {
        return Ok(unresolved(
            asset,
            "ART_FILE_MISSING",
            "asset source file is missing",
            report,
        ));
    }
    let extension = source_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("bin");
    let file = format!("art/{}.{}", asset.id, extension);
    fs::copy(&source_path, resolution.destination.join(&file))?;
    let (art_layout, geometry) =
        source_geometry(resolution.story, &brief.source.anchor_id, resolution.config)?;
    Ok(ArtManifest {
        id: asset.id.clone(),
        role: asset.role.clone(),
        status: format!("{:?}", record.status).to_lowercase(),
        source: Some(source.into()),
        file: Some(file),
        resolved: true,
        art_layout,
        geometry,
    })
}

fn unresolved(
    asset: &ArtReference,
    code: &str,
    message: &str,
    report: &mut ValidationReport,
) -> ArtManifest {
    report.issues.push(ValidationIssue {
        severity: Severity::Warning,
        code: code.into(),
        message: message.into(),
        path: "art/assets.yaml".into(),
        story_id: None,
        unit_id: Some(asset.id.clone()),
    });
    ArtManifest {
        id: asset.id.clone(),
        role: asset.role.clone(),
        status: "unresolved".into(),
        source: None,
        file: None,
        resolved: false,
        art_layout: None,
        geometry: None,
    }
}

fn source_geometry(
    story: &Story,
    anchor_id: &str,
    config: &Config,
) -> Result<(Option<ArtLayout>, Option<ArtGeometry>), AppError> {
    let art_layout = story
        .units
        .iter()
        .find(|unit| unit.directives.anchor.as_deref() == Some(anchor_id))
        .and_then(|unit| unit.directives.art_layout.clone());
    let geometry = if let Some(layout) = art_layout.as_ref() {
        crate::art::validate_layout(config, layout).map_err(AppError::command)?;
        Some(crate::art::geometry(config, layout))
    } else {
        None
    };
    Ok((art_layout, geometry))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetRecord;
    use crate::flow;
    use std::fs;

    #[test]
    fn creates_a_missing_parent_directory_for_package_output() {
        let directory = tempfile::tempdir().unwrap();
        let story_path = directory.path().join("story.md");
        fs::write(
            &story_path,
            "---\nid: story\ntitle: Story\n---\n<!-- anchor: story-opening -->\n<!-- paragraph: opening -->\n\nOnce **upon** a [time](https://example.com).\n",
        )
        .unwrap();
        let story = flow::load_story(&story_path).unwrap();
        let flow_plan: StoryFlowPlan = serde_yaml::from_str(&format!(
            "schema: compositor.dev/story-flow/v1\nstory:\n  id: story\n  source_revision: {}\nspreads:\n  - id: spread-001\n    source:\n      from: {{ type: paragraph, id: opening }}\n      through: {{ type: paragraph, id: opening }}\n    role: opening\n    energy: 1\n    narrative: {{ purpose: Open the story. }}\n",
            story.source_hash
        ))
        .unwrap();
        let composition: CompositionPlan = serde_yaml::from_str(
            "schema: compositor.dev/composition-plan/v2\nstory:\n  id: story\n  flow: story.flow.yaml\nedition:\n  id: first-edition\n  design_system: example\nopener:\n  title: Story\n  placement: center-page\n  art: { id: story-opener, role: primary-subject }\nspreads:\n  - id: spread-001\n    layout: { family: text, variant: standard }\n    text: { density: standard }\n    illustration: { mode: none, focal_subject: none }\n",
        )
        .unwrap();
        let output = directory.path().join("delivery/first-edition/package");

        build(
            directory.path(),
            &Config::default(),
            &story,
            &flow_plan,
            &composition,
            &AssetRegistry {
                schema: assets::ASSET_REGISTRY_SCHEMA.into(),
                assets: Vec::new(),
            },
            &output,
            PackagePolicy {
                minimum: AssetStatus::Draft,
                strict: false,
            },
        )
        .unwrap();

        assert!(output.join("assembly-guide.html").is_file());
        assert_eq!(
            fs::read_to_string(output.join("opener/title.txt")).unwrap(),
            "Story\n"
        );
        assert!(output.join("spreads/001-opening/text.md").is_file());
        assert_eq!(
            fs::read_to_string(output.join("spreads/001-opening/text.txt")).unwrap(),
            "Once upon a time.\n"
        );
        let manifest = fs::read_to_string(output.join("spreads/001-opening/spread.yaml")).unwrap();
        assert!(manifest.contains("file: text.txt"));
    }

    #[test]
    fn leaves_unlinked_story_art_unresolved() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("art/briefs")).unwrap();
        let story_path = directory.path().join("story.md");
        fs::write(
            &story_path,
            "---\nid: story\ntitle: Story\n---\n<!-- anchor: scene -->\n<!-- paragraph: opening -->\n\nOnce upon a time.\n",
        )
        .unwrap();
        let story = flow::load_story(&story_path).unwrap();
        fs::write(
            directory.path().join("art/briefs/story-art.yaml"),
            "schema_version: 2\nart_id: story-art\nsource: { story_id: story, anchor_id: scene }\ngeneration: { page_treatment: floating, prompt: A scene. }\n",
        )
        .unwrap();
        let registry = AssetRegistry {
            schema: crate::assets::ASSET_REGISTRY_SCHEMA.into(),
            assets: vec![AssetRecord {
                id: "story-art".into(),
                brief: "art/briefs/story-art.yaml".into(),
                status: AssetStatus::Draft,
                file: None,
                superseded_by: None,
            }],
        };
        let mut report = ValidationReport::default();
        let manifest = resolve_asset(
            directory.path(),
            &registry,
            &ArtReference {
                id: "story-art".into(),
                role: "primary-subject".into(),
            },
            AssetResolution {
                story: &story,
                config: &Config::default(),
                expected_usage: ArtUsage::Story,
                expected_spread_id: Some("spread-001"),
                destination: directory.path(),
                policy: PackagePolicy {
                    minimum: AssetStatus::Draft,
                    strict: false,
                },
            },
            &mut report,
        )
        .unwrap();
        assert!(!manifest.resolved);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "ART_SPREAD_LINK_MISSING"));
    }

    #[test]
    fn emits_source_derived_geometry_for_layout_controlled_art() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("art/briefs")).unwrap();
        fs::create_dir_all(directory.path().join("assets/drafts")).unwrap();
        let story_path = directory.path().join("story.md");
        fs::write(
            &story_path,
            "---\nid: story\ntitle: Story\n---\n<!-- anchor: scene -->\n<!-- art-layout: surface=double-page-spread orientation=landscape height=25% -->\n<!-- paragraph: opening -->\n\nOnce upon a time.\n",
        )
        .unwrap();
        let story = flow::load_story(&story_path).unwrap();
        fs::write(
            directory.path().join("art/briefs/story-art.yaml"),
            "schema_version: 2\nart_id: story-art\nsource: { story_id: story, anchor_id: scene, spread_ids: [spread-001] }\ngeneration: { page_treatment: framed, prompt: A scene. }\n",
        )
        .unwrap();
        fs::write(
            directory.path().join("assets/drafts/story-art.png"),
            "placeholder",
        )
        .unwrap();
        let registry = AssetRegistry {
            schema: crate::assets::ASSET_REGISTRY_SCHEMA.into(),
            assets: vec![AssetRecord {
                id: "story-art".into(),
                brief: "art/briefs/story-art.yaml".into(),
                status: AssetStatus::Draft,
                file: Some("assets/drafts/story-art.png".into()),
                superseded_by: None,
            }],
        };
        let flow_plan: StoryFlowPlan = serde_yaml::from_str(&format!(
            "schema: compositor.dev/story-flow/v1\nstory:\n  id: story\n  source_revision: {}\nspreads:\n  - id: spread-001\n    source:\n      from: {{ type: paragraph, id: opening }}\n      through: {{ type: paragraph, id: opening }}\n    role: opening\n    energy: 1\n    narrative: {{ purpose: Open the story. }}\n",
            story.source_hash
        ))
        .unwrap();
        let composition: CompositionPlan = serde_yaml::from_str(
            "schema: compositor.dev/composition-plan/v2\nstory:\n  id: story\n  flow: story.flow.yaml\nedition:\n  id: first-edition\n  design_system: example\nopener:\n  title: Story\n  placement: center-page\n  art: { id: story-opener, role: primary-subject }\nspreads:\n  - id: spread-001\n    layout: { family: text, variant: standard }\n    text: { density: standard }\n    illustration: { mode: none, focal_subject: none }\n    art_assets: [{ id: story-art, role: primary-subject }]\n",
        )
        .unwrap();
        let output = directory.path().join("package");

        build(
            directory.path(),
            &Config::default(),
            &story,
            &flow_plan,
            &composition,
            &registry,
            &output,
            PackagePolicy {
                minimum: AssetStatus::Draft,
                strict: false,
            },
        )
        .unwrap();

        let manifest = fs::read_to_string(output.join("spreads/001-opening/spread.yaml")).unwrap();
        assert!(manifest.contains("schema: compositor.dev/resolved-spread/v2"));
        assert!(manifest.contains("height_percent: 25"));
        assert!(manifest.contains("width_in: 16.0"));
        assert!(manifest.contains("height_in: 2.5"));
        assert!(manifest.contains("aspect_ratio: 6.4"));
        assert!(manifest.contains("width_px: 4800"));
        assert!(manifest.contains("height_px: 750"));
    }
}
