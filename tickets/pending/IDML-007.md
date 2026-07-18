---
id: IDML-007
title: XMP metadata
depends_on: [IDML-002]
parallel_group: domain-b
owned_paths: crates/idml/src/xmp/**; crates/idml/tests/xmp.rs
commit_subject: "feat(idml): port XMP metadata"
---

# IDML-007: XMP metadata

Port XMP parsing, packet access, replacement and removal, thumbnail updates, and assigned tests.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

