---
id: IDML-009
title: Secure IDML package read/write and caching
depends_on: [IDML-007, IDML-008]
parallel_group: package
owned_paths: crates/idml/src/package/**; crates/idml/src/paths.rs; crates/idml/tests/package_io.rs
commit_subject: "feat(idml): port IDML package IO"
---

# IDML-009: Secure IDML package read/write and caching

Port ordered secure ZIP IO, read options, paths, raw files, lazy caches, metadata, writer invariants, structural round trips, examples, benchmarks, and assigned tests.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

