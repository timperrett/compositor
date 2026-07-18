# IDML Rust port ticket queue

This directory is the source of truth for the standalone `idml` crate port.
Ticket location is its state: `pending`, `in-progress`, or `done`.

Only the coordinator may move tickets or commit. Coding agents may edit only
their ticket's declared paths and must spawn a read-only adversarial reviewer
before handoff. The implementer must resolve every review finding; the
coordinator records the disposition and validation evidence before moving the
ticket to `done`.

At most two disjoint implementation tickets may run concurrently. Shared
workspace manifests, lockfiles, public re-exports, fixtures, and this ledger
are exclusive ownership.

Every completed ticket must pass its targeted checks plus:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p idml --doc
cargo bench -p idml --no-run
```

