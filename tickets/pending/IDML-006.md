---
id: IDML-006
title: Fonts, styles, graphics, and resource parsing
depends_on: [IDML-002]
parallel_group: domain-b
owned_paths: crates/idml/src/resources/**; crates/idml/tests/resources.rs
commit_subject: "feat(idml): port IDML resources"
---

# IDML-006: Fonts, styles, graphics, and resource parsing

Port retained resource models and parsers for fonts, styles, graphics, colors, gradients, swatches, hierarchy, unknown children, examples, and tests.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

