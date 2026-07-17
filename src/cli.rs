use crate::build;
use crate::config::{Config, DEFAULT_CONFIG};
use crate::discovery::discover;
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
        kind: InspectKind,
        id: String,
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
enum InspectKind {
    Story,
    Unit,
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
    Approve {
        art_id: String,
        revision: String,
    },
    Attach {
        art_id: String,
        path: PathBuf,
    },
}

pub fn run() -> Result<(), AppError> {
    let cli = Cli::parse();
    let root = project_root(cli.root)?;
    match cli.command {
        Command::Init { force } => init(&root, force, cli.format),
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
            let status = status_output(root, &config, &prepared)?;
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
                BuildOutput {
                    wrote_manifest_revision: manifest.map(|value| value.revision),
                    plans: plans
                        .iter()
                        .map(|plan| format!("{}:v{:03}", plan.story_id, plan.revision))
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
                BuildOutput {
                    wrote_manifest_revision: manifest.map(|value| value.revision),
                    plans: plans
                        .iter()
                        .map(|plan| format!("{}:v{:03}", plan.story_id, plan.revision))
                        .collect(),
                    changes: prepared.changes,
                },
                prepared.validation,
            )
        }
        Command::Proof { story } => proof_command(root, &config, format, story.as_deref()),
        Command::Art { command } => art_command(root, &config, format, command),
        Command::Approve { kind, id, revision } => {
            let revision = parse_revision(&revision)?;
            let file = match kind {
                ApprovalKind::Plan => storage::approve_plan(root, &config, &id, revision)?,
            };
            print_report(format, "approve", file, ValidationReport::default())
        }
        Command::Inspect { kind, id } => inspect(root, &config, format, kind, &id),
        Command::Resolve { old_id, new_id } => {
            let mut resolutions = storage::load_resolutions(root, &config)?;
            resolutions.mappings.insert(old_id.clone(), new_id.clone());
            storage::save_resolutions(root, &config, &resolutions)?;
            print_report(format, "resolve", resolutions, ValidationReport::default())
        }
        Command::Init { .. } => unreachable!(),
    }
}

#[derive(Debug, Serialize)]
struct ArtListItem {
    art_id: String,
    story_id: String,
    pages: Vec<u32>,
    layout: String,
    requirement_revision: u64,
    requirement_status: crate::model::ArtifactStatus,
    candidate_briefs: Vec<String>,
    approved_brief: Option<String>,
    approved_artwork: Option<String>,
}

#[derive(Debug, Serialize)]
struct ArtBriefOutput {
    art_id: String,
    requirement_revision: u64,
    candidate_briefs: Vec<String>,
    approved_brief: Option<String>,
}

fn art_command(
    root: &std::path::Path,
    config: &Config,
    format: OutputFormat,
    command: ArtCommand,
) -> Result<(), AppError> {
    match command {
        ArtCommand::List { story } => {
            let project = discover(root, config)?;
            if let Some(story_id) = story.as_deref() {
                let exists = project
                    .compendiums
                    .iter()
                    .flat_map(|compendium| &compendium.stories)
                    .any(|candidate| candidate.id == story_id);
                if !exists {
                    return Err(AppError::Command(format!("unknown story `{story_id}`")));
                }
            }
            let manifest = storage::load_manifest(root, config)?;
            let mut records = Vec::new();
            for compendium in &project.compendiums {
                for source_story in &compendium.stories {
                    if story.as_deref().is_some_and(|id| id != source_story.id) {
                        continue;
                    }
                    for (art_id, requirement) in
                        crate::art::requirements_for_story(root, config, &source_story.id)?
                    {
                        let brief = art_brief_output(root, config, &art_id, &requirement)?;
                        let unit = manifest.as_ref().and_then(|manifest| {
                            manifest
                                .stories
                                .get(&source_story.id)
                                .and_then(|story| story.units.iter().find(|unit| unit.id == art_id))
                        });
                        records.push(ArtListItem {
                            art_id,
                            story_id: requirement.story_id,
                            pages: requirement.pages,
                            layout: requirement.layout,
                            requirement_revision: requirement.revision,
                            requirement_status: requirement.status,
                            candidate_briefs: brief.candidate_briefs,
                            approved_brief: brief.approved_brief,
                            approved_artwork: unit.and_then(|unit| unit.approved_art.clone()),
                        });
                    }
                }
            }
            print_report(format, "art list", records, ValidationReport::default())
        }
        ArtCommand::Inspect { art_id } => inspect_art(root, config, format, &art_id),
        ArtCommand::Brief { art_id } => {
            let requirement = required_art_requirement(root, config, &art_id)?;
            let output = art_brief_output(root, config, &art_id, &requirement)?;
            print_report(format, "art brief", output, ValidationReport::default())
        }
        ArtCommand::Approve { art_id, revision } => {
            let revision = parse_revision(&revision)?;
            required_art_requirement(root, config, &art_id)?;
            let file = storage::approve_brief(root, config, &art_id, revision)?;
            set_art_relationship(
                root,
                config,
                &art_id,
                Some(format!("briefs/{art_id}/{file}")),
                None,
            )?;
            print_report(format, "art approve", file, ValidationReport::default())
        }
        ArtCommand::Attach { art_id, path } => {
            required_art_requirement(root, config, &art_id)?;
            let relative = validate_approved_asset(root, config, &path)?;
            set_art_relationship(root, config, &art_id, None, Some(relative.clone()))?;
            print_report(format, "art attach", relative, ValidationReport::default())
        }
    }
}

fn required_art_requirement(
    root: &std::path::Path,
    config: &Config,
    art_id: &str,
) -> Result<crate::model::IllustrationRequirement, AppError> {
    storage::load_latest_requirement(root, config, art_id)?.ok_or_else(|| {
        AppError::Command(format!(
            "unknown art `{art_id}`; run build to generate its requirement"
        ))
    })
}

fn art_brief_output(
    root: &std::path::Path,
    config: &Config,
    art_id: &str,
    requirement: &crate::model::IllustrationRequirement,
) -> Result<ArtBriefOutput, AppError> {
    let directory = storage::brief_directory(root, config, art_id);
    let mut candidate_briefs = if directory.is_dir() {
        fs::read_dir(&directory)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().ends_with("-candidate.md"))
            })
            .map(|path| relative_path(root, &path))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    candidate_briefs.sort();
    let index = storage::load_artifact_index(root, config, "briefs", art_id)?;
    Ok(ArtBriefOutput {
        art_id: art_id.into(),
        requirement_revision: requirement.revision,
        candidate_briefs,
        approved_brief: index
            .active
            .map(|file| relative_path(root, &directory.join(file))),
    })
}

#[derive(Debug, Serialize)]
struct StatusOutput {
    changes: ChangeSet,
    active_plans: Vec<String>,
    candidate_plans: Vec<String>,
    missing_approved_briefs: Vec<String>,
    artwork_requirements: Vec<String>,
}

fn status_output(
    root: &std::path::Path,
    config: &Config,
    prepared: &build::PreparedBuild,
) -> Result<StatusOutput, AppError> {
    let mut active_plans = Vec::new();
    let mut candidate_plans = Vec::new();
    let mut missing_approved_briefs = Vec::new();
    let mut artwork_requirements = Vec::new();
    for compendium in &prepared.project.compendiums {
        for story in &compendium.stories {
            let plan_index = storage::load_artifact_index(root, config, "plans", &story.id)?;
            if let Some(active) = plan_index.active {
                active_plans.push(format!("{}:{active}", story.id));
            } else if let Some(plan) = storage::load_latest_plan(root, config, &story.id)? {
                candidate_plans.push(format!("{}:v{:03}", story.id, plan.revision));
            }
            for (art_id, requirement) in
                crate::art::requirements_for_story(root, config, &story.id)?
            {
                artwork_requirements.push(format!(
                    "{art_id}:v{:03}:{:?}",
                    requirement.revision, requirement.status
                ));
                if storage::load_artifact_index(root, config, "briefs", &art_id)?
                    .active
                    .is_none()
                {
                    missing_approved_briefs.push(art_id);
                }
            }
        }
    }
    Ok(StatusOutput {
        changes: prepared.changes.clone(),
        active_plans,
        candidate_plans,
        missing_approved_briefs,
        artwork_requirements,
    })
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

#[derive(Debug, Serialize)]
struct BuildOutput {
    wrote_manifest_revision: Option<u64>,
    plans: Vec<String>,
    changes: ChangeSet,
}

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
    fs::create_dir_all(root.join(".compositor/briefs"))?;
    fs::create_dir_all(root.join(".compositor/layouts"))?;
    fs::create_dir_all(root.join(".compositor/history"))?;
    fs::create_dir_all(root.join(".compositor/locks"))?;
    fs::create_dir_all(root.join("output/reports"))?;
    fs::create_dir_all(root.join("output/proofs"))?;
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

#[derive(Debug, Serialize)]
struct InitOutput {
    root: String,
}

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
  page-plan and illustration-requirement revisions, candidate art briefs, and
  any manual identity resolutions. Do not edit it by hand.
- `output/reports/` and `output/proofs/` contain generated review artifacts.
  HTML proofs are written to `output/proofs/`.

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
   complete candidate repagination.
3. Run `compositor status --format json` for a machine-readable change report.
4. Run `compositor proof` to generate HTML proofs.

Art directives and artwork-oriented layouts create an illustration requirement
plus a candidate Markdown brief at `.compositor/briefs/<art-id>/`. Review or
edit that candidate, then use `compositor art approve <art-id> v001`.
Use `compositor art list`, `compositor art inspect <art-id>`, and
`compositor art brief <art-id>` to review artwork state. Link an approved asset
with `compositor art attach <art-id> assets/approved/<file>`; source files and
approved artifacts are never edited by Compositor.

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

#[derive(Debug, Serialize)]
struct ProofOutput {
    stories: Vec<String>,
    compendium_indexes: Vec<String>,
}

fn inspect(
    root: &std::path::Path,
    config: &Config,
    format: OutputFormat,
    kind: InspectKind,
    id: &str,
) -> Result<(), AppError> {
    let manifest = storage::load_manifest(root, config)?
        .ok_or_else(|| AppError::Command("no manifest exists; run build first".into()))?;
    let value = match kind {
        InspectKind::Story => serde_json::to_value(
            manifest
                .stories
                .get(id)
                .ok_or_else(|| AppError::Command(format!("unknown story `{id}`")))?,
        ),
        InspectKind::Unit => serde_json::to_value(
            manifest
                .stories
                .values()
                .flat_map(|story| story.units.iter())
                .find(|unit| unit.id == id)
                .ok_or_else(|| AppError::Command(format!("unknown unit `{id}`")))?,
        ),
    }
    .map_err(|error| AppError::Serialization(error.to_string()))?;
    print_report(format, "inspect", value, ValidationReport::default())
}

fn inspect_art(
    root: &std::path::Path,
    config: &Config,
    format: OutputFormat,
    art_id: &str,
) -> Result<(), AppError> {
    let requirement = required_art_requirement(root, config, art_id)?;
    let manifest = storage::load_manifest(root, config)?;
    let unit = manifest
        .as_ref()
        .and_then(|manifest| manifest.stories.get(&requirement.story_id))
        .and_then(|story| story.units.iter().find(|unit| unit.id == art_id));
    print_report(
        format,
        "art inspect",
        serde_json::json!({ "requirement": requirement, "unit": unit }),
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
