---
id: IDML-005
title: Spread XML, pages, frames, groups, and images
depends_on: [IDML-004]
parallel_group: domain-c
owned_paths: crates/idml/src/spread/**; crates/idml/tests/spread.rs
commit_subject: "feat(idml): port spread document model"
---

# IDML-005: Spread XML, pages, frames, groups, and images

Complete spread parsing and marshaling, pages, guides, margins, frames, images, links, groups, text capacity, ordered page items, examples, and round-trip tests.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

