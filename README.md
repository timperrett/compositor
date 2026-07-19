# Compositor

Compositor is a deterministic Rust CLI for turning Markdown story manuscripts
into incrementally maintained book-production artifacts.

## Quick start

```bash
cargo run -- init
# Add compendiums/01-example/index.md and numbered story directories containing story.md.
cargo run -- build --format json
cargo run -- proof
```

Stories use YAML front matter with `id` and `title`. A top-level Markdown
thematic break creates a content unit. Production directives use HTML comments,
for example `<!-- anchor: story-opening -->` or `<!-- layout: full-page -->`.

## Commands

`init`, `parse`, `validate`, `status`, `build`, `diff source`, `plan`, `proof`,
`inspect <story.md>`, `source sync`, `source resolve`, `validate-flow`, and `resolve` are available. `build` and `plan` currently support
the conservative mode only. Use `--format json` for stable machine-readable
reports.

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

Generated state lives in `.compositor/`; HTML proofs are written to
`output/proofs/`; layout-ready plain-text exports are written to `output/text/`
on every successful build. Both story-level and compendium-level `.txt` files
are fully generated for import into a layout application. Normal commands never
modify source Markdown or assets.

## Artwork records

Artwork intent, generation prompts, candidates, feedback, and selections live
in human- and skill-authored YAML files at `art/briefs/<art-id>.yaml`.
Compositor validates these records against the current illustration requirement
but does not generate or revise them. See `docs/art-protocol.md` for the v1
format and complete examples. Use `compositor art validate --strict` before
generation or promotion, and `compositor art attach <art-id> --selected` to
copy the selected draft candidate into `assets/approved/`.
