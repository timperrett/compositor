# Art brief protocol v3

An art brief is one YAML record at `art/briefs/<art-id>.yaml`. It is the
single, local source for a skill's image-generation prompt, candidates,
feedback, and chosen direction. Compositor owns the matching illustration
requirement; it validates the record but never writes it.

The normative machine schema is
[`schemas/art-brief-v2.schema.json`](../schemas/art-brief-v2.schema.json).
Compositor accepts YAML and rejects unknown fields, unsafe paths, invalid
candidate images, or a record that does not resolve to the current requirement.

## Required fields

`schema_version` is always `3`. `art_id` must match its durable anchor.
`source` identifies the story and authored `anchor_id`. Narrative artwork that
is placed in a Composition Plan also declares ordered `source.spread_ids`.
Those IDs must include every narrative spread that references the record.
Legacy records without `spread_ids` remain readable until mapped. Opener art
uses `usage: opener`, has no `spread_ids`, and may only appear in the plan's
separate `opener` section.

`generation.prompt` is the canonical image-generation request. It can be a
direct, exploratory prompt: authors do not need to formalize a visual brief
before looking at candidates. `generation.mode` defaults to `exploration`.
`generation.page_treatment` is required. Use `floating` for art that fades
into an otherwise unpainted page, `framed` for rectangular art enclosed by a
thin hand-painted medium-black keyline inside a white margin, `spot` for a
compact isolated subject grouping on an otherwise clean white page, or
`full-bleed` for art that deliberately fills the complete printed frame.

All `file` and `canon_references` paths are project-relative. Candidates must
be existing PNG, JPG, JPEG, or WebP files. A selected candidate must be listed
in `candidates`.

## Typical workflow

1. Prepare the source, then create a Flow Plan and Composition Plan.
2. Run `compositor art coverage --story <story-id> --edition <edition> --format json`.
3. The specification skill writes or updates `art/briefs/<art-id>.yaml`, adds
   `source.spread_ids` for narrative art, registers it, and updates the matching
   Composition Plan reference.
4. Run Flow, Composition, coverage, and strict art validation.
5. The rendering skill writes candidate files and adds them to that record.
6. Run `compositor art select <art-id> <candidate-id>`, then `compositor art review <art-id>`.
7. Run `compositor art approve <art-id>` to validate the reviewed selection,
   copy it into `assets/approved/`, and pin its SHA-256 in `art/assets.yaml`.

## Examples

See [`art-protocol-examples`](art-protocol-examples/) for complete copyable
records: a minimal exploration, Edgar's library discovery, candidate feedback,
a selected candidate, and a stale requirement warning.

There is no `render-prompt.md` fallback. Legacy records without spread links
remain readable only so they can be mapped deliberately; `art coverage` reports
them as `needs-mapping` instead of assigning them automatically.
