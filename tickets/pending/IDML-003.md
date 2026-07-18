---
id: IDML-003
title: Story model and ordered mixed content
depends_on: [IDML-002]
parallel_group: domain-b
owned_paths: crates/idml/src/story/**; crates/idml/tests/story.rs
commit_subject: "feat(idml): port IDML story support"
---

# IDML-003: Story model and ordered mixed content

Port stories, paragraph and character ranges, mixed content, line breaks, unknown children, parsing, marshaling, helpers, examples, and assigned tests.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

