---
id: IDML-001
title: Standalone crate, provenance, fixtures, and parity harness
depends_on: []
parallel_group: exclusive
owned_paths: Cargo.toml; Cargo.lock; crates/idml/Cargo.toml; crates/idml/LICENSE-UPSTREAM; crates/idml/src/lib.rs; crates/idml/src/*/mod.rs; crates/idml/tests/fixtures/**; crates/idml/tests/parity_manifest.rs
commit_subject: "feat(idml): scaffold standalone IDML crate"
---

# IDML-001: Standalone crate, provenance, fixtures, and parity harness

Scaffold the independent workspace crate, freeze dependencies, copy the 17 ledger fixtures, add empty domain modules and a machine-checkable parity and fixture harness.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

