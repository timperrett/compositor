# Compositor

Compositor is a deterministic Rust CLI for turning Markdown story manuscripts
into incrementally maintained book-production artifacts.

## Quick start

```bash
compositor init
# Add compendiums/01-example/index.md and numbered story directories containing story.md.
compositor build example-compendium --format json
```

Stories use YAML front matter with `id` and `title`. A top-level Markdown
thematic break creates a content unit. Production directives use HTML comments,
for example `<!-- anchor: story-opening -->` or `<!-- layout: full-page -->`.

## Commands

`init`, `parse`, `validate`, `tree`, `build`, `inspect <story.md>`,
`source sync`, `source resolve`, and `validate-flow` are available. Use
`--format json` for stable machine-readable
reports.

`tree` prints the ordered compendium and story catalog as `title [id]`, which
makes story IDs easy to find. Add `--art` to nest art IDs from the corresponding
story briefs beneath each story. Use `compositor tree <story-id> --spreads` to
list every Flow spread for one story, with separately labelled mappings from
art briefs and its conventional `hardcover.composition.yaml`.

`inspect <story.md>` reports durable prose-paragraph identifiers and the source
revision needed by a Story Flow Plan. `validate-flow <story.md> <story.flow.yaml>
--design-system <directory>` validates source coverage, declared narrative
roles, energy, and pacing without changing the manuscript.

For a Flow-Plan-ready story, keep `story.md` as clean prose. Run
`source sync story.md --write` to create or update the committed sibling
`story.paragraphs.yaml` ledger and generated `story.annotated.md` review view.
The ledger owns paragraph IDs; the annotated view is derived and must not be
edited. If a substantive paragraph edit cannot be matched unambiguously, make
an explicit editorial decision and use `source resolve` to rebind the approved
existing ID before syncing again.

Composition plans use `compositor.dev/composition-plan/v2`. Each plan has a
separate `opener` with the exact story title, `placement: center-page`, and a
single `usage: opener` art record. Narrative `spreads` are distinct and may
reference only `usage: story` art. Package builds emit the opener under
`opener/` and never treat it as `spread-001`.

Use `compositor art coverage --story <story-id> --edition <edition> --format json`
to inspect the opener separately and identify each narrative spread as covered,
missing, or invalid. Story art referenced by a
Composition Plan must declare that spread in `source.spread_ids`.

## Package builds

Build delivery packages by naming a compendium, with an optional story target:

```bash
# Every story in the compendium.
compositor build door-between-worlds

# One story in that compendium.
compositor build door-between-worlds world-of-lantern-tides
```

Targets may be either authored IDs or their directory names. For each selected
story, Compositor reads the conventional sibling files `story.md`,
`story.flow.yaml`, and `hardcover.composition.yaml`; it reads the standard art
registry at `art/assets.yaml`; and it derives the design-system directory from
the composition plan (`design-systems/<design-system-id>`). `--design-system`
and `--assets` remain available as explicit overrides.

Packages are written to
`output/packages/<compendium-id>/rNN/<story-directory>/`. The revision is
allocated automatically (`r01`, then `r02`, and so on); a multi-story build
shares one revision. Use `--output` only when a single story needs a
non-conventional destination.

Every package includes `assembly-guide.html`, an HTML review surface for the
resolved opener and spreads. Existing `.compositor/` state is unsupported: keep
it in version control, remove it manually, and rebuild from the Flow and
Composition Plans. Normal commands never modify source Markdown or assets.

## One-time legacy migration

For an existing project, use the standalone bridge before removing legacy state:

```bash
bash scripts/migrate-legacy-production-state --root /path/to/project
bash scripts/migrate-legacy-production-state --root /path/to/project --apply
```

The first command is a dry run and prints the complete mapping report. `--apply`
only imports unambiguous, current Flow/Composition-linked artwork; it upgrades
briefs, records verified selections as `review`, copies verified historical
approvals into `assets/approved/`, and writes a receipt to
`output/reports/legacy-production-migration.json`. It never deletes, renames,
or archives `.compositor/`; review the receipt and remove that directory manually.

## Artwork records

Artwork intent, generation prompts, candidates, feedback, and selections live
in human- and skill-authored YAML files at `art/briefs/<art-id>.yaml`.
Compositor validates these records against the current Flow/Composition
requirement but does not generate or revise them. See `docs/art-protocol.md`
for the v3 format and complete examples. Use `compositor art validate --strict`
before generation or promotion, then explicitly `select`, `review`, and
`approve` a candidate to copy the pinned asset into `assets/approved/`.
