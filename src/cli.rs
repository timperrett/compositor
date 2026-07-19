use crate::art_brief;
use crate::build;
use crate::config::{Config, DEFAULT_CONFIG};
use crate::discovery::discover;
use crate::flow;
use crate::model::{ChangeSet, ValidationReport};
use crate::proof;
use crate::report::{self, OutputFormat};
use crate::storage;
use crate::validation;
use crate::{project_root, AppError};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

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
    Status,
    Build {
        #[arg(long, default_value = "conservative")]
        mode: String,
        #[arg(long)]
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
    ValidateFlow {
        story: PathBuf,
        flow: PathBuf,
        #[arg(long)]
        design_system: PathBuf,
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
#[derive(Debug, Clone, clap::ValueEnum)]
enum ApprovalKind {
    Plan,
}

#[derive(Debug, Subcommand)]
enum ArtCommand {
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
        Command::ValidateFlow {
            story,
            flow: flow_path,
            design_system,
        } => validate_flow(&story, &flow_path, &design_system, cli.format),
        command => execute(&root, cli.format, command),
    }
}

fn execute(root: &std::path::Path, format: OutputFormat, command: Command) -> Result<(), AppError> {
    let config = Config::load(root)?;
    match command {
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
        Command::Build { mode, story } => {
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
        Command::Inspect { .. } | Command::ValidateFlow { .. } => unreachable!(),
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
