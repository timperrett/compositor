---
id: IDML-011
title: Typed accessors, resources, fonts, and metadata
depends_on: [IDML-010]
parallel_group: package
owned_paths: crates/idml/src/access/**; crates/idml/tests/accessors.rs
commit_subject: "feat(idml): port typed package accessors"
---

# IDML-011: Typed accessors, resources, fonts, and metadata

Port typed collection access, setters, font lookup, style hierarchy, metadata and XMP persistence, cache statistics, accessor traits, mocks, examples, and tests.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

