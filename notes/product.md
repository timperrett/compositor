# Compositor Product Specification

**Status:** First draft
**Implementation language:** Rust
**Working name:** `compositor`
**Primary interface:** Command-line application
**Primary users:** Authors and editors producing illustrated story compendiums with Codex-assisted workflows

---

## 1. Product summary

Compositor is a deterministic production tool for converting Markdown story manuscripts into structured, versioned inputs for illustrated-book production.

Authors write each story as a single Markdown file with YAML front matter. Horizontal rules divide the manuscript into narrative content units. Compositor parses these files, preserves stable identities across edits, tracks changes incrementally, validates the compendium, and generates production artifacts such as page plans, illustration requirements, proofs, and build manifests.

Compositor is intended to be operated both directly by a human and indirectly by Codex through a reusable skill.

The tool must make repeated runs predictable. Editing one passage should not unnecessarily reorder the book, replace stable identifiers, invalidate approved artwork, or regenerate unrelated production assets.

---

## 2. Product goals

Compositor should:

1. Allow authors to write and revise an entire story in one readable Markdown file.
2. Use filesystem naming conventions to determine story order.
3. Use Markdown horizontal rules to express authored narrative boundaries.
4. Keep YAML front matter concise and limited to authored metadata.
5. Convert manuscripts into deterministic structured build artifacts.
6. Preserve stable identities for unchanged or moved content.
7. Detect and report inserted, removed, moved, split, merged, and edited content units.
8. Preserve approved page plans, illustration briefs, and artwork wherever possible.
9. Support conservative incremental regeneration by default.
10. Provide clear machine-readable output that Codex can use reliably.
11. Produce human-readable reports and proofs for editorial review.
12. Avoid modifying approved artifacts or source manuscripts without an explicit command.
13. Support an eventual full pipeline from manuscript to print-ready book layout.

---

## 3. Non-goals for the initial release

The first version of Compositor will not attempt to:

* write or rewrite story prose;
* make final aesthetic judgments about illustrations;
* generate images directly;
* replace a professional layout application in all cases;
* guarantee typographic quality suitable for commercial printing;
* resolve ambiguous content merges without human input;
* store project state in an external database;
* require authors to edit generated JSON, YAML, or internal manifests;
* automatically overwrite approved page plans or artwork;
* infer all narrative continuity without Codex or editorial review.

These may be added later through integrations or higher-level Codex workflows.

---

## 4. Core design principles

### 4.1 Markdown is the authoring source of truth

Human-authored story content must remain in Markdown.

Generated manifests, page plans, layout geometry, content hashes, and production metadata must be stored separately.

Compositor must never require an author to edit generated files.

### 4.2 Deterministic before generative

Compositor owns deterministic behavior:

* parsing;
* filesystem ordering;
* identity resolution;
* hashing;
* change detection;
* validation;
* state transitions;
* output locations;
* page-plan versioning;
* asset relationships;
* proof assembly.

Codex or another model may make creative recommendations, but it must consume and update Compositor-managed structures rather than recreate them independently.

### 4.3 Incremental by default

A normal build should preserve everything that remains valid.

Unchanged units should retain:

* stable identity;
* page assignment;
* illustration relationship;
* approved art;
* editorial notes;
* revision history.

A small prose edit should not cause the entire story or compendium to be reinterpreted.

### 4.4 Approved artifacts are immutable

Approved page plans, briefs, layouts, and artwork must never be modified in place.

Changes create new candidate revisions.

### 4.5 Filesystem structure should remain understandable

The repository itself should reveal:

* compendium order;
* story order;
* source manuscript location;
* generated state;
* approved assets;
* current output.

The project should not depend on opaque application state.

---

## 5. Primary use case

An author maintains a story in a single file:

```text
compendiums/
└── 01-seven-books-of-magic/
    ├── index.md
    ├── 01-the-hidden-shelf.md
    ├── 02-the-golden-book-of-kindness.md
    └── 03-the-silver-book-of-courage.md
```

Each story contains:

* YAML front matter;
* ordinary Markdown manuscript text;
* horizontal rules dividing narrative units;
* optional stable anchors;
* optional illustration and layout notes.

The author edits the manuscript and runs:

```bash
compositor build
```

Compositor then:

1. discovers compendiums and stories;
2. parses front matter and Markdown;
3. orders stories from filename prefixes;
4. divides each story into content units;
5. resolves stable identities;
6. compares the current source against the prior manifest;
7. reports changed and unchanged units;
8. preserves valid production relationships;
9. emits a new candidate build;
10. generates validation and proof outputs.

Codex may then use the build report to propose or update page plans and illustration briefs only where needed.

---

## 6. Project structure

A recommended repository structure is:

```text
edgar-stories/
├── AGENTS.md
├── Cargo.toml
├── compositor.toml
│
├── canon/
│   ├── characters/
│   ├── locations/
│   ├── objects/
│   └── illustration-style.md
│
├── compendiums/
│   └── 01-seven-books-of-magic/
│       ├── index.md
│       ├── 01-the-hidden-shelf.md
│       ├── 02-the-golden-book-of-kindness.md
│       └── 03-the-silver-book-of-courage.md
│
├── assets/
│   ├── references/
│   ├── drafts/
│   └── approved/
│
├── .compositor/
│   ├── manifest.json
│   ├── state/
│   ├── plans/
│   ├── briefs/
│   ├── layouts/
│   ├── history/
│   └── locks/
│
├── output/
│   ├── reports/
│   ├── proofs/
│   └── print/
│
└── tests/
    └── fixtures/
```

The exact structure should be configurable, but this convention should be the default.

---

## 7. Authoring format

### 7.1 Compendium metadata

Each compendium directory contains an `index.md`.

Example:

```markdown
---
id: seven-books-of-magic
title: Edgar and the Seven Books of Magic
type: compendium
status: draft
trim_size: 8x10
orientation: portrait
---

A collection of connected bedtime stories in which Edgar discovers
the seven books of magic.

Each story should stand alone while advancing a larger narrative.
```

The body may contain editorial intent and compendium-level notes.

The `index.md` file is not treated as story content unless explicitly configured.

### 7.2 Story metadata

Each story is one Markdown file.

Example:

```markdown
---
id: golden-book-of-kindness
title: The Golden Book of Kindness
type: story
status: draft
target_age: 5
target_reading_minutes: 8
---

Edgar woke before the castle bells.
```

Required fields for the initial version:

```yaml
id: golden-book-of-kindness
title: The Golden Book of Kindness
```

Optional fields may include:

```yaml
status: draft
target_age: 5
target_reading_minutes: 8
standalone: true
```

Derived values such as word count, story order, source path, and content hashes must not be written back into source front matter.

---

## 8. Content-unit boundaries

### 8.1 Horizontal rules

A top-level Markdown horizontal rule consisting of three hyphens is the canonical authored content boundary:

```markdown
Edgar woke before the castle bells.

---

He followed the golden sound into the passage.
```

The material between boundaries is called a **content unit**.

The term “content unit” is deliberate. A content unit represents authored narrative pacing but does not permanently require one physical printed page.

A page plan may later place:

* one unit on one page;
* multiple units on one page;
* one unit across a spread;
* one text unit opposite a full-page illustration;
* an illustration-only unit on one page.

### 8.2 Parsing constraints

The parser must distinguish:

* YAML front matter at the beginning of the file;
* top-level thematic breaks used as content boundaries;
* horizontal rules inside blockquotes or code fences;
* hyphens used as ordinary prose punctuation;
* Markdown lists.

Only valid top-level thematic breaks should divide units.

### 8.3 Empty units

Consecutive page boundaries that create an empty unit should produce a validation error unless the empty unit is explicitly marked as intentional.

---

## 9. Inline production directives

Compositor should support a deliberately small set of HTML-comment directives.

### 9.1 Stable anchor

```markdown
<!-- anchor: golden-book-reveal -->
```

Anchors provide stable identities for important content units.

Anchor names must:

* be unique within the project;
* use lowercase letters, digits, and hyphens;
* remain stable across file renaming or reordering;
* be used as the preferred unit identity.

### 9.2 Illustration note

```markdown
<!-- art:
Wide view of the castle library from Edgar's eye level.
Lady Aster's keys should be prominent.
Leave quiet space in the upper-left for text.
-->
```

This is authored art intent, not a complete generated art brief.

### 9.3 Layout preference

```markdown
<!-- layout: full-page -->
```

Supported initial values may include:

```text
auto
text-dominant
art-dominant
full-page
full-spread
facing-art
spot-art
illustration-only
```

These are preferences unless explicitly configured as constraints.

### 9.4 Keep relationship

```markdown
<!-- keep-with-next -->
```

This indicates that the current content unit should preferably remain adjacent to the next unit.

### 9.5 Explicit unit type

```markdown
<!-- unit: illustration-only -->
```

Potential types:

```text
narrative
transition
story-opening
story-closing
illustration-only
blank
```

The default is `narrative`.

### 9.6 Directive philosophy

The syntax must remain small and readable.

Compositor should not evolve into a general-purpose markup language. Rich generated production details belong under `.compositor/`.

---

## 10. Ordering conventions

### 10.1 Compendium order

Compendium directories are ordered lexicographically by filename.

Example:

```text
01-seven-books-of-magic/
02-door-to-other-worlds/
```

### 10.2 Story order

Story files within a compendium are ordered lexicographically by filename.

Example:

```text
01-the-hidden-shelf.md
02-the-golden-book-of-kindness.md
03-the-silver-book-of-courage.md
```

### 10.3 Identity versus position

The filename determines where a story appears.

The front-matter `id` determines what the story is.

Renaming:

```text
02-the-golden-book-of-kindness.md
```

to:

```text
03-the-golden-book-of-kindness.md
```

changes story order but must not change story identity.

### 10.4 Ignored files

By default, Compositor should ignore:

* files beginning with `_`;
* hidden files;
* temporary editor files;
* generated output directories;
* unsupported extensions.

---

## 11. Stable identity model

### 11.1 Story identity

Story identity is taken from the front-matter `id`.

The ID must remain stable if the filename or title changes.

### 11.2 Content-unit identity

Unit identity resolution should use this priority:

1. explicit `anchor`;
2. existing manifest relationship;
3. deterministic content fingerprint;
4. newly generated provisional ID.

### 11.3 Provisional identifiers

An unanchored unit may receive an ID such as:

```text
golden-book-of-kindness:u-6e83a1
```

The identifier must be deterministic where possible.

It should not be based only on ordinal position.

Potential matching signals include:

* normalized content hash;
* first and last sentence fingerprints;
* neighboring unit identities;
* source story ID;
* prior manifest location;
* similarity score.

### 11.4 Anchoring requirement

Before a content unit can have approved artwork, an approved page assignment, or an external reference, it should have an explicit stable anchor.

Compositor may warn or require promotion of a provisional ID to an anchor before approval.

---

## 12. Change detection

On each build, Compositor compares the new source model against the prior manifest.

It should classify changes as:

```text
unchanged
edited
inserted
deleted
moved
split
merged
reordered
ambiguous
```

### 12.1 Unchanged

Content and identity remain equivalent.

Production relationships remain untouched.

### 12.2 Edited

The unit identity is preserved, but its prose or directives changed.

Compositor should report:

* word-count change;
* changed directives;
* potential layout impact;
* whether visible action changed;
* whether a linked illustration brief may require review.

The initial implementation may use deterministic signals only and leave narrative interpretation to Codex.

### 12.3 Inserted

A new unit appears without a prior match.

It receives a new provisional or anchored identity.

### 12.4 Deleted

A prior unit no longer exists.

Linked approved assets become orphaned but must not be deleted.

### 12.5 Moved

A unit appears in a different ordinal position but retains identity.

Artwork and briefs remain attached.

### 12.6 Split

One prior unit appears to have become two or more units.

If an anchor exists, the section containing the anchor retains the original identity.

New sections receive new identities.

### 12.7 Merge

Multiple prior units appear to have become one.

If more than one source unit has approved production relationships, Compositor must not silently choose which identity survives.

The build should be blocked or marked as requiring resolution.

### 12.8 Ambiguous match

When confidence is below the configured threshold, Compositor must report ambiguity instead of inventing certainty.

---

## 13. Manifest

The manifest is the central generated record of source identity and production relationships.

Default location:

```text
.compositor/manifest.json
```

Example:

```json
{
  "schema_version": 1,
  "tool_version": "0.1.0",
  "compendiums": {
    "seven-books-of-magic": {
      "source": "compendiums/01-seven-books-of-magic/index.md",
      "stories": [
        "the-hidden-shelf",
        "golden-book-of-kindness"
      ]
    }
  },
  "stories": {
    "golden-book-of-kindness": {
      "source": "compendiums/01-seven-books-of-magic/02-the-golden-book-of-kindness.md",
      "source_hash": "sha256:...",
      "ordinal": 2,
      "units": [
        {
          "id": "golden-book-of-kindness:u-6e83a1",
          "anchor": null,
          "ordinal": 1,
          "content_hash": "sha256:...",
          "state": "active"
        },
        {
          "id": "golden-book-reveal",
          "anchor": "golden-book-reveal",
          "ordinal": 3,
          "content_hash": "sha256:...",
          "art_brief": "briefs/golden-book-reveal/v002.md",
          "approved_art": "assets/approved/golden-book-reveal-r03.png",
          "state": "active"
        }
      ]
    }
  }
}
```

The manifest should be generated atomically.

A failed build must not corrupt the prior valid manifest.

---

## 14. Build modes

Compositor should support explicit planning modes.

### 14.1 Conservative mode

Default mode.

```bash
compositor build --mode conservative
```

Behavior:

* preserve existing identities;
* preserve approved relationships;
* preserve page assignments wherever possible;
* limit invalidation to changed units;
* warn about imbalance or overflow;
* avoid broad repagination;
* produce a candidate build rather than modify approved plans.

### 14.2 Rebalance mode

```bash
compositor build --mode rebalance
```

Behavior:

* preserve anchors and approved artwork;
* allow page assignments to move;
* reconsider neighboring units;
* improve pacing and page density;
* retain prior plans for comparison.

### 14.3 Fresh mode

```bash
compositor build --mode fresh
```

Behavior:

* rebuild generated plans from source;
* preserve approved asset files;
* do not preserve their prior placement automatically;
* generate a new candidate plan;
* produce a full comparison against the active approved plan;
* never overwrite prior approved revisions.

---

## 15. State and approval model

### 15.1 Production artifact states

Generated artifacts may use states such as:

```text
draft
candidate
needs-review
approved
superseded
orphaned
locked
```

### 15.2 Immutability

An artifact in `approved` or `locked` state cannot be edited in place.

A requested change must create a new revision.

### 15.3 Active versus candidate

Each production category may have:

* one active approved revision;
* zero or more candidate revisions.

Example:

```json
{
  "active": "v002-approved.json",
  "candidate": "v003-candidate.json"
}
```

### 15.4 Approval mechanism

The first version may support approval through CLI commands:

```bash
compositor approve plan golden-book-of-kindness v003
compositor approve brief golden-book-reveal v002
compositor approve layout seven-books-of-magic v005
```

Approval should:

* validate the artifact;
* record timestamp and actor where available;
* mark the prior active revision as superseded;
* update the active pointer atomically.

---

## 16. Page plans

### 16.1 Purpose

A page plan maps content units to physical pages or spreads.

It is generated output and must not be authored directly in story Markdown.

### 16.2 Example structure

```json
{
  "story_id": "golden-book-of-kindness",
  "revision": 3,
  "status": "candidate",
  "assignments": [
    {
      "pages": [14],
      "units": ["golden-book-opening"],
      "layout": "text-dominant"
    },
    {
      "pages": [15, 16],
      "units": ["golden-book-reveal"],
      "layout": "full-spread",
      "art_id": "golden-book-reveal"
    }
  ]
}
```

### 16.3 Initial page planning

The first release may use deterministic rules based on:

* unit word count;
* configured page capacity;
* authored layout preferences;
* story-open rules;
* recto and verso requirements;
* approved prior assignments;
* illustration-only units;
* keep-with-next relationships.

Subjective page-planning recommendations may be supplied later by Codex.

### 16.4 Page plans are versioned

Each generated plan should be immutable:

```text
.compositor/plans/golden-book-of-kindness/
├── v001-candidate.json
├── v002-approved.json
└── v003-candidate.json
```

---

## 17. Illustration requirements and briefs

### 17.1 Authored art notes

Inline `art` directives express brief author intent.

### 17.2 Generated illustration requirement

Compositor may generate a requirement record when a page plan indicates that artwork is needed.

Example:

```json
{
  "art_id": "golden-book-reveal",
  "story_id": "golden-book-of-kindness",
  "unit_ids": ["golden-book-reveal"],
  "pages": [15, 16],
  "layout": "full-spread",
  "status": "needs-brief"
}
```

### 17.3 Generated art briefs

Rich art briefs should be stored outside the manuscript:

```text
.compositor/briefs/golden-book-reveal/v001.md
```

A brief may contain:

* narrative purpose;
* visible action;
* characters;
* location;
* composition;
* text-safe region;
* gutter constraints;
* continuity references;
* style references;
* revision notes;
* technical output requirements.

### 17.4 Artwork relationships

Artwork files should remain outside `.compositor/`:

```text
assets/drafts/golden-book-reveal/r01/
assets/approved/golden-book-reveal-r03.png
```

Compositor tracks the relationship but does not initially generate images.

---

## 18. Proof generation

The initial release should support simple proof generation.

### 18.1 HTML proof

Compositor should be able to generate an HTML proof containing:

* compendium title;
* story title;
* page numbers;
* manuscript text;
* unit identifiers;
* placeholders for missing art;
* approved artwork where available;
* warnings;
* revision metadata.

### 18.2 PDF proof

PDF generation may be included in the first release if a reliable Rust-compatible rendering path is selected.

Otherwise, HTML output should be designed for deterministic conversion through an external renderer.

### 18.3 Proof purpose

The first proof is for:

* page-flow review;
* missing-art review;
* overflow detection;
* sequencing;
* story opening placement;
* comparison between plan revisions.

It does not need to represent final commercial typography.

---

## 19. Validation

Compositor should provide:

```bash
compositor validate
```

Validation categories include:

### 19.1 Source validation

* malformed YAML front matter;
* missing required story IDs;
* duplicate story IDs;
* duplicate anchors;
* invalid anchor names;
* unreadable Markdown;
* empty units;
* unsupported directives;
* invalid filename order prefixes.

### 19.2 Structural validation

* broken compendium membership;
* duplicate ordering;
* missing compendium index;
* stories outside configured roots;
* references to missing units;
* references to missing assets.

### 19.3 Production validation

* approved art attached to a provisional unanchored unit;
* approved artifact modified in place;
* unresolved unit merge;
* orphaned approved assets;
* page-plan references to deleted units;
* missing page assignments;
* missing illustration briefs;
* missing approved artwork;
* output dimensions below configured requirements.

### 19.4 Report severity

Each issue should be classified:

```text
info
warning
error
blocking
```

Exit codes should reflect whether the build may proceed.

---

## 20. Command-line interface

Proposed initial commands:

```bash
compositor init
compositor status
compositor parse
compositor validate
compositor build
compositor diff
compositor plan
compositor proof
compositor approve
compositor inspect
```

### 20.1 `compositor init`

Creates:

* default directory structure;
* `compositor.toml`;
* `.compositor/`;
* optional sample compendium;
* optional `AGENTS.md` snippet.

### 20.2 `compositor status`

Shows:

* changed stories;
* changed units;
* unresolved matches;
* active plans;
* candidate plans;
* missing briefs;
* orphaned assets;
* validation state.

### 20.3 `compositor parse`

Parses source Markdown and emits a source model without changing production plans.

Useful flags:

```bash
compositor parse --story golden-book-of-kindness
compositor parse --format json
```

### 20.4 `compositor validate`

Runs project validation.

```bash
compositor validate
compositor validate --strict
compositor validate --story golden-book-of-kindness
```

### 20.5 `compositor build`

Runs parsing, identity resolution, change detection, validation, and candidate generation.

```bash
compositor build
compositor build --mode conservative
compositor build --story golden-book-of-kindness
```

### 20.6 `compositor diff`

Compares:

* source builds;
* manifests;
* page plans;
* briefs;
* layouts.

Examples:

```bash
compositor diff source
compositor diff plan golden-book-of-kindness v002 v003
```

### 20.7 `compositor plan`

Generates or updates a page-plan candidate.

```bash
compositor plan golden-book-of-kindness
compositor plan golden-book-of-kindness --mode rebalance
```

### 20.8 `compositor proof`

Generates an HTML or PDF proof.

```bash
compositor proof
compositor proof --story golden-book-of-kindness
compositor proof --format html
```

### 20.9 `compositor approve`

Approves a candidate artifact.

```bash
compositor approve plan golden-book-of-kindness v003
```

### 20.10 `compositor inspect`

Returns structured detail for Codex or human debugging.

```bash
compositor inspect story golden-book-of-kindness
compositor inspect unit golden-book-reveal
compositor inspect art golden-book-reveal
```

---

## 21. Machine-readable output

Every command should support structured output:

```bash
compositor status --format json
```

JSON output should have:

* a versioned schema;
* stable field names;
* no ANSI formatting;
* explicit paths;
* explicit IDs;
* explicit severity;
* clear exit codes.

Human-readable output should remain the default for interactive use.

Codex should normally consume JSON mode.

---

## 22. Configuration

Project configuration should live in:

```text
compositor.toml
```

Example:

```toml
schema_version = 1

[source]
compendiums_dir = "compendiums"
canon_dir = "canon"

[state]
directory = ".compositor"

[assets]
directory = "assets"
approved_directory = "assets/approved"
draft_directory = "assets/drafts"

[output]
directory = "output"

[ordering]
filename_prefix_digits = 2
ignore_prefix = "_"

[markdown]
boundary = "thematic_break"
require_story_id = true
require_anchor_before_approval = true

[build]
default_mode = "conservative"
similarity_threshold = 0.82

[book]
trim_width_in = 8.0
trim_height_in = 10.0
orientation = "portrait"
bleed_in = 0.125

[pagination]
target_words_per_text_page = 90
maximum_words_per_text_page = 130
story_starts_on_recto = true
```

Configuration should be validated and versioned.

---

## 23. Codex integration

Compositor should be designed for operation through a Codex skill.

The skill should instruct Codex to:

1. run `compositor status --format json`;
2. inspect changed units only;
3. use conservative mode unless explicitly instructed otherwise;
4. never reconstruct ordering or identity outside Compositor;
5. never modify approved artifacts in place;
6. create candidate briefs or plans;
7. validate after changes;
8. report exactly what changed.

Example Codex workflow:

```text
User asks to revise Story 3.

Codex edits the Markdown manuscript.

Codex runs:
compositor build --story story-3 --mode conservative --format json

Compositor reports:
- one edited unit;
- no identity changes;
- one possible text overflow;
- no artwork invalidation.

Codex proposes a local text or layout adjustment.

Codex runs validation and produces a new proof.
```

---

## 24. Rust implementation guidance

### 24.1 Workspace structure

A Rust workspace may contain:

```text
crates/
├── compositor-cli/
├── compositor-core/
├── compositor-markdown/
├── compositor-model/
├── compositor-state/
├── compositor-planning/
├── compositor-render/
└── compositor-test-support/
```

A smaller initial implementation may begin with fewer crates, but core parsing and state logic should remain separate from CLI presentation.

### 24.2 Candidate dependencies

Potential categories include:

* CLI parsing: `clap`
* serialization: `serde`, `serde_json`, `toml`
* YAML front matter: `serde_yaml` or a maintained YAML alternative
* Markdown parsing: `pulldown-cmark` or `comrak`
* hashing: `sha2`
* file traversal: `walkdir`
* error handling: `thiserror`, `anyhow`
* timestamps: `time`
* atomic writes and temporary files: `tempfile`
* similarity matching: custom normalized token comparison or a suitable crate
* HTML templating: `askama`, `minijinja`, or equivalent
* logging: `tracing`

Dependency selection should prioritize:

* active maintenance;
* deterministic behavior;
* minimal unnecessary complexity;
* cross-platform support;
* clear licensing.

### 24.3 Atomicity

State changes must use atomic file replacement where supported.

The sequence should be:

1. write new output to a temporary file;
2. flush and validate;
3. rename into place;
4. retain the previous valid state if any step fails.

### 24.4 Reproducibility

Where possible:

* sort all filesystem input explicitly;
* avoid map iteration ordering in generated files;
* normalize line endings;
* normalize Markdown content before hashing;
* use canonical JSON serialization or stable field ordering;
* record tool and schema versions;
* avoid timestamps in content-addressed outputs unless needed.

---

## 25. Testing strategy

### 25.1 Unit tests

Test:

* front-matter parsing;
* boundary detection;
* directive parsing;
* anchor validation;
* content normalization;
* identity matching;
* change classification;
* state transitions;
* path resolution;
* configuration parsing.

### 25.2 Fixture tests

Fixture stories should cover:

```text
simple-story.md
story-with-no-boundaries.md
story-with-code-fence-rule.md
story-with-blockquote-rule.md
inserted-unit.md
deleted-unit.md
moved-anchor.md
split-unit-before-anchor.md
split-unit-after-anchor.md
merged-units.md
duplicate-anchor.md
renamed-story-file.md
reordered-story.md
```

### 25.3 Snapshot tests

Snapshot tests should verify:

* source model output;
* manifests;
* validation reports;
* change reports;
* page-plan candidates;
* proof HTML.

The same input must produce byte-for-byte equivalent output where appropriate.

### 25.4 Regression tests

Every discovered identity or incremental-build bug should gain a permanent fixture.

### 25.5 End-to-end acceptance tests

Important acceptance scenarios:

#### Scenario A: Small prose edit

1. Build a story.
2. Approve its plan.
3. Edit two sentences in one unit.
4. Rebuild conservatively.

Expected:

* all unit IDs remain stable;
* unaffected page assignments remain unchanged;
* approved artwork remains linked;
* only the edited unit is marked changed.

#### Scenario B: Insert a new unit

1. Insert a boundary and new prose.
2. Rebuild.

Expected:

* surrounding units retain identity;
* the new unit receives a new ID;
* downstream ordinals update;
* approved assets remain attached to original units.

#### Scenario C: Move an anchored reveal

1. Move an anchored unit later in the story.
2. Rebuild.

Expected:

* anchor and unit identity remain stable;
* artwork remains attached;
* only position and page-plan impact are reported.

#### Scenario D: Merge two approved units

1. Merge two anchored units with approved assets.
2. Rebuild.

Expected:

* Compositor reports an unresolved merge;
* no approved asset is deleted;
* approval state is preserved;
* build requires explicit resolution.

---

## 26. Initial release scope

Version `0.1` should include:

1. project initialization;
2. configuration loading;
3. compendium and story discovery;
4. filename-based ordering;
5. YAML front-matter parsing;
6. Markdown content-unit parsing;
7. inline directive parsing;
8. stable anchor support;
9. manifest generation;
10. change detection;
11. conservative incremental builds;
12. source and structural validation;
13. JSON and human-readable reports;
14. simple deterministic page planning;
15. HTML proof generation;
16. approval and revision primitives;
17. fixture and snapshot tests.

Version `0.1` does not need:

* image generation;
* advanced AI integration;
* final typography;
* sophisticated visual analysis;
* complete print preflight;
* a graphical interface.

---

## 27. Future roadmap

### Version 0.2

* richer page-plan constraints;
* generated illustration requirement records;
* Markdown art-brief management;
* compendium-wide proof assembly;
* continuity metadata;
* page-plan visual diff;
* improved split and merge resolution.

### Version 0.3

* image-generation adapters;
* candidate artwork manifests;
* artwork revision tracking;
* automated dimension and aspect-ratio checks;
* text-safe and gutter metadata;
* contact-sheet generation.

### Version 0.4

* SVG page rendering;
* higher-quality PDF output;
* font embedding;
* trim and bleed handling;
* print preflight;
* printer-specific export profiles.

### Later possibilities

* local editorial dashboard;
* interactive plan approval;
* visual continuity matrix;
* semantic continuity analysis through Codex;
* InDesign or IDML export;
* collaborative review;
* remote asset storage;
* multiple editions from one manuscript.

---

## 28. Open design questions

The following decisions should be resolved before implementation begins:

1. Should every content unit require an anchor, or only units with production relationships?
2. Should a horizontal rule always create a unit, or should alternative thematic breaks be supported?
3. Should inline art directives remain in the manuscript after promotion to a full brief?
4. How should manual unit-match resolutions be recorded so they remain reproducible?
5. What similarity algorithm is sufficient for matching edited, moved, split, and merged units?
6. Should page plans be JSON, TOML, Markdown, or another generated format?
7. Should proof PDF generation be built into Rust or delegated to an external renderer?
8. How should Compositor identify the actor approving an artifact?
9. Should approval metadata be committed to Git?
10. How much layout geometry belongs in Compositor versus a later dedicated renderer?
11. Should canon files be parsed by Compositor in version `0.1`, or treated as opaque Codex-readable Markdown?
12. Should source comments support multiline YAML-like structures, or remain simple text directives?
13. Should word-count limits be global, compendium-level, story-level, or all three?
14. How should story transitions and front matter be represented in the initial source model?
15. Should generated identifiers be human-readable, hash-based, or a combination?

---

## 29. Success criteria

The first useful release is successful when an author can:

1. create a compendium containing several single-file Markdown stories;
2. use filename prefixes to control story order;
3. use horizontal rules to control narrative units;
4. build a deterministic manifest;
5. edit one part of a story;
6. rebuild without unrelated identities or production relationships changing;
7. see a clear report of exactly what changed;
8. preserve approved artwork and page assignments;
9. generate a readable proof;
10. allow Codex to operate the workflow consistently through stable CLI commands and machine-readable output.

The most important acceptance criterion is:

> Adding or editing prose in one part of a story must not cause unrelated units, approved artwork, or stable page assignments to change without an explicit rebalance or fresh-build request.
