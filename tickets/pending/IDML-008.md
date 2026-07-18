---
id: IDML-008
title: Document and design-map model
depends_on: [IDML-003, IDML-005, IDML-006]
parallel_group: domain-d
owned_paths: crates/idml/src/document/**; crates/idml/tests/document.rs
commit_subject: "feat(idml): port document model"
---

# IDML-008: Document and design-map model

Port designmap and document types, layers, references, assignments, sections, numbering, grids, variables, metadata wrappers, parsing, marshaling, examples, and tests.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

