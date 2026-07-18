---
id: IDML-010
title: Template creation and layout milestone
depends_on: [IDML-009]
parallel_group: package
owned_paths: crates/idml/src/template/**; crates/idml/tests/templates.rs
commit_subject: "feat(idml): add template-based document creation"
---

# IDML-010: Template creation and layout milestone

Embed the minimal templates and port presets, dimensions, orientation, margins, columns, master-spread generation, archive invariants, and template tests.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

