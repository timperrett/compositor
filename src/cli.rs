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
    about = "Deterministic Markdown-to-book production tooling"
)]
struct Cli {
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Human, global = true)]
    format: OutputFormat,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init {
        #[arg(long)]
        force: bool,
    },
    Parse {
        #[arg(long)]
        story: Option<String>,
    },
    Validate {
        #[arg(long)]
        story: Option<String>,
        #[arg(long)]
        strict: bool,
    },
    /// Display compendiums, stories, and optionally their art IDs as a tree.
    Tree {
        /// Include art IDs from story-linked art briefs.
        #[arg(long)]
        art: bool,
    },
    Status,
    Build {
        /// A compendium ID or directory name. Builds every story when omitted after this target.
        compendium: Option<String>,
        /// A story ID or directory name within the selected compendium.
        package_story: Option<String>,
        /// Override the design system derived from the composition plan.
        #[arg(long)]
        design_system: Option<PathBuf>,
        /// Override the conventional art asset registry at art/assets.yaml.
        #[arg(long)]
        assets: Option<PathBuf>,
        #[arg(long, default_value = "draft")]
        asset_policy: String,
        #[arg(long)]
        strict_art: bool,
        /// Override the generated package destination (single-story builds only).
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, default_value = "conservative")]
        mode: String,
        /// Build production state for one story ID instead of a delivery package.
        #[arg(long, conflicts_with = "compendium")]
        story: Option<String>,
    },
    Diff {
        #[command(subcommand)]
        target: DiffTarget,
    },
    Plan {
        story: String,
        #[arg(long, default_value = "conservative")]
        mode: String,
    },
    Proof {
        #[arg(long)]
        story: Option<String>,
    },
    Art {
        #[command(subcommand)]
        command: ArtCommand,
    },
    Approve {
        kind: ApprovalKind,
        id: String,
        revision: String,
    },
    Inspect {
        story: PathBuf,
    },
    Source {
        #[command(subcommand)]
        command: SourceCommand,
    },
    ValidateFlow {
        story: PathBuf,
        flow: PathBuf,
        #[arg(long)]
        design_system: PathBuf,
    },
    ValidateComposition {
        story: PathBuf,
        flow: PathBuf,
        composition: PathBuf,
        #[arg(long)]
        design_system: PathBuf,
    },
    Diagnose {
        story: PathBuf,
        flow: PathBuf,
        composition: PathBuf,
        #[arg(long)]
        design_system: PathBuf,
    },
    Reconcile {
        composition: PathBuf,
        overrides: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Resolve {
        old_id: String,
        new_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum DiffTarget {
    Source,
    Plan {
        story: String,
        before: String,
        after: String,
    },
}

#[derive(Debug, Subcommand)]
enum SourceCommand {
    Sync {
        story: PathBuf,
        #[arg(long)]
        write: bool,
    },
    Resolve {
        story: PathBuf,
        old_id: String,
        candidate_fingerprint: String,
        #[arg(long)]
        write: bool,
    },
}
#[derive(Debug, Clone, clap::ValueEnum)]
enum ApprovalKind {
    Plan,
}

#[derive(Debug, Subcommand)]
enum ArtCommand {
    Registry {
        #[arg(long)]
        write: bool,
    },
    MigrateBriefs {
        #[arg(long)]
        write: bool,
    },
    Register {
        art_id: String,
    },
    IngestCandidate {
        art_id: String,
        source: PathBuf,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        attempt: u32,
    },
    Select {
        art_id: String,
        candidate_id: String,
        #[arg(long)]
        feedback: Option<String>,
    },
    Review {
        art_id: String,
    },
    ApproveAsset {
        art_id: String,
    },
    Reject {
        art_id: String,
    },
    Supersede {
        art_id: String,
        successor: String,
    },
    List {
        #[arg(long)]
        story: Option<String>,
    },
    Inspect {
        art_id: String,
    },
    Brief {
        art_id: String,
    },
    Coverage {
        #[arg(long)]
        story: String,
        #[arg(long)]
        edition: String,
    },
    Validate {
        #[arg(long)]
        story: Option<String>,
        #[arg(long)]
        strict: bool,
    },
    Attach {
        art_id: String,
        path: Option<PathBuf>,
        #[arg(long)]
        selected: bool,
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
        Command::Tree { art } => {
            let project = discover(root, &config)?;
            tree_command(root, &project, art, format)
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
    format: OutputFormat,
) -> Result<(), AppError> {
    let art_by_story = if include_art {
        art_ids_by_story(root)?
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
