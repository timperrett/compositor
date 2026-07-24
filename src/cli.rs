use crate::art_brief;
use crate::config::{Config, DEFAULT_CONFIG};
use crate::discovery::discover;
use crate::flow;
use crate::model::{SourceProject, ValidationReport};
use crate::report::{self, OutputFormat};
use crate::storage;
use crate::validation;
use crate::{assets, composition, package};
use crate::{project_root, AppError};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "cli_art_commands.rs"]
mod art_commands;
#[path = "cli_build_commands.rs"]
mod build_commands;

#[derive(Debug, Parser)]
#[command(
    name = "compositor",
    version = crate::BUILD_VERSION,
    about = "Deterministic Markdown-to-book production tooling",
    after_help = "Common workflow:\n  compositor validate\n      Check Markdown source and reject unsupported legacy state.\n  compositor build <compendium> [story]\n      Create a revisioned Flow/Composition delivery package and HTML assembly guide.\n\nRun `compositor help <command>` for command-specific guidance."
)]
struct Cli {
    /// Project directory to read and write. Defaults to the current directory.
    #[arg(long, global = true, value_name = "DIRECTORY")]
    root: Option<PathBuf>,
    /// Format reports as readable text or stable machine-readable JSON.
    #[arg(long, value_enum, default_value_t = OutputFormat::Human, global = true)]
    format: OutputFormat,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
#[command(verbatim_doc_comment)]
enum Command {
    /// Inspect and manage artwork requirements, briefs, candidates, and assets.
    Art {
        #[command(subcommand)]
        command: ArtCommand,
    },
    /// Create a conventional Flow/Composition delivery package.
    ///
    /// Examples:
    /// `compositor build my-compendium my-story --asset-policy approved --strict-art`
    Build {
        /// Compendium ID or directory name.
        compendium: String,
        /// Story ID or directory name within the selected compendium.
        package_story: Option<String>,
        /// Design-system directory to use instead of the plan-derived directory.
        #[arg(long)]
        design_system: Option<PathBuf>,
        /// Art asset registry to use instead of `art/assets.yaml`.
        #[arg(long)]
        assets: Option<PathBuf>,
        /// Minimum artwork status accepted by a package: `draft`, `review`, or `approved`.
        #[arg(long, default_value = "draft")]
        asset_policy: String,
        /// Treat any artwork validation issue as a package-build failure.
        #[arg(long)]
        strict_art: bool,
        /// Package destination override; valid only for a single-story package build.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Replace an existing explicit --output directory after validation succeeds.
        #[arg(long, requires = "output")]
        replace: bool,
    },
    /// Create the directory layout and starter files for a new project.
    ///
    /// Example: `compositor --root ./my-book init`
    Init {
        /// Replace existing generated project files when they already exist.
        #[arg(long)]
        force: bool,
    },
    /// Inspect a Markdown story and print its durable paragraph/source metadata.
    ///
    /// Example: `compositor inspect compendiums/01-book/my-story/story.md`
    Inspect {
        /// Story Markdown file to inspect, for example `compendiums/01-book/story/story.md`.
        story: PathBuf,
    },
    /// Parse the project and report its discovered compendiums, stories, and units.
    ///
    /// Example: `compositor parse --story my-story --format json`
    Parse {
        /// Limit parsing to one story ID; omit to parse the entire project.
        #[arg(long)]
        story: Option<String>,
    },
    /// Synchronize or repair the paragraph ledger for a Flow-ready story.
    ///
    /// Examples:
    /// `compositor source sync story.md --write`
    ///
    /// `compositor source resolve story.md paragraph-001 fingerprint --write`
    Source {
        #[command(subcommand)]
        command: SourceCommand,
    },
    /// Display compendiums, stories, and optionally their art IDs as a tree.
    ///
    /// Examples:
    /// `compositor tree --art`
    ///
    /// `compositor tree my-story --spreads`
    Tree {
        /// Limit the tree to one story ID.
        story: Option<String>,
        /// Include art IDs from story-linked art briefs.
        #[arg(long)]
        art: bool,
        /// List every Flow spread with art mapped by briefs and the hardcover Composition Plan.
        #[arg(long, requires = "story")]
        spreads: bool,
    },
    /// Validate authored Markdown and generated production state.
    ///
    /// Examples:
    /// `compositor validate`
    ///
    /// `compositor validate --story my-story --strict`
    Validate {
        /// Limit validation to one story ID; omit to validate the whole project.
        #[arg(long)]
        story: Option<String>,
        /// Also fail for warnings, not only errors.
        #[arg(long)]
        strict: bool,
    },
    /// Validate a Composition Plan against its Flow Plan and design system.
    ///
    /// Example: `compositor validate-composition story.md story.flow.yaml hardcover.composition.yaml --design-system design-systems/print`
    ValidateComposition {
        /// Story Markdown file used to check title and source relationships.
        story: PathBuf,
        /// Story Flow Plan referenced by the Composition Plan.
        flow: PathBuf,
        /// Composition Plan to validate.
        composition: PathBuf,
        /// Design-system directory used for layout-family validation.
        #[arg(long)]
        design_system: PathBuf,
    },
    /// Validate a Story Flow Plan against its Markdown source and design system.
    ///
    /// Example: `compositor validate-flow story.md story.flow.yaml --design-system design-systems/print`
    ValidateFlow {
        /// Story Markdown file whose paragraph IDs and source revision are checked.
        story: PathBuf,
        /// Story Flow Plan to validate.
        flow: PathBuf,
        /// Design-system directory used for role, energy, and pacing validation.
        #[arg(long)]
        design_system: PathBuf,
    },
    /// Verify that a delivery package matches the current manuscript and plans.
    ///
    /// Example: `compositor validate-package output/packages/my-book/r001/my-story --strict`
    ValidatePackage {
        /// Package directory containing manifest.yaml.
        package: PathBuf,
        /// Treat advisory paragraph-economy findings as validation failures.
        #[arg(long)]
        strict: bool,
    },
}

#[derive(Debug, Subcommand)]
#[command(verbatim_doc_comment)]
enum SourceCommand {
    /// Rebind an unmatched paragraph to an existing durable paragraph ID.
    ///
    /// Example: `compositor source resolve story.md paragraph-001 fingerprint --write`
    Resolve {
        /// Story Markdown file whose paragraph ledger should be repaired.
        story: PathBuf,
        /// Existing paragraph ID to preserve.
        old_id: String,
        /// Fingerprint of the candidate paragraph that should receive the ID.
        candidate_fingerprint: String,
        /// Write the resolution; without this flag, show the proposed result only.
        #[arg(long)]
        write: bool,
    },
    /// Generate or update the paragraph ledger and annotated review view.
    ///
    /// Example: `compositor source sync story.md --write`
    Sync {
        /// Story Markdown file to synchronize.
        story: PathBuf,
        /// Write `story.paragraphs.yaml` and `story.annotated.md`; without this flag, preview only.
        #[arg(long)]
        write: bool,
    },
}
#[derive(Debug, Subcommand)]
#[command(verbatim_doc_comment)]
enum ArtCommand {
    /// Approve a reviewed selected candidate and copy it into assets/approved.
    ///
    /// Example: `compositor art approve lantern-opener`
    Approve {
        /// Artwork ID from the illustration requirement and art brief.
        art_id: String,
    },
    /// Print the validated YAML art brief for one artwork ID.
    ///
    /// Example: `compositor art brief lantern-opener`
    Brief {
        /// Artwork ID whose brief should be shown.
        art_id: String,
    },
    /// Report opener and narrative-spread artwork coverage for one edition.
    ///
    /// Example: `compositor art coverage --story my-story --edition hardcover`
    Coverage {
        /// Story ID whose Flow and Composition Plans should be checked.
        #[arg(long)]
        story: String,
        /// Edition ID used to locate `<edition>.composition.yaml` beside the story.
        #[arg(long)]
        edition: String,
    },
    /// Copy a generated image into the brief as a geometry-checked candidate.
    ///
    /// Candidates that do not match the current requirement geometry are kept
    /// under the revision's rejected-attempt log for later review.
    ///
    /// Example: `compositor art ingest-candidate lantern-opener candidate.png --revision r04 --attempt 1`
    IngestCandidate {
        /// Artwork ID whose requirement and brief should receive the candidate.
        art_id: String,
        /// PNG, JPG, JPEG, or WebP file to copy into the project.
        source: PathBuf,
        /// Requirement revision, for example `r04`.
        #[arg(long)]
        revision: String,
        /// Generation attempt number from 1 through 3.
        #[arg(long)]
        attempt: u32,
    },
    /// Show the current requirement, brief, candidates, and registry lifecycle state.
    ///
    /// Example: `compositor art inspect lantern-opener`
    Inspect {
        /// Artwork ID to inspect.
        art_id: String,
    },
    /// List illustration requirements and their current brief/candidate state.
    ///
    /// Example: `compositor art list --story my-story --format json`
    List {
        /// Limit the list to one story ID; omit to list the whole project.
        #[arg(long)]
        story: Option<String>,
    },
    /// Create a registry entry for an artwork requirement.
    ///
    /// Example: `compositor art register lantern-opener`
    Register {
        /// Artwork ID to add to `art/assets.yaml`.
        art_id: String,
    },
    /// Mark an artwork record as rejected.
    ///
    /// Example: `compositor art reject lantern-opener`
    Reject {
        /// Artwork ID to reject.
        art_id: String,
    },
    /// Move an artwork record into review status.
    ///
    /// Example: `compositor art review lantern-opener`
    Review {
        /// Artwork ID ready for human review.
        art_id: String,
    },
    /// Select one candidate in an art brief without approving it.
    ///
    /// Example: `compositor art select lantern-opener b --feedback "Use candidate b; preserve the warm window light."`
    Select {
        /// Artwork ID whose candidate should be selected.
        art_id: String,
        /// Candidate ID, usually `a`, `b`, or `c`.
        candidate_id: String,
        /// Optional editorial feedback recorded with the selection.
        #[arg(long)]
        feedback: Option<String>,
    },
    /// Mark an artwork record as superseded by another artwork ID.
    ///
    /// Example: `compositor art supersede old-opener new-opener`
    Supersede {
        /// Existing artwork ID to supersede.
        art_id: String,
        /// Successor artwork ID that replaces it.
        successor: String,
    },
    /// Validate art briefs, requirements, and the asset registry.
    ///
    /// Examples:
    /// `compositor art validate --strict`
    ///
    /// `compositor art validate --story my-story --format json`
    Validate {
        /// Limit validation to one story ID; omit to validate all artwork records.
        #[arg(long)]
        story: Option<String>,
        /// Also fail for warnings, not only blocking issues.
        #[arg(long)]
        strict: bool,
    },
}

pub fn run() -> Result<(), AppError> {
    let cli = Cli::parse();
    let root = project_root(cli.root)?;
    match cli.command {
        Command::Init { force } => build_commands::init(&root, force, cli.format),
        Command::Inspect { story } => inspect_source(&story, cli.format),
        Command::Source { command } => source_command(command, cli.format),
        Command::ValidateFlow {
            story,
            flow: flow_path,
            design_system,
        } => validate_flow(&story, &flow_path, &design_system, cli.format),
        Command::ValidateComposition {
            story,
            flow,
            composition: plan,
            design_system,
        } => validate_composition(&root, &story, &flow, &plan, &design_system, cli.format),
        command => execute(&root, cli.format, command),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_subcommands_are_alphabetical() {
        let cli = Cli::command();
        assert_subcommands_are_alphabetical(&cli);

        for name in ["art", "source"] {
            let nested = cli
                .get_subcommands()
                .find(|command| command.get_name() == name)
                .expect("nested command should exist");
            assert_subcommands_are_alphabetical(nested);
        }
    }

    fn assert_subcommands_are_alphabetical(command: &clap::Command) {
        let actual: Vec<_> = command
            .get_subcommands()
            .filter(|subcommand| subcommand.get_name() != "help")
            .map(|subcommand| subcommand.get_name().to_owned())
            .collect();
        let mut expected = actual.clone();
        expected.sort_unstable();
        assert_eq!(actual, expected, "subcommands for `{}`", command.get_name());
    }
}

fn source_command(command: SourceCommand, format: OutputFormat) -> Result<(), AppError> {
    let ledger = match command {
        SourceCommand::Sync { story, write } => crate::paragraph_ledger::sync(&story, write)?,
        SourceCommand::Resolve {
            story,
            old_id,
            candidate_fingerprint,
            write,
        } => crate::paragraph_ledger::resolve(&story, &old_id, &candidate_fingerprint, write)?,
    };
    print_report(format, "source", ledger, ValidationReport::default())
}

fn execute(root: &std::path::Path, format: OutputFormat, command: Command) -> Result<(), AppError> {
    let config = Config::load(root)?;
    match command {
        Command::Source { .. } => Err(AppError::command(
            "source commands are handled before project configuration is loaded".into(),
        )),
        Command::Parse { story } => {
            let project = filtered_project(discover(root, &config)?, story.as_deref())?;
            let validation = validation::validate(&project);
            print_report(format, "parse", project, validation)
        }
        Command::Validate { story, strict } => {
            let project = filtered_project(discover(root, &config)?, story.as_deref())?;
            let mut report = validation::validate(&project);
            if root.join(".compositor").exists() {
                report.issues.push(crate::model::ValidationIssue {
                    severity: crate::model::Severity::Blocking,
                    code: "LEGACY_PRODUCTION_STATE_UNSUPPORTED".into(),
                    message: "legacy .compositor state is unsupported; preserve it in version control, remove it manually, and rebuild from Flow and Composition plans".into(),
                    path: ".compositor".into(), story_id: None, unit_id: None,
                });
            }
            print_report(format, "validate", report.clone(), report.clone())?;
            if strict
                && report
                    .issues
                    .iter()
                    .any(|issue| issue.severity == crate::model::Severity::Warning)
            {
                Err(AppError::Validation)
            } else if report.can_proceed() {
                Ok(())
            } else if report.is_blocking() {
                Err(AppError::Blocking(
                    "validation contains blocking issues".into(),
                ))
            } else {
                Err(AppError::Validation)
            }
        }
        Command::ValidatePackage { package, strict } => {
            let (output, report) = crate::package::validate_package(root, &config, &package)?;
            print_report(format, "validate-package", output, report.clone())?;
            if strict
                && report
                    .issues
                    .iter()
                    .any(|issue| issue.severity == crate::model::Severity::Warning)
            {
                Err(AppError::Validation)
            } else if report.can_proceed() {
                Ok(())
            } else {
                Err(AppError::Validation)
            }
        }
        Command::Tree {
            story,
            art,
            spreads,
        } => {
            let project = filtered_project(discover(root, &config)?, story.as_deref())?;
            tree_command(root, &project, art, spreads, format)
        }
        Command::Build {
            compendium,
            package_story,
            design_system,
            assets: assets_path,
            asset_policy,
            strict_art,
            output,
            replace,
        } => build_conventional_packages(
            root,
            &config,
            ConventionalPackageBuildRequest {
                compendium_selector: &compendium,
                story_selector: package_story.as_deref(),
                design_system_override: design_system.as_deref(),
                assets_override: assets_path.as_deref(),
                output_override: output.as_deref(),
                replace,
                policy: &asset_policy,
                strict_art,
                format,
            },
        ),
        Command::Art { command } => art_commands::art_command(root, &config, format, command),
        Command::Inspect { .. }
        | Command::ValidateFlow { .. }
        | Command::ValidateComposition { .. } => unreachable!(),
        Command::Init { .. } => Err(AppError::command(
            "init is handled before configuration is loaded".into(),
        )),
    }
}

#[derive(Debug, Serialize)]
struct TreeOutput {
    compendiums: Vec<TreeCompendium>,
}

#[derive(Debug, Serialize)]
struct TreeCompendium {
    id: String,
    title: String,
    stories: Vec<TreeStory>,
}

#[derive(Debug, Serialize)]
struct TreeStory {
    id: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    art_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spreads: Option<Vec<TreeSpread>>,
}

#[derive(Debug, Clone, Serialize)]
struct TreeSpread {
    id: String,
    brief_art_ids: Vec<String>,
    composition_art_ids: Vec<String>,
}

#[derive(Debug)]
struct TreeNode {
    label: String,
    children: Vec<TreeNode>,
}

fn tree_command(
    root: &Path,
    project: &SourceProject,
    include_art: bool,
    include_spreads: bool,
    format: OutputFormat,
) -> Result<(), AppError> {
    let art_by_story = if include_art {
        art_ids_by_story(root)?
    } else {
        BTreeMap::new()
    };
    let spreads_by_story = if include_spreads {
        spread_art_by_story(root, project)?
    } else {
        BTreeMap::new()
    };
    let output = TreeOutput {
        compendiums: project
            .compendiums
            .iter()
            .map(|compendium| TreeCompendium {
                id: compendium.id.clone(),
                title: compendium.title.clone(),
                stories: compendium
                    .stories
                    .iter()
                    .map(|story| TreeStory {
                        id: story.id.clone(),
                        title: story.title.clone(),
                        art_ids: include_art
                            .then(|| art_by_story.get(&story.id).cloned().unwrap_or_default()),
                        spreads: include_spreads
                            .then(|| spreads_by_story.get(&story.id).cloned().unwrap_or_default()),
                    })
                    .collect(),
            })
            .collect(),
    };

    match format {
        OutputFormat::Human => {
            print!("{}", render_tree(&output));
            Ok(())
        }
        OutputFormat::Json => print_report(format, "tree", output, ValidationReport::default()),
    }
}

fn spread_art_by_story(
    root: &Path,
    project: &SourceProject,
) -> Result<BTreeMap<String, Vec<TreeSpread>>, AppError> {
    let mut briefs_by_story = BTreeMap::<String, BTreeMap<String, Vec<String>>>::new();
    for id in art_brief::ids(root)? {
        let Some(brief) = art_brief::load(root, &id)? else {
            continue;
        };
        if brief.usage != art_brief::ArtUsage::Story {
            continue;
        }
        for spread_id in &brief.source.spread_ids {
            briefs_by_story
                .entry(brief.source.story_id.clone())
                .or_default()
                .entry(spread_id.clone())
                .or_default()
                .push(brief.art_id.clone());
        }
    }
    for spreads in briefs_by_story.values_mut() {
        for art_ids in spreads.values_mut() {
            art_ids.sort();
        }
    }

    let mut output = BTreeMap::new();
    for story in project
        .compendiums
        .iter()
        .flat_map(|compendium| &compendium.stories)
    {
        let story_path = root.join(&story.source);
        let directory = story_path.parent().ok_or_else(|| {
            AppError::command(format!("story has no parent directory: {}", story.source))
        })?;
        let flow = flow::load_plan(&directory.join("story.flow.yaml"))?;
        let composition = composition::load_plan(&directory.join("hardcover.composition.yaml"))?;
        let briefs = briefs_by_story.get(&story.id);
        let spreads = flow
            .spreads
            .iter()
            .map(|flow_spread| {
                let mut composition_art_ids = composition
                    .spreads
                    .iter()
                    .find(|spread| spread.id == flow_spread.id)
                    .map(|spread| {
                        spread
                            .art_assets
                            .iter()
                            .map(|asset| asset.id.clone())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                composition_art_ids.sort();
                TreeSpread {
                    id: flow_spread.id.clone(),
                    brief_art_ids: briefs
                        .and_then(|spreads| spreads.get(&flow_spread.id))
                        .cloned()
                        .unwrap_or_default(),
                    composition_art_ids,
                }
            })
            .collect();
        output.insert(story.id.clone(), spreads);
    }
    Ok(output)
}

fn art_ids_by_story(root: &Path) -> Result<BTreeMap<String, Vec<String>>, AppError> {
    let mut art_by_story = BTreeMap::<String, Vec<String>>::new();
    for id in art_brief::ids(root)? {
        let Some(brief) = art_brief::load(root, &id)? else {
            continue;
        };
        art_by_story
            .entry(brief.source.story_id)
            .or_default()
            .push(brief.art_id);
    }
    for art_ids in art_by_story.values_mut() {
        art_ids.sort();
    }
    Ok(art_by_story)
}

fn render_tree(output: &TreeOutput) -> String {
    let root = TreeNode {
        label: "compendiums".into(),
        children: output
            .compendiums
            .iter()
            .map(|compendium| TreeNode {
                label: format!("{} [{}]", compendium.title, compendium.id),
                children: compendium
                    .stories
                    .iter()
                    .map(|story| TreeNode {
                        label: format!("{} [{}]", story.title, story.id),
                        children: story
                            .art_ids
                            .as_deref()
                            .unwrap_or_default()
                            .iter()
                            .map(|id| TreeNode {
                                label: format!("art: {id}"),
                                children: Vec::new(),
                            })
                            .chain(story.spreads.as_deref().unwrap_or_default().iter().map(
                                |spread| {
                                    TreeNode {
                                        label: format!("spread: {}", spread.id),
                                        children: spread
                                            .brief_art_ids
                                            .iter()
                                            .map(|id| TreeNode {
                                                label: format!("brief: {id}"),
                                                children: Vec::new(),
                                            })
                                            .chain(spread.composition_art_ids.iter().map(|id| {
                                                TreeNode {
                                                    label: format!("composition: {id}"),
                                                    children: Vec::new(),
                                                }
                                            }))
                                            .collect(),
                                    }
                                },
                            ))
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    };
    let mut rendered = format!("{}\n", root.label);
    append_tree_children(&mut rendered, "", &root.children);
    rendered
}

fn append_tree_children(rendered: &mut String, prefix: &str, children: &[TreeNode]) {
    for (index, child) in children.iter().enumerate() {
        let is_last = index + 1 == children.len();
        rendered.push_str(prefix);
        rendered.push_str(if is_last { "└── " } else { "├── " });
        rendered.push_str(&child.label);
        rendered.push('\n');
        let child_prefix = if is_last {
            format!("{prefix}    ")
        } else {
            format!("{prefix}│   ")
        };
        append_tree_children(rendered, &child_prefix, &child.children);
    }
}

fn relative_path(root: &std::path::Path, path: &std::path::Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[derive(Debug, Serialize)]
struct SourceInspection {
    story: String,
    source_revision: String,
    paragraphs: Vec<crate::model::SourceParagraph>,
}

fn inspect_source(path: &std::path::Path, format: OutputFormat) -> Result<(), AppError> {
    let story = flow::load_story(path)?;
    let validation = flow::source_report(&story);
    print_report(
        format,
        "inspect",
        SourceInspection {
            story: story.id,
            source_revision: story.source_hash,
            paragraphs: story.paragraphs,
        },
        validation,
    )
}

fn validate_flow(
    story_path: &std::path::Path,
    flow_path: &std::path::Path,
    design_system: &std::path::Path,
    format: OutputFormat,
) -> Result<(), AppError> {
    let story = flow::load_story(story_path)?;
    let plan = flow::load_plan(flow_path)?;
    let design = flow::load_design_system(design_system)?;
    let report = flow::validate(&story, &plan, &design);
    print_report(format, "validate-flow", &plan, report.clone())?;
    if report.can_proceed() {
        Ok(())
    } else {
        Err(AppError::Validation)
    }
}

fn validate_composition(
    root: &std::path::Path,
    story_path: &std::path::Path,
    flow_path: &std::path::Path,
    composition_path: &std::path::Path,
    design_system: &std::path::Path,
    format: OutputFormat,
) -> Result<(), AppError> {
    let story = flow::load_story(story_path)?;
    let flow_plan = flow::load_plan(flow_path)?;
    let plan = composition::load_plan(composition_path)?;
    let catalog = composition::load_catalog(design_system)?;
    let mut report = composition::validate(&flow_plan, &plan, &catalog);
    report
        .issues
        .extend(composition::validate_art_usage(root, &flow_plan, &plan)?.issues);
    report
        .issues
        .extend(composition::validate_story_title(&story, &plan).issues);
    print_report(format, "validate-composition", &plan, report.clone())?;
    if report.can_proceed() {
        Ok(())
    } else {
        Err(AppError::Validation)
    }
}

#[derive(Debug, Serialize)]
struct PackageBuildOutput {
    revision: Option<String>,
    outputs: Vec<String>,
}

struct ConventionalPackageStory {
    directory: PathBuf,
}

struct ConventionalPackageBuildRequest<'a> {
    compendium_selector: &'a str,
    story_selector: Option<&'a str>,
    design_system_override: Option<&'a Path>,
    assets_override: Option<&'a Path>,
    output_override: Option<&'a Path>,
    replace: bool,
    policy: &'a str,
    strict_art: bool,
    format: OutputFormat,
}

fn build_conventional_packages(
    root: &Path,
    config: &Config,
    request: ConventionalPackageBuildRequest<'_>,
) -> Result<(), AppError> {
    let legacy_manifest = root.join(".compositor/manifest.json");
    if legacy_manifest.exists() {
        return Err(AppError::Blocking(format!(
            "legacy production state detected at {}; this Flow/Composition-only release will not read or migrate it. Preserve it in version control, remove .compositor, then rebuild from story.flow.yaml and hardcover.composition.yaml",
            legacy_manifest.display()
        )));
    }
    let project = discover(root, config)?;
    let compendium = project
        .compendiums
        .iter()
        .find(|candidate| {
            candidate.id == request.compendium_selector
                || candidate
                    .source
                    .strip_suffix("/index.md")
                    .and_then(|path| Path::new(path).file_name())
                    .and_then(|name| name.to_str())
                    == Some(request.compendium_selector)
        })
        .ok_or_else(|| {
            AppError::command(format!(
                "unknown compendium `{}`; use its ID or directory name",
                request.compendium_selector
            ))
        })?;
    let stories = compendium
        .stories
        .iter()
        .filter(|candidate| {
            request.story_selector.is_none_or(|selector| {
                candidate.id == selector
                    || Path::new(&candidate.source)
                        .parent()
                        .and_then(|path| path.file_name())
                        .and_then(|name| name.to_str())
                        == Some(selector)
            })
        })
        .map(|candidate| ConventionalPackageStory {
            directory: root.join(
                Path::new(&candidate.source)
                    .parent()
                    .expect("story source always has a parent"),
            ),
        })
        .collect::<Vec<_>>();
    if stories.is_empty() {
        return Err(match request.story_selector {
            Some(selector) => AppError::command(format!(
                "unknown story `{selector}` in compendium `{}`; use its ID or directory name",
                compendium.id
            )),
            None => AppError::command(format!(
                "compendium `{}` contains no story directories",
                compendium.id
            )),
        });
    }
    if request.output_override.is_some() && stories.len() > 1 {
        return Err(AppError::command(
            "--output is only supported when building a single story package".into(),
        ));
    }
    if request.replace && request.output_override.is_none() {
        return Err(AppError::command("--replace requires --output".into()));
    }
    let assets_path = request
        .assets_override
        .map(Path::to_path_buf)
        .unwrap_or_else(|| assets::path(root));
    let revision = request
        .output_override
        .is_none()
        .then(|| next_package_revision(&config.output_dir(root), &compendium.id))
        .transpose()?;
    let generated_output_root = revision.as_ref().map(|revision| {
        config
            .output_dir(root)
            .join("packages")
            .join(&compendium.id)
            .join(revision)
    });
    let mut validation = ValidationReport::default();
    let mut outputs = Vec::new();
    for target in stories {
        let story_path = target.directory.join("story.md");
        let flow_path = target.directory.join("story.flow.yaml");
        let composition_path = target.directory.join("hardcover.composition.yaml");
        let composition_plan = composition::load_plan(&composition_path)?;
        let design_system = request
            .design_system_override
            .map(Path::to_path_buf)
            .unwrap_or_else(|| {
                root.join("design-systems")
                    .join(&composition_plan.edition.design_system)
            });
        let output = request
            .output_override
            .map(Path::to_path_buf)
            .unwrap_or_else(|| {
                generated_output_root
                    .as_ref()
                    .expect("generated package output has a revision")
                    .join(
                        target
                            .directory
                            .file_name()
                            .expect("story directory always has a name"),
                    )
            });
        validation.issues.extend(
            build_package(
                root,
                config,
                &story_path,
                &flow_path,
                &composition_path,
                &design_system,
                &assets_path,
                &output,
                request.replace,
                request.policy,
                request.strict_art,
            )?
            .issues,
        );
        outputs.push(relative_path(root, &output));
    }
    print_report(
        request.format,
        "build",
        PackageBuildOutput { revision, outputs },
        validation.clone(),
    )?;
    if validation.can_proceed() || !request.strict_art {
        Ok(())
    } else {
        Err(AppError::Validation)
    }
}

fn next_package_revision(output_directory: &Path, compendium_id: &str) -> Result<String, AppError> {
    let revisions = output_directory.join("packages").join(compendium_id);
    let mut greatest = 0_u64;
    if revisions.is_dir() {
        for entry in fs::read_dir(&revisions)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let Some(value) = name.to_str().and_then(|name| name.strip_prefix('r')) else {
                continue;
            };
            if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
                greatest = greatest.max(value.parse::<u64>().map_err(|error| {
                    AppError::command(format!(
                        "invalid package revision `{}`: {error}",
                        name.to_string_lossy()
                    ))
                })?);
            }
        }
    }
    Ok(format!("r{:02}", greatest + 1))
}

#[allow(clippy::too_many_arguments)]
fn build_package(
    project_root: &Path,
    config: &Config,
    story_path: &Path,
    flow_path: &Path,
    composition_path: &Path,
    design_system: &Path,
    assets_path: &Path,
    output: &Path,
    replace: bool,
    policy: &str,
    strict_art: bool,
) -> Result<ValidationReport, AppError> {
    let story = flow::load_story(story_path)?;
    let flow_plan = flow::load_plan(flow_path)?;
    let composition_plan = composition::load_plan(composition_path)?;
    let catalog = composition::load_catalog(design_system)?;
    let mut report = composition::validate(&flow_plan, &composition_plan, &catalog);
    report.issues.extend(
        composition::validate_art_usage(project_root, &flow_plan, &composition_plan)?.issues,
    );
    report
        .issues
        .extend(composition::validate_story_title(&story, &composition_plan).issues);
    if !report.can_proceed() {
        return Err(AppError::Validation);
    }
    let registry = assets::load_from(assets_path)?
        .ok_or_else(|| AppError::command("asset registry does not exist".into()))?;
    let minimum = match policy {
        "draft" => assets::AssetStatus::Draft,
        "review" => assets::AssetStatus::Review,
        "approved" => assets::AssetStatus::Approved,
        _ => {
            return Err(AppError::command(
                "asset policy must be draft, review, or approved".into(),
            ))
        }
    };
    let validation = package::build(
        project_root,
        config,
        &story,
        &flow_plan,
        &composition_plan,
        &registry,
        output,
        replace,
        package::PackagePolicy {
            minimum,
            strict: strict_art,
        },
    )?;
    Ok(validation)
}

fn filtered_project(
    mut project: crate::model::SourceProject,
    story_filter: Option<&str>,
) -> Result<crate::model::SourceProject, AppError> {
    if let Some(story_id) = story_filter {
        let mut found = false;
        for compendium in &mut project.compendiums {
            compendium.stories.retain(|story| {
                let keep = story.id == story_id;
                found |= keep;
                keep
            });
        }
        project
            .compendiums
            .retain(|compendium| !compendium.stories.is_empty());
        if !found {
            return Err(AppError::Command(format!("unknown story `{story_id}`")));
        }
    }
    Ok(project)
}

fn print_report<T: Serialize + std::fmt::Debug>(
    format: OutputFormat,
    command: &str,
    data: T,
    validation: ValidationReport,
) -> Result<(), AppError> {
    print!("{}", report::render(format, command, data, validation)?);
    Ok(())
}
