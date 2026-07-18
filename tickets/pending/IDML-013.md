---
id: IDML-013
title: Content modification API
depends_on: [IDML-012]
parallel_group: package
owned_paths: crates/idml/src/modification/**; crates/idml/tests/modification.rs
commit_subject: "feat(idml): port package modification API"
---

# IDML-013: Content modification API

Port add, update, and remove operations for stories, text frames, and rectangles with duplicate and not-found handling, validation, cleanup hooks, cache invalidation, and all modification scenarios.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

