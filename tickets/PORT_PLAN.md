# Port idmllib IDML core to Rust

Baseline: upstream `github.com/dimelords/idmllib` tag `v2.2.1`, commit
`01546af176afbe72cce45378d6c927099bcc9383`.

Create an independent `crates/idml` Rust library with behavioral parity for
the upstream common, document, story, spread, resources, XMP, and IDML package
surfaces. Exclude analysis, IDMS export, Go binaries, and fixtures used only by
those exclusions. Preserve structural XML/ZIP fidelity and unknown XML; exact
byte output is not required for tests upstream already skips.

The first milestone is template-based layout bootstrapping. The remaining
tickets continue through the full retained IDML-core parity ledger in
`upstream-v2.2.1.toml`.

## Execution protocol

1. The coordinator moves a ready ticket to `in-progress` without committing.
2. A coding agent implements only the ticket-owned paths and tests.
3. The coding agent spawns a read-only adversarial reviewer and resolves every
   finding before handoff.
4. The coordinator runs ticket and workspace validation, records review and
   validation evidence, moves the ticket to `done`, and commits only that
   ticket's paths with its prescribed Conventional Commit subject.
5. If a correction requires code, the coordinator delegates it back instead
   of editing it.

## Architecture

The root becomes a Cargo workspace containing `compositor` and a publish-disabled
`idml` crate. The crate uses domain modules, ordered ZIP storage and lazy typed
caches. XML processing must retain qualified names, namespace declarations,
attribute/child order, processing instructions, mixed content, and unknown
subtrees. `mimetype` is always the first uncompressed ZIP entry. Public APIs
use Rust `Result`, `Option`, references, traits, and enums while retaining
upstream concepts and behavior.

