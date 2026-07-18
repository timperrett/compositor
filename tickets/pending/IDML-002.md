---
id: IDML-002
title: Common errors, raw XML, and XML utilities
depends_on: [IDML-001]
parallel_group: domain-a
owned_paths: crates/idml/src/common/**; crates/idml/src/xml/**; crates/idml/tests/common.rs; crates/idml/tests/xml.rs
commit_subject: "feat(idml): port common XML primitives"
---

# IDML-002: Common errors, raw XML, and XML utilities

Port upstream pkg/common and internal/xmlutil, including ordered raw XML, namespaces, metadata instructions, structural comparison, error chains, and assigned tests.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

