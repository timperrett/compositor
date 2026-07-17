# Compositor

Compositor is a deterministic Rust CLI for turning Markdown story manuscripts
into incrementally maintained book-production artifacts.

## Quick start

```bash
cargo run -- init
# Add compendiums/01-example/index.md and numbered story Markdown files.
cargo run -- build --format json
cargo run -- proof
```

Stories use YAML front matter with `id` and `title`. A top-level Markdown
thematic break creates a content unit. Production directives use HTML comments,
for example `<!-- anchor: story-opening -->` or `<!-- layout: full-page -->`.

## Commands

`init`, `parse`, `validate`, `status`, `build`, `diff source`, `plan`, `proof`,
`inspect`, and `resolve` are available. `build` and `plan` currently support
the conservative mode only. Use `--format json` for stable machine-readable
reports.

Generated state lives in `.compositor/`; HTML proofs are written to
`output/proofs/`. Normal commands never modify source Markdown or assets.
