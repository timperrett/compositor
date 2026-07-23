use crate::art_brief;
use crate::build;
use crate::config::{Config, DEFAULT_CONFIG};
use crate::discovery::discover;
use crate::flow;
use crate::model::{ChangeSet, SourceProject, ValidationReport};
use crate::proof;
use crate::report::{self, OutputFormat};
use crate::storage;
use crate::validation;
use crate::{assets, composition, overrides, package};
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
#[path = "cli_proof_command.rs"]
mod proof_command;

#[derive(Debug, Parser)]
#[command(
    name = "compositor",
    version = crate::BUILD_VERSION,
    about = "Deterministic Markdown-to-book production tooling",
    after_help = "Common workflows:\n  compositor init\n      Create the project directories and starter compositor.toml.\n  compositor validate\n      Check authored Markdown and generated state for blocking issues.\n  compositor build --mode conservative\n      Rebuild production state while preserving unaffected assignments.\n  compositor proof\n      Write HTML proofs to output/proofs/ for editorial review.\n  compositor build <compendium> [story]\n      Create a delivery package from the conventional Flow and Composition files.\n\nRun `compositor help <command>` for command-specific guidance, or\n`compositor help art <command>` for an artwork workflow."
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
    /// Approve a generated artifact revision for downstream use.
    ///
    /// Example: `compositor approve plan my-story v001`
    Approve {
        /// Kind of artifact to approve. Currently only `plan` is supported.
        kind: ApprovalKind,
        /// Story or artifact ID whose revision should be approved.
        id: String,
        /// Revision to approve, for example `v001`.
        revision: String,
    },
    /// Inspect and manage artwork requirements, briefs, candidates, and assets.
    Art {
        #[command(subcommand)]
        command: ArtCommand,
    },
    /// Build production state or create a conventional delivery package.
    ///
    /// Without a compendium target, this updates `.compositor/` production
    /// state. With a compendium target, it packages the selected story or all
    /// stories using `story.md`, `story.flow.yaml`, and the edition Composition
    /// Plan found beside the manuscript.
    ///
    /// Examples:
    /// `compositor build --mode conservative`
    ///
    /// `compositor build my-compendium my-story --asset-policy approved --strict-art`
    Build {
        /// Compendium ID or directory name for a delivery package. When present,
        /// every story is built unless a package story is supplied.
        compendium: Option<String>,
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
        /// Production-state planning mode: `conservative`, `rebalance`, or `fresh`.
        #[arg(long, default_value = "conservative")]
        mode: String,
        /// Build production state for one story ID instead of a delivery package.
        #[arg(long, conflicts_with = "compendium")]
        story: Option<String>,
    },
    /// Diagnose one Flow and Composition Plan pair and report compatibility issues.
    ///
    /// Example: `compositor diagnose story.md story.flow.yaml hardcover.composition.yaml --design-system design-systems/print`
    Diagnose {
        /// Story Markdown file used as the source of truth.
        story: PathBuf,
        /// Story Flow Plan to diagnose against the source.
        flow: PathBuf,
        /// Composition Plan to diagnose against the Flow Plan.
        composition: PathBuf,
        /// Design-system directory referenced by the Composition Plan.
        #[arg(long)]
        design_system: PathBuf,
    },
    /// Compare source state or two saved production-plan revisions.
    ///
    /// Examples:
    /// `compositor diff source`
    ///
    /// `compositor diff plan my-story v001 v002`
    Diff {
        #[command(subcommand)]
        target: DiffTarget,
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
    /// Build or refresh the production plan for one story.
    ///
    /// Example: `compositor plan my-story --mode rebalance`
    Plan {
        /// Story ID to plan.
        story: String,
        /// Planning mode: `conservative`, `rebalance`, or `fresh`.
        #[arg(long, default_value = "conservative")]
        mode: String,
    },
    /// Generate HTML proofs for all stories, or one selected story.
    ///
    /// Example: `compositor proof --story my-story`
    Proof {
        /// Story ID to prove; omit to write each story proof and compendium indexes.
        #[arg(long)]
        story: Option<String>,
    },
    /// Reconcile a Composition Plan with overrides and write the resolved plan.
    ///
    /// Example: `compositor reconcile story.composition.yaml overrides.yaml --output resolved.composition.yaml`
    Reconcile {
        /// Composition Plan to reconcile.
        composition: PathBuf,
        /// Overrides file containing deliberate editorial adjustments.
        overrides: PathBuf,
        /// Output path for the reconciled Composition Plan.
        #[arg(long)]
        output: PathBuf,
    },
    /// Record a deliberate manual identity match between two story IDs.
    ///
    /// Example: `compositor resolve old-story-id my-story-id`
    Resolve {
        /// Previous or automatically generated story ID.
        old_id: String,
        /// Stable story ID that should replace or resolve the old ID.
        new_id: String,
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
    /// Show change detection, active/candidate plans, and artwork requirements.
    ///
    /// Example: `compositor status --format json`
    Status,
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
enum DiffTarget {
    /// Compare two saved plan revisions and write an HTML comparison report.
    ///
    /// Example: `compositor diff plan my-story v001 v002`
    Plan {
        /// Story ID whose revisions should be compared.
        story: String,
        /// Earlier plan revision, for example `v001`.
        before: String,
        /// Later plan revision, for example `v002`.
        after: String,
    },
    /// Recompute and report changes between source and the current production state.
    ///
    /// Example: `compositor diff source`
    Source,
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
#[derive(Debug, Clone, clap::ValueEnum)]
enum ApprovalKind {
    Plan,
}

#[derive(Debug, Subcommand)]
#[command(verbatim_doc_comment)]
enum ArtCommand {
    /// Mark an attached artwork record as approved for production use.
    ///
    /// Example: `compositor art approve-asset lantern-opener`
    ApproveAsset {
        /// Artwork ID from the illustration requirement and art brief.
        art_id: String,
    },
    /// Attach approved artwork to an illustration requirement.
    ///
    /// Supply a path to an already approved file, or use `--selected` to copy
    /// the selected candidate from the art brief into `assets/approved/`.
    ///
    /// Examples:
    /// `compositor art attach lantern-opener assets/approved/lantern-opener.png`
    ///
    /// `compositor art attach lantern-opener --selected`
    Attach {
        /// Artwork ID to attach.
        art_id: String,
        /// Approved image path when attaching an existing approved asset.
        path: Option<PathBuf>,
        /// Copy the selected candidate into the approved asset directory first.
        #[arg(long)]
        selected: bool,
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
    /// Show the requirement, brief, candidates, selection, and attachment state.
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
    /// Preview or write migration of legacy Markdown-style art briefs to YAML v2.
    ///
    /// Example: `compositor art migrate-briefs --write`
    MigrateBriefs {
        /// Rewrite the brief files; without this flag, report the files that would change.
        #[arg(long)]
        write: bool,
    },
    /// Create a registry entry for an artwork requirement.
    ///
    /// Example: `compositor art register lantern-opener`
    Register {
        /// Artwork ID to add to `art/assets.yaml`.
        art_id: String,
    },
    /// Preview or write migration of the artwork asset registry.
    ///
    /// Example: `compositor art registry --write`
    Registry {
        /// Write the migrated registry to `art/assets.yaml`.
        #[arg(long)]
        write: bool,
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
    /// Select one candidate in an art brief without approving or attaching it.
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
        Command::Diagnose {
            story,
            flow,
            composition: plan,
            design_system,
        } => validate_composition(&root, &story, &flow, &plan, &design_system, cli.format),
        Command::Reconcile {
            composition: plan,
            overrides: overrides_path,
            output,
        } => reconcile_composition(&plan, &overrides_path, &output, cli.format),
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

        for name in ["art", "diff", "source"] {
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
            let manifest = storage::load_manifest(root, &config)?;
            report.issues.extend(
                validation::validate_state(root, &config, &project, manifest.as_ref()).issues,
            );
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
        Command::Status => {
            let prepared = build::prepare(root, &config)?;
            let status = art_commands::status_output(root, &config, &prepared)?;
            print_report(format, "status", status, prepared.validation)
        }
        Command::Diff {
            target: DiffTarget::Source,
        } => {
            let prepared = build::prepare(root, &config)?;
            print_report(format, "diff source", prepared.changes, prepared.validation)
        }
        Command::Diff {
            target:
                DiffTarget::Plan {
                    story,
                    before,
                    after,
                },
        } => {
            let before =
                storage::load_plan_revision(root, &config, &story, parse_revision(&before)?)?;
            let after =
                storage::load_plan_revision(root, &config, &story, parse_revision(&after)?)?;
            let diff = crate::diff::compare_plans(&before, &after);
            let path = config.output_dir(root).join("reports").join(format!(
                "{}-v{:03}-v{:03}-plan-diff.html",
                story, before.revision, after.revision
            ));
            storage::write_text_atomic(&path, &crate::diff::render_plan_diff_html(&diff))?;
            print_report(
                format,
                "diff plan",
                PlanDiffOutput {
                    diff,
                    path: relative_path(root, &path),
                },
                ValidationReport::default(),
            )
        }
        Command::Build {
            compendium,
            package_story,
            design_system,
            assets: assets_path,
            asset_policy,
            strict_art,
            output,
            mode,
            story,
        } => {
            if let Some(compendium) = compendium {
                return build_conventional_packages(
                    root,
                    &config,
                    ConventionalPackageBuildRequest {
                        compendium_selector: &compendium,
                        story_selector: package_story.as_deref(),
                        design_system_override: design_system.as_deref(),
                        assets_override: assets_path.as_deref(),
                        output_override: output.as_deref(),
                        policy: &asset_policy,
                        strict_art,
                        format,
                    },
                );
            }
            if package_story.is_some() {
                return Err(AppError::command(
                    "a story package target requires a compendium target".into(),
                ));
            }
            let mode = parse_mode(&mode)?;
            let (prepared, manifest, plans) =
                build::build_with_mode(root, &config, story.as_deref(), mode)?;
            print_report(
                format,
                "build",
                build_commands::BuildOutput {
                    wrote_manifest_revision: manifest.map(|value| value.revision),
                    plans: plans
                        .iter()
                        .map(|plan| format!("{}:v{:03}", plan.story_id, plan.revision))
                        .collect(),
                    text_exports: crate::text::export_paths(root, &config, &prepared.project)
                        .into_iter()
                        .map(|path| relative_path(root, &path))
                        .collect(),
                    changes: prepared.changes,
                },
                prepared.validation,
            )
        }
        Command::Plan { story, mode } => {
            let mode = parse_mode(&mode)?;
            let (prepared, manifest, plans) =
                build::build_with_mode(root, &config, Some(&story), mode)?;
            print_report(
                format,
                "plan",
                build_commands::BuildOutput {
                    wrote_manifest_revision: manifest.map(|value| value.revision),
                    plans: plans
                        .iter()
                        .map(|plan| format!("{}:v{:03}", plan.story_id, plan.revision))
                        .collect(),
                    text_exports: crate::text::export_paths(root, &config, &prepared.project)
                        .into_iter()
                        .map(|path| relative_path(root, &path))
                        .collect(),
                    changes: prepared.changes,
                },
                prepared.validation,
            )
        }
        Command::Proof { story } => {
            proof_command::proof_command(root, &config, format, story.as_deref())
        }
        Command::Art { command } => art_commands::art_command(root, &config, format, command),
        Command::Approve { kind, id, revision } => {
            let revision = parse_revision(&revision)?;
            let file = match kind {
                ApprovalKind::Plan => storage::approve_plan(root, &config, &id, revision)?,
            };
            print_report(format, "approve", file, ValidationReport::default())
        }
        Command::Inspect { .. }
        | Command::ValidateFlow { .. }
        | Command::ValidateComposition { .. }
        | Command::Diagnose { .. }
        | Command::Reconcile { .. } => unreachable!(),
        Command::Resolve { old_id, new_id } => {
            let mut resolutions = storage::load_resolutions(root, &config)?;
            resolutions.mappings.insert(old_id.clone(), new_id.clone());
            storage::save_resolutions(root, &config, &resolutions)?;
            print_report(format, "resolve", resolutions, ValidationReport::default())
        }
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

#[derive(Debug, Serialize)]
struct PlanDiffOutput {
    diff: crate::diff::PlanDiff,
    path: String,
}

fn relative_path(root: &std::path::Path, path: &std::path::Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn parse_revision(value: &str) -> Result<u64, AppError> {
    value
        .strip_prefix('v')
        .unwrap_or(value)
        .parse()
        .map_err(|_| AppError::Command(format!("invalid revision `{value}`; use v001")))
}

fn validate_approved_asset(
    root: &std::path::Path,
    config: &Config,
    path: &std::path::Path,
) -> Result<String, AppError> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    if !candidate.is_file() {
        return Err(AppError::Command(format!(
            "approved artwork does not exist: {}",
            candidate.display()
        )));
    }
    let approved = root
        .join(&config.assets.approved_directory)
        .canonicalize()?;
    let candidate = candidate.canonicalize()?;
    if !candidate.starts_with(&approved) {
        return Err(AppError::Command(format!(
            "approved artwork must be inside {}",
            approved.display()
        )));
    }
    candidate
        .strip_prefix(root.canonicalize()?)
        .map(|value| value.to_string_lossy().replace('\\', "/"))
        .map_err(|_| AppError::Command("approved artwork must be inside the project root".into()))
}

fn set_art_relationship(
    root: &std::path::Path,
    config: &Config,
    art_id: &str,
    brief: Option<String>,
    artwork: Option<String>,
) -> Result<(), AppError> {
    let mut manifest = storage::load_manifest(root, config)?
        .ok_or_else(|| AppError::Command("no manifest exists; run build first".into()))?;
    let unit = manifest
        .stories
        .values_mut()
        .flat_map(|story| story.units.iter_mut())
        .find(|unit| unit.id == art_id)
        .ok_or_else(|| AppError::Command(format!("unknown art `{art_id}`")))?;
    if config.markdown.require_anchor_before_approval && unit.anchor.is_none() {
        return Err(AppError::Blocking(format!(
            "artwork relationship `{art_id}` requires an explicit anchor"
        )));
    }
    if let Some(brief) = brief {
        unit.art_brief = Some(brief);
    }
    if let Some(artwork) = artwork {
        unit.approved_art = Some(artwork);
    }
    manifest.revision += 1;
    storage::save_manifest(root, config, &manifest)
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
struct BuildOutput {
    wrote_manifest_revision: Option<u64>,
    plans: Vec<String>,
    text_exports: Vec<String>,
    changes: ChangeSet,
}

#[allow(dead_code)]
fn init(root: &std::path::Path, force: bool, format: OutputFormat) -> Result<(), AppError> {
    let config_path = root.join("compositor.toml");
    let readme_path = root.join("README.md");
    if (config_path.exists() || readme_path.exists()) && !force {
        return Err(AppError::Command(format!(
            "{} or {} already exists; use --force to replace generated project files",
            config_path.display(),
            readme_path.display()
        )));
    }
    fs::create_dir_all(root.join("compendiums"))?;
    fs::create_dir_all(root.join("canon"))?;
    fs::create_dir_all(root.join("assets/references"))?;
    fs::create_dir_all(root.join("assets/drafts"))?;
    fs::create_dir_all(root.join("assets/approved"))?;
    fs::create_dir_all(root.join(".compositor"))?;
    fs::create_dir_all(root.join(".compositor/state"))?;
    fs::create_dir_all(root.join(".compositor/plans"))?;
    fs::create_dir_all(root.join(".compositor/requirements"))?;
    fs::create_dir_all(root.join(".compositor/layouts"))?;
    fs::create_dir_all(root.join(".compositor/history"))?;
    fs::create_dir_all(root.join(".compositor/locks"))?;
    fs::create_dir_all(root.join("output/reports"))?;
    fs::create_dir_all(root.join("output/proofs"))?;
    fs::create_dir_all(root.join("output/text"))?;
    fs::write(config_path, DEFAULT_CONFIG)?;
    fs::write(readme_path, PROJECT_README)?;
    print_report(
        format,
        "init",
        InitOutput {
            root: root.display().to_string(),
        },
        ValidationReport::default(),
    )
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
struct InitOutput {
    root: String,
}

#[allow(dead_code)]
const PROJECT_README: &str = r#"# Compositor project

This directory is a Compositor project. Write stories in Markdown, then use
`compositor build` to create deterministic production state and page proofs.

## Directory guide

- `compendiums/` contains the authored books. Each numbered compendium directory
  has an `index.md` plus numbered story Markdown files. Filename order controls
  reading order; front-matter `id` values provide stable identities.
- `canon/` holds optional continuity and style notes for authors and assistants.
  Compositor leaves these Markdown files unchanged.
- `assets/references/` stores source reference material; `assets/drafts/` holds
  in-progress art; `assets/approved/` holds artwork you want to preserve.
- `.compositor/` is generated state: the current manifest, immutable history,
  page-plan and illustration-requirement revisions, and
  any manual identity resolutions. Do not edit it by hand.
- `output/reports/`, `output/proofs/`, and `output/text/` contain generated
  review and layout artifacts. HTML proofs are written to `output/proofs/`;
  plain-text files for import into a layout application are written to
  `output/text/`. Do not edit generated output as manuscript source.

## Authoring a story

Each story needs YAML front matter with a stable `id` and `title`:

```markdown
---
id: the-hidden-shelf
title: The Hidden Shelf
---

Edgar woke before the castle bells.

---

<!-- anchor: hidden-shelf-reveal -->
He found the hidden shelf behind the tapestry.
```

A top-level `---` creates a content unit. Use anchors for units that need a
stable external relationship, such as artwork. Optional production directives
include `art`, `layout`, `keep-with-next`, and `unit`; see `compositor --help`.

## Normal workflow

1. Run `compositor validate` after adding or editing stories.
2. Run `compositor build --mode conservative` to retain unaffected page
   assignments. Use `rebalance` or `fresh` only when you explicitly want a
   complete candidate repagination. It also refreshes plain-text layout exports
   in `output/text/`.
3. Run `compositor status --format json` for a machine-readable change report.
4. Run `compositor proof` to generate HTML proofs.

Art directives and artwork-oriented layouts create an illustration requirement.
Create a matching skill-authored YAML record at `art/briefs/<art-id>.yaml`; it
contains the generation prompt, candidates, feedback, and selection. Run
`compositor art validate --strict` before generation or promotion. Use
`compositor art list`, `compositor art inspect <art-id>`, and `compositor art
brief <art-id>` to review artwork state. Promote a selected candidate with
`compositor art attach <art-id> --selected`; source files and approved artifacts
are never edited in place by Compositor.

Use `compositor resolve <old-id> <new-id>` only to record a deliberate manual
identity match that Compositor could not determine automatically.

## Pagination capacity

The `[pagination]` settings in `compositor.toml` determine text-page packing.
`target_words_per_text_page` is the preferred density; long text units are
automatically split at the nearest paragraph or sentence boundary within the
maximum. A word-boundary split is used only when a single sentence exceeds the
maximum.
`maximum_words_per_text_page` is the hard cap when combining fragments or
units (except an explicit `keep-with-next` constraint, which emits a warning).
A change to either setting (or the recto setting) creates a new page-plan
revision on the next build without rewriting an unchanged source manifest.
"#;

#[allow(dead_code)]
fn proof_command(
    root: &std::path::Path,
    config: &Config,
    format: OutputFormat,
    story_filter: Option<&str>,
) -> Result<(), AppError> {
    let manifest = storage::load_manifest(root, config)?
        .ok_or_else(|| AppError::Command("no manifest exists; run build first".into()))?;
    let project = discover(root, config)?;
    let mut paths = Vec::new();
    let mut compendium_indexes = Vec::new();
    for compendium in project.compendiums {
        let mut compendium_stories = Vec::new();
        for story in compendium.stories {
            if story_filter.is_some_and(|filter| filter != story.id) {
                continue;
            }
            let plan = storage::load_active_plan(root, config, &story.id)?
                .or(storage::load_latest_plan(root, config, &story.id)?)
                .ok_or_else(|| {
                    AppError::Command(format!("no plan for {}; run build first", story.id))
                })?;
            let manifest_story = manifest.stories.get(&story.id).ok_or_else(|| {
                AppError::Command(format!("story {} absent from manifest", story.id))
            })?;
            let path = config
                .output_dir(root)
                .join("proofs")
                .join(format!("{}.html", story.id));
            fs::create_dir_all(path.parent().expect("proof parent"))?;
            fs::write(&path, proof::render_html(&story, &plan, manifest_story))?;
            compendium_stories.push((story.title.clone(), format!("{}.html", story.id)));
            paths.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
        if story_filter.is_none() && !compendium_stories.is_empty() {
            let path = config
                .output_dir(root)
                .join("proofs")
                .join(format!("{}-compendium.html", compendium.id));
            fs::write(
                &path,
                proof::render_compendium_html(&compendium.title, &compendium_stories),
            )?;
            compendium_indexes.push(relative_path(root, &path));
        }
    }
    if paths.is_empty() {
        return Err(AppError::Command("no matching stories to prove".into()));
    }
    print_report(
        format,
        "proof",
        ProofOutput {
            stories: paths,
            compendium_indexes,
        },
        ValidationReport::default(),
    )
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
struct ProofOutput {
    stories: Vec<String>,
    compendium_indexes: Vec<String>,
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

fn reconcile_composition(
    composition_path: &std::path::Path,
    overrides_path: &std::path::Path,
    output: &std::path::Path,
    format: OutputFormat,
) -> Result<(), AppError> {
    let plan = composition::load_plan(composition_path)?;
    let values = overrides::load(overrides_path)?;
    let (resolved, report) = overrides::reconcile(&plan, &values);
    if report.can_proceed() {
        overrides::write(output, &resolved)?;
    }
    print_report(format, "reconcile", resolved, report.clone())?;
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
    policy: &'a str,
    strict_art: bool,
    format: OutputFormat,
}

fn build_conventional_packages(
    root: &Path,
    config: &Config,
    request: ConventionalPackageBuildRequest<'_>,
) -> Result<(), AppError> {
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
        package::PackagePolicy {
            minimum,
            strict: strict_art,
        },
    )?;
    Ok(validation)
}

fn inspect_art(
    root: &std::path::Path,
    config: &Config,
    format: OutputFormat,
    art_id: &str,
) -> Result<(), AppError> {
    let requirement = art_commands::required_art_requirement(root, config, art_id)?;
    let manifest = storage::load_manifest(root, config)?;
    let unit = manifest
        .as_ref()
        .and_then(|manifest| manifest.stories.get(&requirement.story_id))
        .and_then(|story| story.units.iter().find(|unit| unit.id == art_id));
    let brief = art_brief::inspect(root, config, art_id);
    print_report(
        format,
        "art inspect",
        serde_json::json!({ "requirement": requirement, "unit": unit, "brief": brief }),
        ValidationReport::default(),
    )
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

fn parse_mode(mode: &str) -> Result<build::BuildMode, AppError> {
    match mode {
        "conservative" => Ok(build::BuildMode::Conservative),
        "rebalance" => Ok(build::BuildMode::Rebalance),
        "fresh" => Ok(build::BuildMode::Fresh),
        _ => Err(AppError::Command(format!("unknown build mode `{mode}`"))),
    }
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
