---
id: IDML-014
title: Resource discovery, validation, and auto-resolution
depends_on: [IDML-013]
parallel_group: package
owned_paths: crates/idml/src/resource_manager/validation.rs; crates/idml/src/resource_manager/auto_resolution.rs; crates/idml/tests/resource_validation.rs
commit_subject: "feat(idml): port resource validation"
---

# IDML-014: Resource discovery, validation, and auto-resolution

Port missing-resource discovery, usage reports, validation options and errors, built-in filtering, font and color tracking, auto-resolution, and assigned tests.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

