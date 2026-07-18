---
id: IDML-016
title: Exhaustive parity audit and release-quality documentation
depends_on: [IDML-015]
parallel_group: exclusive
owned_paths: crates/idml/README.md; crates/idml/benches/**; crates/idml/tests/**; tickets/upstream-v2.2.1.toml
commit_subject: "test(idml): complete upstream parity audit"
---

# IDML-016: Exhaustive parity audit and release-quality documentation

Close every ledger entry, convert examples to doc tests and benchmarks to Criterion, remove data-dependent skips, document retained ignores, and prove there is no IDMS, analysis, CLI, or Compositor dependency.

## Acceptance

- All upstream sources, declarations, and fixtures assigned by the parity ledger have Rust counterparts.
- Targeted tests pass without silent fixture-dependent early returns.
- Formatting, Clippy with warnings denied, workspace tests, doc tests, and benchmark compilation pass.
- The implementer spawned an adversarial reviewer and resolved every finding.

## Review and coordinator evidence

Pending.

