---
id: IDML-004
title: Spread geometry and shape behavior
depends_on: [IDML-002]
parallel_group: domain-b
owned_paths: crates/idml/src/spread/geometry.rs; crates/idml/src/spread/oval.rs; crates/idml/src/spread/polygon.rs; crates/idml/src/spread/graphic_line.rs; crates/idml/tests/spread_geometry.rs
commit_subject: "feat(idml): port spread geometry"
---

# IDML-004: Spread geometry and shape behavior

Port transforms, bounds, path fallback, rectangles, ovals, polygons, graphic lines, constructors, calculations, mutation helpers, and assigned tests.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

