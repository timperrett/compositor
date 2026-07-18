---
id: IDML-012
title: Page-item index and selection
depends_on: [IDML-011]
parallel_group: package
owned_paths: crates/idml/src/selection/**; crates/idml/tests/selection.rs; crates/idml/benches/selection.rs
commit_subject: "feat(idml): add indexed page-item selection"
---

# IDML-012: Page-item index and selection

Port lazy O(1) indexes, typed and multi-ID lookup, owned heterogeneous selections, invalidation, concurrency tests, and selection benchmarks.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

