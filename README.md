# Compositor

Compositor is a deterministic, Markdown-first Rust CLI for moving story
manuscripts toward a clear, reviewable book-production handoff. It keeps the
manuscript readable, makes production decisions explicit, and creates
revisioned packages that a layout application can use without taking over the
creative work.

Its goal is to make the path from manuscript to layout legible and repeatable:
authors retain editorial control, people or AI skills can propose structured
Flow, Composition, and artwork records, and Compositor checks their links,
coverage, freshness, and production constraints. It records what was built
from which source and approved assets, so review findings can be acted on
without silently turning suggestions into decisions.

The workflow is deliberately staged: write the story in Markdown; sync it into
stable source records; draft and validate the Flow and Composition Plans; then
select, review, and approve any artwork before building a revisioned package
and assembly guide. Compositor validates and packages those decisions, while
final styling, page separation, and typesetting remain the responsibility of
the design system and layout application.

Learn more at the [Compositor project site](https://timperrett.github.io/compositor/).

## Installation

From this repository, install the supported 0.2.x CLI with `cargo install
--path .`, or build it with `cargo build --release` and use
`target/release/compositor`. Verify the installed binary with
`compositor --version`.

## Dependency licenses

Compositor is licensed under Apache-2.0. The committed
`THIRD_PARTY_NOTICES.html` records license texts for the locked dependency
graph and is included when packaging the crate. Before changing dependencies,
install `cargo-about` and `cargo-deny`, then run:

```bash
make notices
make licenses-check
```

`make notices` refreshes the notice artifact; `make licenses-check` verifies
both the allowed-license policy and that the artifact is current. `about.toml`
is the single source of truth for accepted licenses; the check derives Cargo
Deny's configuration from it.

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

Use `compositor art dashboard` to write a project-wide, read-only HTML art
review report at `output/reports/art-dashboard.html`. It groups required art by
story, shows candidates, lifecycle state, default draft-policy readiness, and
the exact next lifecycle command. `art validate` and strict package builds
remain the authoritative checks.

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

## Artwork records

Artwork intent, generation prompts, candidates, and feedback live in human- and
skill-authored YAML files at `art/briefs/<art-id>.yaml`; `art/assets.yaml` owns
the selected candidate and approved artifact. Compositor validates these records
against the current Flow/Composition requirement but does not generate or revise
them. See `docs/art-protocol.md` for the v3 format and complete examples. Use
`compositor art validate --strict` before generation or promotion, then
explicitly `select`, `review`, and `approve` a candidate to copy the pinned asset
into `assets/approved/`.

Package `manifest.yaml` records the source revision, Composition edition,
asset policy, and ordered package entries. `art/assets.yaml` is the project
lifecycle registry; package `diagnostics.yaml` records build findings; and
`assembly-guide.html` is the human review surface for the exact opener and
spreads included in that package.
