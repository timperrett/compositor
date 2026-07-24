use crate::art_brief::{self, ArtUsage};
use crate::assets::{self, AssetRegistry, AssetStatus};
use crate::composition::{
    ArtReference, CompositionPlan, Edition, IllustrationIntent, LayoutChoice, OpenerPlacement,
};
use crate::config::{Config, ParagraphEconomyConfig};
use crate::discovery::discover;
use crate::flow::{SourceRef, StoryFlowPlan};
use crate::model::{ArtGeometry, ArtLayout, Severity, Story, ValidationIssue, ValidationReport};
use crate::AppError;
use serde::{Deserialize, Serialize};
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
    paragraph_economy: ParagraphEconomyMetrics,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParagraphEconomyMetrics {
    pub paragraph_count: usize,
    pub short_paragraph_count: usize,
    pub longest_short_paragraph_run: usize,
    pub paragraphs_per_100_words: f64,
    pub status: ParagraphEconomyStatus,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParagraphEconomyStatus {
    Ok,
    Warning,
    Waived,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    replace: bool,
    policy: PackagePolicy,
) -> Result<ValidationReport, AppError> {
    let registry_report = assets::validate(root, registry);
    if policy.strict && !registry_report.can_proceed() {
        return Err(AppError::Validation);
    }
    // Non-strict packages remain story-scoped: only their resolved records are
    // hard gates. `art validate` remains the complete shared-registry audit.
    let mut report = ValidationReport::default();
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
    let opener_summary = (opener_art.id.clone(), opener_art.status.clone());
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
        "<!doctype html><html><body><h1>Assembly guide</h1><section><h2>Story opener — {}</h2><p>{:?}</p><p>art: {} ({})</p></section>",
        escape_html(&composition.opener.title),
        composition.opener.placement,
        escape_html(&opener_summary.0),
        escape_html(&opener_summary.1),
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
        let text = render_spread_markdown(&paragraphs);
        fs::write(spread_directory.join("text.md"), &text)?;
        fs::write(
            spread_directory.join("text.txt"),
            render_spread_text(&paragraphs),
        )?;
        let word_count = paragraphs
            .iter()
            .map(|paragraph| paragraph.word_count)
            .sum::<usize>();
        let waiver = paragraph_economy_waiver(flow, &flow_spread.id);
        let paragraph_economy = paragraph_economy_metrics(
            &paragraphs,
            config.editorial.paragraph_economy.as_ref(),
            waiver,
        );
        if paragraph_economy.status == ParagraphEconomyStatus::Warning {
            package_issue(
                &mut report,
                Severity::Warning,
                "PARAGRAPH_ECONOMY_FRAGMENTED",
                format!(
                    "spread has {:.1} paragraphs per 100 words and a run of {} short paragraphs",
                    paragraph_economy.paragraphs_per_100_words,
                    paragraph_economy.longest_short_paragraph_run
                ),
                &directory,
                Some(&story.id),
                Some(&flow_spread.id),
            );
        }
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
        let art_summary = art
            .iter()
            .map(|asset| format!("{}: {}", asset.id, asset.status))
            .collect::<Vec<_>>()
            .join(", ");
        let manifest = SpreadManifest {
            schema: "compositor.dev/resolved-spread/v3",
            id: &flow_spread.id,
            number: index + 1,
            role: &flow_spread.role,
            energy: flow_spread.energy,
            layout: &composition_spread.layout,
            text: TextManifest {
                file: "text.txt",
                word_count,
                density: composition_spread.text.density.clone(),
                paragraph_economy,
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
            "<section><h2>Spread {} — {}</h2><p>id: {}</p><p>layout: {}/{}</p><p>art: {}</p><pre>{}</pre></section>",
            index + 1,
            escape_html(&flow_spread.role),
            escape_html(&flow_spread.id),
            escape_html(&composition_spread.layout.family),
            escape_html(&composition_spread.layout.variant),
            escape_html(&art_summary),
            escape_html(&text),
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
    if !report.can_proceed() {
        return Err(AppError::Validation);
    }
    if output.exists() && !replace {
        return Err(AppError::command(format!(
            "package output already exists: {}; use --replace with an explicit --output to replace it",
            output.display()
        )));
    }
    if output.exists() {
        fs::remove_dir_all(output)?;
    }
    fs::rename(package_root, output)?;
    Ok(report)
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[derive(Debug, Serialize)]
pub struct PackageValidationOutput {
    pub package: String,
    pub story: String,
    pub source_revision: String,
    pub checked_spreads: usize,
}

#[derive(Debug, Deserialize)]
struct PackageRootManifest {
    story: PackageStoryManifest,
    edition: Edition,
    opener: PackageOpenerEntry,
    build: PackageBuildManifest,
    spreads: PackageSpreadsManifest,
}
#[derive(Debug, Deserialize)]
struct PackageBuildManifest {
    asset_policy: String,
    #[serde(rename = "strict_art")]
    _strict_art: bool,
}
#[derive(Debug, Deserialize)]
struct PackageStoryManifest {
    id: String,
    title: String,
    source_revision: String,
}
#[derive(Debug, Deserialize)]
struct PackageOpenerEntry {
    directory: String,
    title: String,
    placement: OpenerPlacement,
}
#[derive(Debug, Deserialize)]
struct PackageOpenerManifest {
    schema: String,
    title: String,
    placement: OpenerPlacement,
    art: ArtManifest,
}
#[derive(Debug, Deserialize)]
struct PackageSpreadsManifest {
    count: usize,
    entries: Vec<PackageSpreadEntry>,
}
#[derive(Debug, Deserialize)]
struct PackageSpreadEntry {
    directory: String,
    id: String,
    role: String,
}
#[derive(Debug, Deserialize)]
struct PackageSpreadManifest {
    schema: String,
    id: String,
    number: usize,
    role: String,
    layout: LayoutChoice,
    text: PackageTextManifest,
    illustration: IllustrationIntent,
    art: Vec<ArtManifest>,
}
#[derive(Debug, Deserialize)]
struct PackageTextManifest {
    file: String,
    word_count: usize,
    density: String,
    #[serde(default)]
    paragraph_economy: Option<ParagraphEconomyMetrics>,
}

/// Verifies that a package was rendered from the current source and plans.
/// This function is deliberately read-only: package generation remains the
/// only operation that writes package artifacts.
pub fn validate_package(
    root: &Path,
    config: &Config,
    package: &Path,
) -> Result<(PackageValidationOutput, ValidationReport), AppError> {
    let package = if package.is_absolute() {
        package.to_path_buf()
    } else {
        root.join(package)
    };
    let manifest_path = package.join("manifest.yaml");
    let root_manifest: PackageRootManifest = load_yaml(&manifest_path)?;
    let project = discover(root, config)?;
    let story = project
        .compendiums
        .iter()
        .flat_map(|compendium| &compendium.stories)
        .find(|story| story.id == root_manifest.story.id)
        .ok_or_else(|| {
            AppError::command(format!(
                "package story `{}` is not in the current project",
                root_manifest.story.id
            ))
        })?;
    let story_directory = root
        .join(&story.source)
        .parent()
        .ok_or_else(|| AppError::command("story source has no parent directory".into()))?
        .to_path_buf();
    let flow: StoryFlowPlan = load_yaml(&story_directory.join("story.flow.yaml"))?;
    let composition: CompositionPlan =
        load_yaml(&story_directory.join(format!("{}.composition.yaml", root_manifest.edition.id)))?;
    let mut report = ValidationReport::default();
    let policy = match root_manifest.build.asset_policy.as_str() {
        "draft" => AssetStatus::Draft,
        "review" => AssetStatus::Review,
        "approved" => AssetStatus::Approved,
        _ => {
            package_issue(
                &mut report,
                Severity::Error,
                "PACKAGE_ASSET_POLICY_INVALID",
                "package manifest has an unsupported asset policy".into(),
                "manifest.yaml",
                Some(&story.id),
                None,
            );
            AssetStatus::Approved
        }
    };
    let registry = match assets::load(root)? {
        Some(registry) => registry,
        None => {
            package_issue(
                &mut report,
                Severity::Error,
                "PACKAGE_ART_REGISTRY_MISSING",
                "current project has no art registry".into(),
                "art/assets.yaml",
                Some(&story.id),
                None,
            );
            AssetRegistry {
                schema: assets::ASSET_REGISTRY_SCHEMA.into(),
                assets: Vec::new(),
            }
        }
    };
    if root_manifest.story.title != story.title || root_manifest.story.id != story.id {
        package_issue(
            &mut report,
            Severity::Error,
            "PACKAGE_STORY_STALE",
            "package story metadata does not match the current manuscript".into(),
            "manifest.yaml",
            Some(&story.id),
            None,
        );
    }
    if root_manifest.story.source_revision != story.source_hash {
        package_issue(
            &mut report,
            Severity::Error,
            "PACKAGE_SOURCE_STALE",
            "package source revision does not match the current manuscript".into(),
            "manifest.yaml",
            Some(&story.id),
            None,
        );
    }
    if flow.story.source_revision != story.source_hash {
        package_issue(
            &mut report,
            Severity::Error,
            "PACKAGE_FLOW_STALE",
            "current Flow Plan source revision does not match the manuscript".into(),
            "story.flow.yaml",
            Some(&story.id),
            None,
        );
    }
    if composition.story.id != story.id || composition.edition != root_manifest.edition {
        package_issue(
            &mut report,
            Severity::Error,
            "PACKAGE_COMPOSITION_STALE",
            "package edition metadata does not match the current Composition Plan".into(),
            "manifest.yaml",
            Some(&story.id),
            None,
        );
    }
    let expected_opener_title = format!("{}\n", composition.opener.title);
    let opener_manifest: Result<PackageOpenerManifest, AppError> =
        load_yaml(&package.join("opener/opener.yaml"));
    if root_manifest.opener.directory != "opener"
        || root_manifest.opener.title != composition.opener.title
        || root_manifest.opener.placement != composition.opener.placement
        || !matches!(
            opener_manifest,
            Ok(PackageOpenerManifest {
                schema,
                title,
                placement,
                ..
            }) if schema == "compositor.dev/story-opener/v1"
                && title == composition.opener.title
                && placement == composition.opener.placement
        )
        || fs::read_to_string(package.join("opener/title.txt"))
            .map(|title| title != expected_opener_title)
            .unwrap_or(true)
    {
        package_issue(
            &mut report,
            Severity::Error,
            "PACKAGE_COMPOSITION_STALE",
            "package opener metadata does not match the current Composition Plan".into(),
            "opener",
            Some(&story.id),
            None,
        );
    }
    if let Ok(opener_manifest) =
        load_yaml::<PackageOpenerManifest>(&package.join("opener/opener.yaml"))
    {
        validate_package_art(
            root,
            config,
            story,
            &registry,
            std::slice::from_ref(&composition.opener.art),
            std::slice::from_ref(&opener_manifest.art),
            ArtUsage::Opener,
            None,
            policy,
            &package.join("opener"),
            "opener",
            &mut report,
        );
    }
    if root_manifest.spreads.count != flow.spreads.len()
        || root_manifest.spreads.entries.len() != flow.spreads.len()
    {
        package_issue(
            &mut report,
            Severity::Error,
            "PACKAGE_FLOW_STALE",
            "package spread count does not match the current Flow Plan".into(),
            "manifest.yaml",
            Some(&story.id),
            None,
        );
    }

    for (index, flow_spread) in flow.spreads.iter().enumerate() {
        let expected_directory = format!("spreads/{:03}-{}", index + 1, flow_spread.role);
        let entry = root_manifest.spreads.entries.get(index);
        if !entry.is_some_and(|entry| {
            entry.id == flow_spread.id
                && entry.role == flow_spread.role
                && entry.directory == expected_directory
        }) {
            package_issue(
                &mut report,
                Severity::Error,
                "PACKAGE_FLOW_STALE",
                "package spread entry does not match the current Flow Plan".into(),
                "manifest.yaml",
                Some(&story.id),
                Some(&flow_spread.id),
            );
            continue;
        }
        let Some(composition_spread) = composition
            .spreads
            .iter()
            .find(|spread| spread.id == flow_spread.id)
        else {
            package_issue(
                &mut report,
                Severity::Error,
                "PACKAGE_COMPOSITION_STALE",
                "current Composition Plan has no matching spread".into(),
                "hardcover.composition.yaml",
                Some(&story.id),
                Some(&flow_spread.id),
            );
            continue;
        };
        let spread_directory = package.join(&expected_directory);
        let spread_manifest_path = spread_directory.join("spread.yaml");
        let actual: PackageSpreadManifest = match load_yaml(&spread_manifest_path) {
            Ok(value) => value,
            Err(_) => {
                package_issue(
                    &mut report,
                    Severity::Error,
                    "PACKAGE_SPREAD_MISSING",
                    "package spread manifest is missing or unreadable".into(),
                    &expected_directory,
                    Some(&story.id),
                    Some(&flow_spread.id),
                );
                continue;
            }
        };
        let paragraphs =
            source_paragraphs(story, &flow_spread.source.from, &flow_spread.source.through)?;
        let word_count = paragraphs
            .iter()
            .map(|paragraph| paragraph.word_count)
            .sum::<usize>();
        let metrics = paragraph_economy_metrics(
            &paragraphs,
            config.editorial.paragraph_economy.as_ref(),
            paragraph_economy_waiver(&flow, &flow_spread.id),
        );
        if actual.schema != "compositor.dev/resolved-spread/v2"
            && actual.schema != "compositor.dev/resolved-spread/v3"
            || actual.id != flow_spread.id
            || actual.number != index + 1
            || actual.role != flow_spread.role
            || actual.layout != composition_spread.layout
            || actual.illustration != composition_spread.illustration
            || actual.text.file != "text.txt"
            || actual.text.word_count != word_count
            || actual.text.density != composition_spread.text.density
        {
            package_issue(
                &mut report,
                Severity::Error,
                "PACKAGE_COMPOSITION_STALE",
                "package spread metadata does not match the current Flow or Composition Plan"
                    .into(),
                &expected_directory,
                Some(&story.id),
                Some(&flow_spread.id),
            );
        }
        if actual.schema == "compositor.dev/resolved-spread/v3"
            && actual.text.paragraph_economy.as_ref() != Some(&metrics)
        {
            package_issue(
                &mut report,
                Severity::Error,
                "PACKAGE_TEXT_STALE",
                "package paragraph-economy metrics do not match the current source".into(),
                &expected_directory,
                Some(&story.id),
                Some(&flow_spread.id),
            );
        }
        validate_package_art(
            root,
            config,
            story,
            &registry,
            &composition_spread.art_assets,
            &actual.art,
            ArtUsage::Story,
            Some(&flow_spread.id),
            policy,
            &spread_directory,
            &expected_directory,
            &mut report,
        );
        compare_package_text(
            &mut report,
            &spread_directory.join("text.md"),
            &render_spread_markdown(&paragraphs),
            &expected_directory,
            story,
            &flow_spread.id,
        );
        compare_package_text(
            &mut report,
            &spread_directory.join("text.txt"),
            &render_spread_text(&paragraphs),
            &expected_directory,
            story,
            &flow_spread.id,
        );
        if metrics.status == ParagraphEconomyStatus::Warning {
            package_issue(
                &mut report,
                Severity::Warning,
                "PARAGRAPH_ECONOMY_FRAGMENTED",
                format!(
                    "spread has {:.1} paragraphs per 100 words and a run of {} short paragraphs",
                    metrics.paragraphs_per_100_words, metrics.longest_short_paragraph_run
                ),
                &expected_directory,
                Some(&story.id),
                Some(&flow_spread.id),
            );
        }
    }
    Ok((
        PackageValidationOutput {
            package: package.display().to_string(),
            story: story.id.clone(),
            source_revision: story.source_hash.clone(),
            checked_spreads: flow.spreads.len(),
        },
        report,
    ))
}

fn load_yaml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, AppError> {
    let text = fs::read_to_string(path)?;
    serde_yaml::from_str(&text).map_err(|error| {
        AppError::serialization(format!("could not parse {}: {error}", path.display()))
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_package_art(
    root: &Path,
    config: &Config,
    story: &Story,
    registry: &AssetRegistry,
    expected: &[ArtReference],
    packaged: &[ArtManifest],
    usage: ArtUsage,
    spread_id: Option<&str>,
    policy: AssetStatus,
    directory: &Path,
    package_path: &str,
    report: &mut ValidationReport,
) {
    if expected.len() != packaged.len() {
        package_issue(
            report,
            Severity::Error,
            "PACKAGE_ART_STALE",
            "package art entries do not match the current Composition Plan".into(),
            package_path,
            Some(&story.id),
            spread_id,
        );
    }
    for (reference, actual) in expected.iter().zip(packaged) {
        let invalid = |message: &str, report: &mut ValidationReport| {
            package_issue(
                report,
                Severity::Error,
                "PACKAGE_ART_STALE",
                message.into(),
                package_path,
                Some(&story.id),
                spread_id,
            )
        };
        if actual.id != reference.id || actual.role != reference.role || !actual.resolved {
            invalid(
                "package art entry does not match current art reference",
                report,
            );
            continue;
        }
        let Some(record) = assets::record(registry, &reference.id) else {
            invalid("current registry no longer contains package art", report);
            continue;
        };
        let record_validation = assets::validate_record(root, record);
        if !record_validation.can_proceed() {
            report.issues.extend(record_validation.issues);
            invalid("current package art registry record is invalid", report);
            continue;
        }
        let Ok(Some(brief)) = art_brief::load(root, &reference.id) else {
            invalid("current package art brief is missing or invalid", report);
            continue;
        };
        if brief.usage != usage
            || spread_id
                .is_some_and(|spread| !brief.source.spread_ids.iter().any(|id| id == spread))
            || !assets::allowed(record.status, policy)
        {
            invalid(
                "current art lifecycle or Flow/Composition mapping has changed",
                report,
            );
            continue;
        }
        let pinned = match record.status {
            AssetStatus::Approved => record
                .approved
                .as_ref()
                .map(|value| (&value.file, &value.sha256)),
            AssetStatus::Draft | AssetStatus::Review => record
                .selection
                .as_ref()
                .map(|value| (&value.file, &value.sha256)),
            _ => None,
        };
        let Some((source, hash)) = pinned else {
            invalid("current package art has no pinned file", report);
            continue;
        };
        let extension = Path::new(source)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("bin");
        let expected_file = format!("art/{}.{}", reference.id, extension);
        let (layout, geometry) = match source_geometry(story, &brief.source.anchor_id, config) {
            Ok(value) => value,
            Err(_) => {
                invalid("current art geometry is invalid", report);
                continue;
            }
        };
        if let Some(expected_geometry) = geometry.as_ref() {
            match art_brief::candidate_geometry(&root.join(source)) {
                Ok(actual_geometry)
                    if art_brief::geometry_matches(expected_geometry, &actual_geometry) => {}
                _ => {
                    invalid(
                        "current package art no longer matches its required geometry",
                        report,
                    );
                    continue;
                }
            }
        }
        let package_file_ok = actual.status == format!("{:?}", record.status).to_lowercase()
            && actual.source.as_deref() == Some(source)
            && actual.file.as_deref() == Some(&expected_file)
            && actual.art_layout == layout
            && actual.geometry == geometry
            && directory.join(&expected_file).is_file()
            && assets::sha256_path(&directory.join(&expected_file))
                .map(|value| value == *hash)
                .unwrap_or(false);
        if !package_file_ok {
            invalid("package art file or resolved metadata is stale", report);
        }
    }
}

fn compare_package_text(
    report: &mut ValidationReport,
    path: &Path,
    expected: &str,
    package_path: &str,
    story: &Story,
    spread_id: &str,
) {
    match fs::read_to_string(path) {
        Ok(actual) if actual == expected => {}
        Ok(_) | Err(_) => package_issue(
            report,
            Severity::Error,
            "PACKAGE_TEXT_STALE",
            "package text does not match the current source paragraphs".into(),
            package_path,
            Some(&story.id),
            Some(spread_id),
        ),
    }
}

fn render_spread_markdown(paragraphs: &[&crate::model::SourceParagraph]) -> String {
    paragraphs
        .iter()
        .map(|paragraph| {
            format!(
                "<!-- source: paragraph:{} -->\n\n{}",
                paragraph.id.as_deref().unwrap_or("unknown"),
                paragraph.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn paragraph_economy_metrics(
    paragraphs: &[&crate::model::SourceParagraph],
    config: Option<&ParagraphEconomyConfig>,
    waived: bool,
) -> ParagraphEconomyMetrics {
    let paragraph_count = paragraphs.len();
    let word_count = paragraphs
        .iter()
        .map(|paragraph| paragraph.word_count)
        .sum::<usize>();
    let short_limit = config
        .map(|value| value.short_paragraph_max_words)
        .unwrap_or(0);
    let mut short_paragraph_count = 0;
    let mut longest_short_paragraph_run = 0;
    let mut current_short_paragraph_run = 0;
    for paragraph in paragraphs {
        if short_limit > 0 && paragraph.word_count <= short_limit {
            short_paragraph_count += 1;
            current_short_paragraph_run += 1;
            longest_short_paragraph_run =
                longest_short_paragraph_run.max(current_short_paragraph_run);
        } else {
            current_short_paragraph_run = 0;
        }
    }
    let paragraphs_per_100_words = if word_count == 0 {
        0.0
    } else {
        paragraph_count as f64 * 100.0 / word_count as f64
    };
    let fragmented = config.is_some_and(|value| {
        word_count >= value.minimum_words
            && paragraphs_per_100_words > value.max_paragraphs_per_100_words
            && longest_short_paragraph_run >= value.max_consecutive_short_paragraphs
    });
    ParagraphEconomyMetrics {
        paragraph_count,
        short_paragraph_count,
        longest_short_paragraph_run,
        paragraphs_per_100_words,
        status: if fragmented && waived {
            ParagraphEconomyStatus::Waived
        } else if fragmented {
            ParagraphEconomyStatus::Warning
        } else {
            ParagraphEconomyStatus::Ok
        },
    }
}

fn paragraph_economy_waiver(flow: &StoryFlowPlan, spread_id: &str) -> bool {
    flow.notes.iter().any(|note| {
        note.code == "INTENTIONAL_PARAGRAPH_FRAGMENTATION"
            && note.severity.eq_ignore_ascii_case("info")
            && note.spread == spread_id
            && !note.message.trim().is_empty()
    })
}

fn package_issue(
    report: &mut ValidationReport,
    severity: Severity,
    code: &str,
    message: String,
    path: impl AsRef<Path>,
    story_id: Option<&str>,
    spread_id: Option<&str>,
) {
    report.issues.push(ValidationIssue {
        severity,
        code: code.into(),
        message,
        path: path.as_ref().display().to_string(),
        story_id: story_id.map(str::to_owned),
        unit_id: spread_id.map(str::to_owned),
    });
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
    let record_validation = assets::validate_record(root, record);
    if !record_validation.can_proceed() {
        report.issues.extend(record_validation.issues);
        return Ok(unresolved(
            asset,
            "ART_RECORD_INVALID",
            "registry record is not consistent with its brief or pinned file",
            report,
        ));
    }
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
    let source = match record.status {
        AssetStatus::Approved => record
            .approved
            .as_ref()
            .map(|asset| (asset.file.as_str(), asset.sha256.as_str())),
        AssetStatus::Draft | AssetStatus::Review => record
            .selection
            .as_ref()
            .map(|asset| (asset.file.as_str(), asset.sha256.as_str())),
        _ => None,
    };
    let Some((source, expected_hash)) = source else {
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
    if assets::sha256(root, source)? != expected_hash {
        return Ok(unresolved(
            asset,
            "ART_HASH_MISMATCH",
            "asset source no longer matches its pinned SHA-256",
            report,
        ));
    }
    let (art_layout, geometry) =
        source_geometry(resolution.story, &brief.source.anchor_id, resolution.config)?;
    if let Some(expected) = geometry.as_ref() {
        let geometry_matches = art_brief::candidate_geometry(&source_path)
            .map(|actual| art_brief::geometry_matches(expected, &actual))
            .unwrap_or(false);
        if !geometry_matches {
            return Ok(unresolved(
                asset,
                "ART_GEOMETRY_MISMATCH",
                "asset source no longer matches current art geometry",
                report,
            ));
        }
    }
    let extension = source_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("bin");
    let file = format!("art/{}.{}", asset.id, extension);
    fs::copy(&source_path, resolution.destination.join(&file))?;
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
        severity: Severity::Error,
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
    use crate::model::SourceParagraph;
    use std::fs;

    #[test]
    fn creates_a_missing_parent_directory_for_package_output() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("art/briefs")).unwrap();
        fs::create_dir_all(directory.path().join("assets/drafts")).unwrap();
        image::RgbImage::new(4800, 750)
            .save(directory.path().join("assets/drafts/story-opener.png"))
            .unwrap();
        fs::write(
            directory.path().join("art/briefs/story-opener.yaml"),
            "schema_version: 3\nart_id: story-opener\nsource: { story_id: story, anchor_id: story-opening }\nusage: opener\ngeneration: { page_treatment: floating, prompt: An opener. }\ncandidates: [{ id: a, file: assets/drafts/story-opener.png }]\n",
        )
        .unwrap();
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
                assets: vec![AssetRecord {
                    id: "story-opener".into(),
                    brief: "art/briefs/story-opener.yaml".into(),
                    status: AssetStatus::Draft,
                    selection: Some(crate::assets::AssetSelection {
                        candidate_id: "a".into(),
                        file: "assets/drafts/story-opener.png".into(),
                        sha256: crate::assets::sha256(
                            directory.path(),
                            "assets/drafts/story-opener.png",
                        )
                        .unwrap(),
                    }),
                    approved: None,
                    superseded_by: None,
                }],
            },
            &output,
            false,
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
    fn rejects_invalid_referenced_story_art() {
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
            "schema_version: 3\nart_id: story-art\nsource: { story_id: story, anchor_id: scene }\ngeneration: { page_treatment: floating, prompt: A scene. }\n",
        )
        .unwrap();
        let registry = AssetRegistry {
            schema: crate::assets::ASSET_REGISTRY_SCHEMA.into(),
            assets: vec![AssetRecord {
                id: "story-art".into(),
                brief: "art/briefs/story-art.yaml".into(),
                status: AssetStatus::Draft,
                selection: None,
                approved: None,
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
            .any(|issue| issue.code == "ART_RECORD_INVALID"));
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
            "schema_version: 3\nart_id: story-art\nsource: { story_id: story, anchor_id: scene, spread_ids: [spread-001] }\ngeneration: { page_treatment: framed, prompt: A scene. }\ncandidates: [{ id: a, file: assets/drafts/story-art.png }]\n",
        )
        .unwrap();
        image::RgbImage::new(4800, 750)
            .save(directory.path().join("assets/drafts/story-art.png"))
            .unwrap();
        image::RgbImage::new(4800, 750)
            .save(directory.path().join("assets/drafts/story-opener.png"))
            .unwrap();
        fs::write(
            directory.path().join("art/briefs/story-opener.yaml"),
            "schema_version: 3\nart_id: story-opener\nsource: { story_id: story, anchor_id: scene }\nusage: opener\ngeneration: { page_treatment: floating, prompt: An opener. }\ncandidates: [{ id: a, file: assets/drafts/story-opener.png }]\n",
        )
        .unwrap();
        let registry = AssetRegistry {
            schema: crate::assets::ASSET_REGISTRY_SCHEMA.into(),
            assets: vec![
                AssetRecord {
                    id: "story-art".into(),
                    brief: "art/briefs/story-art.yaml".into(),
                    status: AssetStatus::Draft,
                    selection: Some(crate::assets::AssetSelection {
                        candidate_id: "a".into(),
                        file: "assets/drafts/story-art.png".into(),
                        sha256: crate::assets::sha256(
                            directory.path(),
                            "assets/drafts/story-art.png",
                        )
                        .unwrap(),
                    }),
                    approved: None,
                    superseded_by: None,
                },
                AssetRecord {
                    id: "story-opener".into(),
                    brief: "art/briefs/story-opener.yaml".into(),
                    status: AssetStatus::Draft,
                    selection: Some(crate::assets::AssetSelection {
                        candidate_id: "a".into(),
                        file: "assets/drafts/story-opener.png".into(),
                        sha256: crate::assets::sha256(
                            directory.path(),
                            "assets/drafts/story-opener.png",
                        )
                        .unwrap(),
                    }),
                    approved: None,
                    superseded_by: None,
                },
            ],
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
            false,
            PackagePolicy {
                minimum: AssetStatus::Draft,
                strict: false,
            },
        )
        .unwrap();

        let manifest = fs::read_to_string(output.join("spreads/001-opening/spread.yaml")).unwrap();
        assert!(manifest.contains("schema: compositor.dev/resolved-spread/v3"));
        assert!(manifest.contains("paragraph_economy:"));
        assert!(manifest.contains("height_percent: 25"));
        assert!(manifest.contains("width_in: 16.0"));
        assert!(manifest.contains("height_in: 2.5"));
        assert!(manifest.contains("aspect_ratio: 6.4"));
        assert!(manifest.contains("width_px: 4800"));
        assert!(manifest.contains("height_px: 750"));
    }

    #[test]
    fn paragraph_economy_flags_fragmentation_and_honors_a_waiver() {
        let paragraphs = (0..16)
            .map(|ordinal| SourceParagraph {
                ordinal,
                source_start: 0,
                source_end: 0,
                content: "One small beat.".into(),
                word_count: 5,
                id: Some(format!("paragraph-{ordinal}")),
                id_comment_start: None,
            })
            .collect::<Vec<_>>();
        let references = paragraphs.iter().collect::<Vec<_>>();
        let config = ParagraphEconomyConfig {
            minimum_words: 80,
            max_paragraphs_per_100_words: 12.0,
            short_paragraph_max_words: 12,
            max_consecutive_short_paragraphs: 5,
        };

        let warning = paragraph_economy_metrics(&references, Some(&config), false);
        assert_eq!(warning.paragraph_count, 16);
        assert_eq!(warning.short_paragraph_count, 16);
        assert_eq!(warning.longest_short_paragraph_run, 16);
        assert_eq!(warning.status, ParagraphEconomyStatus::Warning);

        let waived = paragraph_economy_metrics(&references, Some(&config), true);
        assert_eq!(waived.status, ParagraphEconomyStatus::Waived);

        let short_spread = references.iter().take(15).copied().collect::<Vec<_>>();
        assert_eq!(
            paragraph_economy_metrics(&short_spread, Some(&config), false).status,
            ParagraphEconomyStatus::Ok
        );

        let threshold_paragraphs = (0..12)
            .map(|ordinal| SourceParagraph {
                ordinal,
                source_start: 0,
                source_end: 0,
                content: "A deliberate beat.".into(),
                word_count: if ordinal < 8 { 8 } else { 9 },
                id: Some(format!("threshold-{ordinal}")),
                id_comment_start: None,
            })
            .collect::<Vec<_>>();
        let threshold_references = threshold_paragraphs.iter().collect::<Vec<_>>();
        assert_eq!(
            paragraph_economy_metrics(&threshold_references, Some(&config), false).status,
            ParagraphEconomyStatus::Ok
        );
    }
}
