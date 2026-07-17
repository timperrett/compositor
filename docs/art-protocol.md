# Art brief protocol v1

An art brief is one YAML record at `art/briefs/<art-id>.yaml`. It is the
single, local source for a skill's image-generation prompt, candidates,
feedback, and chosen direction. Compositor owns the matching illustration
requirement; it validates the record but never writes it.

The normative machine schema is
[`schemas/art-brief-v1.schema.json`](../schemas/art-brief-v1.schema.json).
Compositor accepts YAML and rejects unknown fields, unsafe paths, invalid
candidate images, or a record that does not resolve to the current requirement.

## Required fields

`schema_version` is always `1`. `art_id` must equal the requirement and its
anchored unit. `source` identifies the story, unit IDs, and requirement
revision from `compositor art inspect <art-id> --format json`.

`generation.prompt` is the canonical image-generation request. It can be a
direct, exploratory prompt: authors do not need to formalize a visual brief
before looking at candidates. `generation.mode` defaults to `exploration`.

All `file` and `canon_references` paths are project-relative. Candidates must
be existing PNG, JPG, JPEG, or WebP files. A selected candidate must be listed
in `candidates`.

## Typical workflow

1. Add or retain an anchor and art directive, then run `compositor build`.
2. A skill writes or updates `art/briefs/<art-id>.yaml` with its prompt.
3. Run `compositor art validate --strict`.
4. The rendering skill writes candidate files and adds them to that record.
5. Set `selection.candidate_id` when one is wanted, optionally adding feedback.
6. Run `compositor art attach <art-id> --selected` to copy that candidate to
   `assets/approved/` and link it in the manifest.

## Examples

See [`art-protocol-examples`](art-protocol-examples/) for complete copyable
records: a minimal exploration, Edgar's library discovery, candidate feedback,
a selected candidate, and a stale requirement warning.

There is no legacy brief or `render-prompt.md` fallback. A changed requirement
revision produces a warning so the author can consciously update the record.
