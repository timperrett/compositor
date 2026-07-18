---
id: IDML-015
title: Orphan detection and cleanup
depends_on: [IDML-014]
parallel_group: package
owned_paths: crates/idml/src/resource_manager/**; crates/idml/tests/resource_cleanup.rs
commit_subject: "feat(idml): port resource cleanup"
---

# IDML-015: Orphan detection and cleanup

Port orphan detection, dry-run and selective cleanup, hierarchy and reference protection, cleanup reports, round trips, and cleanup property tests.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

