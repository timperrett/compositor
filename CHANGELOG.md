# Changelog

## 0.2.0 - Flow/Composition production

Breaking release. Production is now Flow/Composition-only.

- Removed generated page plans, manifests, pagination state, legacy review
  rendering, planning commands, and legacy production flags.
- Made `art/assets.yaml` schema v2 the sole lifecycle registry and art briefs
  schema v3 creative records.
- Consolidated artwork lifecycle into `select`, `review`, and `approve`, with
  SHA-256-pinned candidates and approved files.
- Package validation now checks current Flow/Composition mappings, lifecycle
  policy, file hashes, and computed artwork geometry.

Existing `.compositor` state is unsupported and must be removed manually before
rebuilding with Flow and Composition plans.
