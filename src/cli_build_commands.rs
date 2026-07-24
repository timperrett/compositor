use super::*;

pub(super) fn init(
    root: &std::path::Path,
    force: bool,
    format: OutputFormat,
) -> Result<(), AppError> {
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

This directory is a Compositor project. Author Markdown, then maintain Flow and
Composition Plans and build a deterministic delivery package.

## Directory guide

- `compendiums/` contains the authored books. Each numbered compendium directory
  has an `index.md` plus numbered story Markdown files. Filename order controls
  reading order; front-matter `id` values provide stable identities.
- `canon/` holds optional continuity and style notes for authors and assistants.
  Compositor leaves these Markdown files unchanged.
- `assets/references/` stores source reference material; `assets/drafts/` holds
  in-progress art; `assets/approved/` holds artwork you want to preserve.
- `art/briefs/` keeps creative intent, candidates, and feedback. `art/assets.yaml`
  is the sole lifecycle record for selected and approved files.
- `output/packages/` contains generated delivery packages. Each package includes
  an HTML assembly guide; do not edit generated output as manuscript source.

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

## Production workflow

1. Run `compositor source sync story.md --write`, then update the sibling Flow
   Plan and Composition Plan.
2. Validate with `compositor validate-flow`, `compositor validate-composition`,
   and `compositor art validate`.
3. Register candidates in a brief, then `art select`, `art review`, and
   `art approve` when an approved asset is required.
4. Run `compositor build <compendium> [story]`. Its `assembly-guide.html` is
   the review surface and uses the same Flow, Composition, and art resolution
   as the package.

## Package builds

Build a production package by naming a compendium and, optionally, one story:

```bash
compositor build <compendium-id-or-directory>
compositor build <compendium-id-or-directory> <story-id-or-directory>
```

For each selected story, Compositor uses the conventional sibling files
`story.md`, `story.flow.yaml`, and `hardcover.composition.yaml`, the project
art registry `art/assets.yaml`, and the design system named by the composition
plan at `design-systems/<design-system-id>`. Packages are written under
`output/packages/<compendium-id>/rNN/<story-directory>/`; `rNN` is allocated
automatically for every build invocation.

Existing `.compositor/` production state is deliberately unsupported. Preserve
it in version control, remove it manually, and rebuild from the plans; Compositor
will never migrate or delete it automatically.
"#;
