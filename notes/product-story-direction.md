# Codex Implementation Brief: Edgar Composition Workflow

## Objective

Update the Edgar publishing workflow so that:

* Markdown remains the canonical source for story content.
* Codex skills make narrative and semantic layout decisions.
* Compositor owns metadata, structure, validation, deterministic planning, diagnostics, and production-package generation.
* Affinity Publisher remains the visual editor and owns all physical page geometry.
* Compositor does not generate or modify `.afpub` files.
* Compositor does not render a complete proof PDF.
* Compositor does not emit DOCX.
* Compositor does not emit frame-specific text files.
* Compositor does not flatten, composite, or render artwork.
* Compositor produces a predictable, spread-oriented filesystem package containing text, structured metadata, and organized art assets.
* Iterative builds may use draft artwork.
* Final builds may require approved artwork.
* The existing Edgar artwork-specification workflow remains in place: `art/briefs/*.yaml` continues to hold creative intent, generation prompts, candidates, review, and selection. Story-flow and composition decisions inform that workflow; they do not replace it.

The resulting workflow should allow Codex to interpret and direct the visual flow of an Edgar story while keeping Compositor deterministic and Affinity Publisher human-controlled.

---

# 1. Architectural Principles

Implement the system around the following separation of responsibilities.

## 1.1 Markdown owns story content

The story Markdown is the source of truth for:

* prose
* headings
* story metadata
* scene boundaries
* durable content identifiers
* explicit story structure
* illustration references where intentionally authored
* continuity metadata
* editorial metadata

The Markdown must remain readable and pleasant to edit.

Authors should not need to write the complete story structure directly in YAML.

## 1.2 Codex owns editorial interpretation

Codex skills should decide:

* where spreads begin and end
* the narrative purpose of each spread
* the emotional energy of each spread
* where page-turn reveals occur
* which story blocks should remain together
* which semantic layout family is appropriate
* which semantic layout variant is appropriate
* what each illustration should communicate
* which subject is the focal point
* where visual quiet may be needed
* whether the visual pacing has sufficient contrast
* whether the sequence of layouts has become repetitive

Codex must express these decisions through structured protocol files.

Codex must not emit:

* absolute page coordinates
* exact text-frame dimensions
* exact image-frame dimensions
* Affinity object identifiers
* `.afpub` files
* flattened spread artwork
* rendered page previews

## 1.3 Compositor owns deterministic structure

Compositor should:

* parse and validate story Markdown
* parse durable source identifiers
* calculate stable source hashes
* consume Codex-generated flow and composition plans
* validate semantic layout selections
* enforce design-system constraints
* validate source coverage
* detect invalid pacing or layout combinations
* resolve art assets according to build policy
* organize per-spread text and artwork
* generate structured diagnostics
* generate an Affinity-friendly production package
* preserve human overrides and locks

Compositor must not make open-ended creative decisions.

It may choose among explicitly declared alternatives only when deterministic rules permit it.

## 1.4 Affinity Publisher owns physical layout

Affinity Publisher owns:

* trim and bleed geometry
* margins and safe areas
* master pages
* exact text-frame placement
* exact image-frame placement
* typography implementation
* font sizes and leading
* image cropping
* visual hierarchy
* text wrapping
* decorative treatments
* foreground and background overlap
* final page composition
* final preflight
* final print PDF export

The connection between Compositor and Affinity is a stable semantic layout identifier.

Example:

```yaml
layout:
  family: cinematic
  variant: reveal-full-bleed
```

The Affinity template should contain a corresponding master-page convention.

Compositor must not inspect, generate, or modify the Affinity file.

## 1.5 Governing rule

Use this rule to resolve architectural ambiguity:

> Codex interprets the story. Compositor validates and packages the interpretation. Affinity Publisher implements the visual layout.

---

# 2. Repository and File Model

Use a structure similar to:

```text
project/
├── compendiums/
│   └── the-door-between-worlds/
│       ├── index.md
│       └── 01-the-map-that-wasnt-there/
│           ├── story.md
│           ├── story.flow.yaml
│           ├── hardcover-primary.composition.yaml
│           └── hardcover-primary.overrides.yaml
│
├── art/
│   ├── briefs/
│   ├── assets.yaml
│   ├── drafts/
│   ├── review/
│   ├── approved/
│   └── archive/
│
├── design-systems/
│   └── edgar-v1/
│       ├── design-system.yaml
│       ├── spread-roles.yaml
│       ├── layout-families.yaml
│       └── validation-rules.yaml
│
├── canon/
│   ├── edgar.md
│   ├── lady-aster.md
│   ├── wizard-king.md
│   └── art-guidelines.md
│
└── build/
```

This is the target canonical layout for new work. Numbered story directories retain
their compendium order, while each story owns its source and companion protocol
files. Existing flat story files must move through a deliberate, tested migration;
the project must not retain two permanent discovery models.

---

# 3. Durable Source Identifiers

Composition plans must never depend on line numbers, paragraph positions, or custom Markdown attribute syntax.

Use HTML comments embedded in the Markdown as the canonical identifier mechanism.

## 3.1 Identifier syntax

Version 1 supports durable identifiers for reader-visible prose paragraphs only.
Scenes and beats may become supported types in a later schema version, but are not
accepted source-reference types in this initial protocol.

Example:

```markdown
<!-- paragraph: opening-rain -->

Rain whispered against the high windows of the Royal Library.

<!-- paragraph: edgar-on-tiptoe -->

Edgar stood on tiptoe as Lady Aster reached toward the highest shelf.

<!-- paragraph: edgar-sees-box -->

High above him sat a wooden box carved with tiny dragons.
```

Comments are metadata and must not appear in reader-visible output.

Do not support custom Markdown attribute syntax such as:

```markdown
## The Hidden Box {#scene-hidden-box}
```

or:

```markdown
Edgar stood on tiptoe. {#paragraph-edgar-tiptoe}
```

## 3.2 Supported identifier types

Initially support:

```text
paragraph
```

Definition:

* `paragraph`: a single reader-visible prose paragraph

Do not allow Codex skills to invent identifier types unless the protocol schema and parser are updated to declare them.

## 3.3 Identifier rules

Identifiers must:

* be unique within the story
* remain stable across ordinary prose edits
* use lowercase kebab case
* contain only ASCII lowercase letters, digits, and hyphens
* begin with a letter
* describe durable narrative content rather than current position

Valid examples:

```text
hidden-box
opening-rain
edgar-sees-box
map-begins-to-glow
```

Invalid examples:

```text
paragraph-7
page-4-text
second-paragraph
final-version
```

Identifiers based primarily on position should produce a validation warning.

## 3.4 Comment association

An identifier comment applies to the next reader-visible prose paragraph.

Example:

```markdown
<!-- paragraph: opening-rain -->

Rain whispered against the high windows of the Royal Library.
```

The `paragraph` identifier applies to the paragraph immediately following the comment.

Compositor must preserve the association between identifier comments and their content after parsing.

## 3.5 Typed references

Protocol files should use typed source references.

Preferred representation:

```yaml
source:
  from:
    type: paragraph
    id: opening-rain

  through:
    type: paragraph
    id: edgar-sees-box
```

A shorthand string form may be supported for human convenience, but the canonical resolved representation should be typed.

## 3.6 Source validation

Compositor must report:

* duplicate identifiers
* malformed identifiers
* identifier comments with no following compatible content
* references to nonexistent identifiers
* duplicate source assignments
* unassigned reader-visible prose paragraphs
* source ranges that run backward
* overlapping source ranges
* ambiguous references
* positional identifiers likely to be unstable

Every reader-visible prose paragraph must be assigned exactly once. Headings,
lists, block quotes, and other Markdown block types are outside version-1 source
coverage unless a later schema explicitly adds them.

## 3.7 Explicit source preparation

Story Flow Planner is intentionally a pure interpretation step: it must never
silently change story Markdown. Before planning an unprepared story, use a
dedicated, explicitly invoked source-preparation workflow to add durable
`paragraph` comments.

That workflow may edit Markdown only when the user requests source preparation.
It may add or repair paragraph identifiers, but must not rewrite prose, change
story structure, rename an existing durable identifier without approval, or
invent identifier types. It must report the identifiers it added and leave the
story ready for review before flow planning begins.

The existing art-specification skills may consume prepared IDs and add artwork
anchors, but exhaustive paragraph identification remains a separate concern from
art specification.

---

# 4. Protocol Model

Implement two distinct generated protocol layers:

1. Story Flow Plan
2. Composition Plan

Human overrides are maintained separately.

---

# 5. Story Flow Plan

Suggested filename:

```text
story.flow.yaml
```

Schema identifier:

```yaml
schema: compositor.dev/story-flow/v1
```

The Story Flow Plan captures narrative and editorial structure independently of a physical edition.

## 5.1 Example

```yaml
schema: compositor.dev/story-flow/v1

story:
  id: the-map-that-wasnt-there
  source: story.md
  source_revision: sha256:81df...

spreads:
  - id: spread-001

    source:
      from:
        type: paragraph
        id: opening-rain

      through:
        type: paragraph
        id: dragon-box-revealed

    role: opening-wonder
    energy: 2

    narrative:
      purpose: Establish the Royal Library as mysterious and inviting.
      reader_question: What is inside the dragon-carved box?
      page_turn_out: discovery

    constraints:
      max_words: 100
      must_keep_together:
        - type: paragraph
          id: opening-rain

        - type: paragraph
          id: dragon-box-revealed

  - id: spread-002

    source:
      from:
        type: paragraph
        id: box-opens

      through:
        type: paragraph
        id: map-appears

    role: discovery
    energy: 3

    narrative:
      purpose: Reveal that the box contains an impossible map.
      page_turn_in: discovery
      page_turn_out: anticipation
```

## 5.2 Story Flow Plan responsibilities

The plan should capture:

* source range
* narrative role
* energy
* narrative purpose
* reader question, where useful
* page-turn intent
* must-keep-together constraints
* optional editorial notes
* optional word-count guidance

It must not contain:

* physical coordinates
* Affinity master names
* art filenames
* exact image geometry
* typography implementation
* physical page dimensions

---

# 6. Composition Plan

Suggested filename:

```text
hardcover-primary.composition.yaml
```

Schema identifier:

```yaml
schema: compositor.dev/composition-plan/v1
```

The Composition Plan maps the Story Flow Plan to semantic visual patterns for a particular edition.

## 6.1 Example

```yaml
schema: compositor.dev/composition-plan/v1

story:
  id: the-map-that-wasnt-there
  flow: story.flow.yaml

edition:
  id: hardcover-primary
  design_system: edgar-v1

spreads:
  - id: spread-001

    layout:
      family: environment-led
      variant: opening-quiet-upper-left

    text:
      density: light

    illustration:
      mode: full-scene
      focal_subject: lady-aster-and-dragon-box
      viewpoint: edgar-eye-level
      quiet_region: upper-left

    art_assets:
      - id: royal-library-rain
        role: background

      - id: lady-aster-dragon-box
        role: primary-subject

      - id: dragon-box-detail
        role: supporting-detail

  - id: spread-002

    layout:
      family: object-led
      variant: discovery-object-right

    text:
      density: medium

    illustration:
      mode: object-led
      focal_subject: impossible-map
      quiet_region: left

    art_assets:
      - id: impossible-map
        role: primary-subject
```

## 6.2 Composition Plan responsibilities

The plan should capture:

* layout family
* layout variant
* text density
* illustration mode
* focal subject
* viewpoint
* scale intent
* quiet-region intent
* referenced art asset IDs
* semantic art roles
* edition-specific visual decisions

It must not contain:

* x/y coordinates
* frame width or height
* crop coordinates
* exact font sizes
* exact page geometry
* Affinity object identifiers
* flattened artwork
* frame-specific text partitions

---

# 7. Design-System Catalog

Create a machine-readable starter design-system catalog. It establishes a small,
declared vocabulary and validation baseline; it is not a claim that a matching
Affinity template already exists.

Suggested structure:

```text
design-systems/
└── edgar-v1/
    ├── design-system.yaml
    ├── spread-roles.yaml
    ├── layout-families.yaml
    └── validation-rules.yaml
```

## 7.1 Design-system descriptor

Example:

```yaml
schema: compositor.dev/design-system/v1

id: edgar-v1
name: Edgar Storybook Design System
version: 1
```

## 7.2 Spread roles

Initial vocabulary:

```yaml
spread_roles:
  - opening-wonder
  - discovery
  - conversation
  - anticipation
  - journey
  - challenge
  - reveal
  - reflection
  - closing
```

Each role should define:

* purpose
* typical energy range
* allowed text densities
* compatible page-turn behavior
* compatible layout families
* pacing guidance

Example:

```yaml
roles:
  reveal:
    purpose: Deliver a major visual or narrative payoff.

    energy:
      min: 4
      max: 5

    text_density:
      allowed:
        - minimal
        - light

    compatible_layout_families:
      - cinematic
      - environment-led

    page_turn:
      preferred_in:
        - reveal
```

## 7.3 Layout families

Example:

```yaml
layout_families:
  cinematic:
    compatible_roles:
      - opening-wonder
      - journey
      - reveal

    variants:
      reveal-full-bleed:
        text_density:
          allowed:
            - minimal
            - light

        illustration:
          coverage: dominant

        affinity_master_hint:
          convention: EDGAR_REVEAL_FULL_BLEED

      cinematic-quiet-corner:
        text_density:
          allowed:
            - light
            - medium

        illustration:
          coverage: dominant

        affinity_master_hint:
          convention: EDGAR_CINEMATIC_QUIET_CORNER

  conversation:
    compatible_roles:
      - conversation
      - reflection

    variants:
      conversation-balanced:
        text_density:
          allowed:
            - medium
            - heavy

        illustration:
          coverage: supporting

        affinity_master_hint:
          convention: EDGAR_CONVERSATION_BALANCED
```

## 7.4 Affinity master hints

`affinity_master_hint` is a human-readable starter convention only.

It may appear in generated manifests and assembly guides.

Compositor must not:

* inspect whether the master exists
* modify an Affinity template
* derive geometry from the name
* require access to an `.afpub` file

## 7.5 No physical geometry

Do not add the following to the design-system protocol:

* x coordinates
* y coordinates
* frame widths
* frame heights
* crop bounds
* bleed coordinates
* exact type sizes
* application-specific object IDs

Those remain implementation details inside the Affinity template.

## 7.6 Validation-only art surface profiles

Affinity remains authoritative for physical page geometry. Compositor may,
however, validate artwork against named, human-maintained art surface profiles
derived from the Affinity template. A profile may describe an acceptable aspect
ratio range and minimum source resolution for a named semantic surface.

These profiles are validation inputs, not layout instructions. Compositor must
not calculate frame positions from them, crop artwork, alter an Affinity file,
or emit their physical values in a Flow Plan, Composition Plan, resolved spread
manifest, or production package. This preserves the useful asset-quality checks
without giving Compositor ownership of the visual editor's geometry.

---

# 8. Codex Skill: Story Flow Planner

Suggested skill name:

```text
edgar-story-flow-planner
```

## 8.1 Purpose

Analyze a completed or near-completed Edgar story and produce a Story Flow Plan.

## 8.2 Inputs

The skill should consume:

* story Markdown
* story front matter
* durable comment-based source identifiers
* relevant Edgar canon references
* target reading age
* target reading time
* target number of spreads or page budget, when provided
* available spread-role vocabulary
* design-system pacing rules

## 8.3 Responsibilities

The skill should:

1. identify natural narrative units
2. divide the manuscript into spreads
3. assign source ranges
4. assign one declared narrative role to each spread
5. assign an energy score
6. describe the narrative purpose of each spread
7. identify intended page-turn behavior
8. identify blocks that should remain together
9. identify unusually dense or sparse sections
10. preserve all reader-visible story text
11. avoid rewriting prose unless explicitly requested
12. identify where setup is required before a reveal
13. check that the energy curve has appropriate contrast

## 8.4 Output

The skill must emit valid:

```yaml
schema: compositor.dev/story-flow/v1
```

It should not emit explanatory prose unless explicitly requested.

## 8.5 Restrictions

The skill must not:

* rewrite the story silently
* modify story Markdown or source identifiers
* use line numbers as source references
* emit physical layout geometry
* select undeclared spread roles
* refer to Affinity masters
* assign art filenames

## 8.6 Editorial notes

The skill may recommend prose changes, but must express them as structured notes.

Example:

```yaml
notes:
  - code: DENSE_DISCOVERY
    severity: warning
    spread: spread-004
    message: This discovery spread contains 182 words and may be difficult to support with the intended illustration.
```

## 8.7 Self-validation

Before completing, the skill should check:

* every source block is assigned
* no source block is assigned twice
* spread IDs are unique and sequential
* roles come from the supplied catalog
* energy values are valid
* reveal spreads have setup
* page-turn intents are coherent
* the energy curve contains contrast
* maximum word-count guidance is respected or explicitly flagged

---

# 9. Codex Skill: Layout Director

Suggested skill name:

```text
edgar-layout-director
```

## 9.1 Purpose

Consume a Story Flow Plan and select semantic layout families, variants, illustration intent, and art assets for each spread.

## 9.2 Inputs

The skill should consume:

* story Markdown
* Story Flow Plan
* design-system catalog
* art asset registry
* illustration rules
* character canon
* location canon
* prior Compositor diagnostics
* edition metadata
* human overrides and locks, where present

## 9.3 Responsibilities

The skill should:

1. select a compatible layout family
2. select a declared layout variant
3. assign text density
4. define illustration mode
5. identify the focal subject
6. identify quiet-region needs
7. identify viewpoint and scale where relevant
8. select art asset IDs
9. assign semantic art roles
10. request missing art where necessary
11. vary adjacent layout patterns
12. avoid repetitive visual rhythm
13. preserve locked human decisions
14. revise a plan in response to structured Compositor diagnostics

## 9.4 Output

The skill must emit valid:

```yaml
schema: compositor.dev/composition-plan/v1
```

## 9.5 Restrictions

The skill must not:

* invent layout-family names
* invent layout-variant names
* emit physical coordinates
* emit exact frame dimensions
* emit Affinity object instructions
* generate flattened artwork
* split prose into frame-specific files
* rewrite the story
* rename durable source IDs
* select rejected or superseded artwork
* override locked human values

## 9.6 Diagnostic revision behavior

When Compositor provides permitted resolutions, the skill should choose from those alternatives.

Example diagnostic:

```yaml
code: INCOMPATIBLE_TEXT_DENSITY

permitted_resolutions:
  - choose_variant:
      - conversation-text-heavy
      - conversation-balanced
```

The skill must not invent an undeclared third variant.

## 9.7 Relationship to existing Edgar art skills

`specify-story-art` and `specify-opener-art` remain the workflows for
manuscript-level art intent and `art/briefs/*.yaml` records. They should evolve
to consume the Story Flow Plan and Composition Plan so that artwork follows the
chosen spread, narrative purpose, focal subject, quiet region, and semantic
layout intent.

They do not become physical-layout tools and do not replace the separate
source-preparation step. `render-storybook-art` remains responsible for draft
candidate generation and continues to operate on validated art records.

---

# 10. Artwork Asset Lifecycle Integration

Artwork must be described through stable identities and lifecycle statuses rather
than inferred exclusively from filenames. The existing `art/briefs/*.yaml`
workflow remains the creative-record system of record for prompts, candidates,
review, and selection.

The exact integration contract between those briefs and a source-side asset
registry is deliberately deferred for a separate design discussion. The
`art/assets.yaml` filename and examples below are a proposed registry shape, not
an instruction to replace briefs or to implement a second competing art
workflow. Until that decision is made, commands in this document use “asset
registry” to mean the approved integration contract.

Proposed filename:

```text
art/assets.yaml
```

Schema identifier:

```yaml
schema: compositor.dev/art-assets/v1
```

The `art-assets` schema becomes required only after the separate lifecycle
integration decision in section 10. Existing art-record schemas remain in force
until then.

## 10.1 Asset lifecycle

Support the following statuses:

```text
requested
draft
review
approved
rejected
superseded
```

Definitions:

* `requested`: required artwork has been identified, but no placeable file exists
* `draft`: usable work-in-progress artwork exists
* `review`: artwork is ready for structured review
* `approved`: artwork has been accepted for final production
* `rejected`: artwork must not be used
* `superseded`: artwork has been replaced by a newer asset

Only the following are eligible for ordinary placement:

```text
draft
review
approved
```

Assets marked `rejected` or `superseded` must never be selected automatically.

A `requested` asset has no placeable file.

## 10.2 Registry example

```yaml
schema: compositor.dev/art-assets/v1

assets:
  - id: royal-library-rain
    status: approved
    role: background
    file: approved/royal-library-rain-v03.tif

  - id: lady-aster-dragon-box
    status: draft
    role: primary-subject
    file: drafts/lady-aster-dragon-box-v02.png

  - id: dragon-box-detail
    status: requested
    role: supporting-detail

  - id: old-map
    status: superseded
    role: primary-subject
    file: archive/old-map-v01.png
    superseded_by: impossible-map
```

## 10.3 Stable asset identity

Composition plans must refer to stable asset IDs when they select placeable art:

```yaml
art_assets:
  - id: lady-aster-dragon-box
    role: primary-subject
```

They should not normally refer directly to versioned filenames.

The stable asset ID allows the underlying file to change while preserving the production-package path.

## 10.4 Asset ID rules

Asset IDs must:

* be unique within the registry
* use lowercase kebab case
* remain stable across revisions
* describe the depicted asset
* avoid version numbers
* avoid status names
* avoid words such as `final`, `latest`, or `new`

Valid:

```text
lady-aster-dragon-box
royal-library-rain
impossible-map
golden-door-glow
```

Invalid:

```text
lady-aster-v2
final-library
latest-map
approved-dragon
```

---

# 11. Build Asset Policies

Add an explicit asset policy to `compositor build`.

Supported policies:

```text
draft
review
approved
```

## 11.1 Draft policy

```text
--asset-policy draft
```

Allow:

* draft
* review
* approved

Use for ordinary iteration.

## 11.2 Review policy

```text
--asset-policy review
```

Allow:

* review
* approved

Reject draft-only artwork.

Use for formal visual review.

## 11.3 Approved policy

```text
--asset-policy approved
```

Allow:

* approved only

Use for final production and release candidates.

## 11.4 Default policy

If no asset policy is specified, default to:

```text
draft
```

The ordinary editing workflow should not fail merely because artwork has not yet been approved.

## 11.5 Strict art mode

Add:

```text
--strict-art
```

Without `--strict-art`:

* requested assets produce warnings
* missing files produce warnings
* assets below policy produce warnings
* unresolved spread packages are still emitted
* unresolved assets are recorded in metadata
* the assembly guide marks missing or ineligible art clearly

With `--strict-art`:

* requested assets are errors
* missing files are errors
* assets below policy are errors
* unknown assets are errors
* the build fails

## 11.6 Typical commands

Iterative build:

```bash
compositor build \
  story.md \
  story.flow.yaml \
  hardcover-primary.composition.yaml \
  --assets art/assets.yaml \
  --asset-policy draft \
  --output build/hardcover-primary/
```

Final build:

```bash
compositor build \
  story.md \
  story.flow.yaml \
  hardcover-primary.composition.yaml \
  --assets art/assets.yaml \
  --asset-policy approved \
  --strict-art \
  --output build/hardcover-primary/
```

---

# 12. Asset Resolution

When resolving an asset ID, Compositor must:

1. locate the asset in the registry
2. verify the asset is not rejected
3. verify the asset is not superseded
4. check its status against the selected build policy
5. verify the source file exists when a file is expected
6. copy or link the file into the spread's `art/` directory
7. preserve the original file format
8. preserve alpha channels
9. preserve source resolution
10. avoid destructive conversion
11. record source status and source path in metadata

## 12.1 Stable package filenames

The production-package filename should be based on the stable asset ID.

Example source file:

```text
art/drafts/lady-aster-dragon-box-v02.png
```

Generated package file:

```text
art/lady-aster-dragon-box.png
```

A later source revision:

```text
art/drafts/lady-aster-dragon-box-v03.png
```

should still generate:

```text
art/lady-aster-dragon-box.png
```

This supports stable linked-file paths in Affinity Publisher.

## 12.2 Copy versus link behavior

Support an implementation strategy appropriate to the platform.

The default should be deterministic copying.

A future option may support symbolic links or hard links, but this is not required for the initial implementation.

## 12.3 Filename collision handling

If two assets would generate the same package filename:

* fail validation
* identify both source asset IDs
* do not silently rename either asset

---

# 13. Compositor Commands

Implement the following commands or equivalent capabilities.

---

## 13.1 Inspect

```bash
compositor inspect story.md
```

Responsibilities:

* parse front matter
* parse Markdown structure
* parse identifier comments
* discover identified and unaddressable prose paragraphs
* report word counts
* calculate source revision hash
* detect duplicate IDs
* detect malformed IDs
* report unaddressable reader-visible blocks

Optional machine-readable output:

```bash
compositor inspect story.md --format json
```

---

## 13.2 Validate flow

```bash
compositor validate-flow \
  story.md \
  story.flow.yaml \
  --design-system design-systems/edgar-v1
```

Validate:

* schema version
* story ID
* source revision
* source references
* source ordering
* source coverage
* duplicate assignments
* missing assignments
* role vocabulary
* energy range
* word-count constraints
* must-keep-together rules
* page-turn semantics
* pacing rules

---

## 13.3 Validate composition

```bash
compositor validate-composition \
  story.md \
  story.flow.yaml \
  hardcover-primary.composition.yaml \
  --design-system design-systems/edgar-v1 \
  --assets art/assets.yaml
```

Validate:

* Story Flow Plan alignment
* spread IDs
* edition ID
* design-system ID
* layout-family existence
* layout-variant existence
* role and layout compatibility
* text-density compatibility
* required illustration metadata
* referenced art asset IDs
* art roles
* rejected or superseded asset references
* disallowed physical geometry fields

---

## 13.4 Diagnose

```bash
compositor diagnose \
  story.md \
  story.flow.yaml \
  hardcover-primary.composition.yaml \
  --design-system design-systems/edgar-v1 \
  --assets art/assets.yaml \
  --format yaml
```

Emit stable, machine-readable diagnostics.

Example:

```yaml
schema: compositor.dev/diagnostics/v1

result: warning

diagnostics:
  - code: ENERGY_CLUSTER
    severity: warning
    spreads:
      - spread-005
      - spread-006
      - spread-007
    values:
      - 4
      - 5
      - 5
    message: Three consecutive high-energy spreads may reduce the impact of the final reveal.

  - code: REPEATED_LAYOUT_FAMILY
    severity: warning
    spreads:
      - spread-003
      - spread-004
      - spread-005
    layout_family: conversation
    permitted_resolutions:
      - keep
      - revise_spread_004
      - revise_spread_005
```

Diagnostics should be:

* stable
* code-based
* machine-readable
* actionable
* deterministic
* constrained where possible

---

## 13.5 Reconcile

```bash
compositor reconcile \
  story.md \
  story.flow.yaml \
  hardcover-primary.composition.yaml \
  hardcover-primary.overrides.yaml \
  --design-system design-systems/edgar-v1 \
  --output hardcover-primary.resolved.yaml
```

Resolution order:

1. design-system defaults
2. Story Flow Plan
3. Composition Plan
4. human overrides
5. human locks
6. validation

The resolved output should include provenance.

Example:

```yaml
provenance:
  layout.family:
    source: composition-plan

  layout.variant:
    source: human-override

  illustration.quiet_region:
    source: human-override
```

---

## 13.6 Build

```bash
compositor build \
  story.md \
  story.flow.yaml \
  hardcover-primary.composition.yaml \
  --design-system design-systems/edgar-v1 \
  --assets art/assets.yaml \
  --asset-policy draft \
  --output build/hardcover-primary/
```

The build command must not render the complete book.

It should organize the source material into an Affinity-friendly production package.

---

# 14. Production Package Output

Use a spread-oriented filesystem convention.

Recommended output:

```text
build/
└── hardcover-primary/
    ├── manifest.yaml
    ├── diagnostics.yaml
    ├── assembly-guide.html
    │
    └── spreads/
        ├── 001-opening-wonder/
        │   ├── spread.yaml
        │   ├── text.md
        │   └── art/
        │       ├── royal-library-rain.tif
        │       ├── lady-aster-dragon-box.png
        │       └── dragon-box-detail.png
        │
        ├── 002-discovery/
        │   ├── spread.yaml
        │   ├── text.md
        │   └── art/
        │       └── impossible-map.tif
        │
        └── 003-reveal/
            ├── spread.yaml
            ├── text.md
            └── art/
                ├── golden-door-background.tif
                └── edgar-foreground.png
```

## 14.1 Explicit exclusions

Do not emit:

* `.afpub` files
* complete proof PDFs
* per-spread layout-reference PDFs
* rendered page previews
* DOCX files
* RTF files
* frame-specific text files
* flattened spread images
* flattened preview images
* generated composites
* physical layout coordinates
* exact Affinity placement instructions

---

# 15. Per-Spread Text Output

Each spread must contain one:

```text
text.md
```

This file contains all reader-visible text assigned to the spread, in reading order.

Preserve:

* paragraph breaks
* emphasis
* dialogue punctuation
* supported inline Markdown
* source identifier comments where useful

Do not split text according to Affinity frames.

Example:

```markdown
<!-- source: paragraph:opening-rain -->

Rain whispered against the high windows of the Royal Library.

<!-- source: paragraph:edgar-sees-box -->

Edgar stood on tiptoe as Lady Aster reached toward the highest shelf.
```

Source comments in generated `text.md` are metadata.

They must not be interpreted as reader-visible text.

---

# 16. Per-Spread Manifest

Each spread must contain:

```text
spread.yaml
```

Example:

```yaml
schema: compositor.dev/resolved-spread/v1

id: spread-001
number: 1

source:
  from:
    type: paragraph
    id: opening-rain

  through:
    type: paragraph
    id: dragon-box-revealed

role: opening-wonder
energy: 2

narrative:
  purpose: Establish the Royal Library as mysterious and inviting.
  reader_question: What is inside the dragon-carved box?
  page_turn_out: discovery

layout:
  family: environment-led
  variant: opening-quiet-upper-left
  affinity_master_hint: EDGAR_OPENING_QUIET_UPPER_LEFT

text:
  file: text.md
  word_count: 87
  density: light

illustration:
  mode: full-scene
  focal_subject: lady-aster-and-dragon-box
  viewpoint: edgar-eye-level
  quiet_region: upper-left

art:
  - id: royal-library-rain
    file: art/royal-library-rain.tif
    role: background
    status: approved
    source: art/approved/royal-library-rain-v03.tif
    resolved: true

  - id: lady-aster-dragon-box
    file: art/lady-aster-dragon-box.png
    role: primary-subject
    status: draft
    source: art/drafts/lady-aster-dragon-box-v02.png
    resolved: true

  - id: dragon-box-detail
    role: supporting-detail
    status: requested
    resolved: false

status:
  validation: warning
```

Do not add physical layout geometry to this file.

---

# 17. Root Manifest

The root package must contain:

```text
manifest.yaml
```

Example:

```yaml
schema: compositor.dev/production-package/v1

story:
  id: the-map-that-wasnt-there
  title: The Map That Wasn't There
  source: story.md
  source_revision: sha256:81df...

edition:
  id: hardcover-primary
  design_system: edgar-v1

build:
  asset_policy: draft
  strict_art: false

spreads:
  count: 14

  entries:
    - id: spread-001
      directory: spreads/001-opening-wonder
      role: opening-wonder

      layout:
        family: environment-led
        variant: opening-quiet-upper-left

    - id: spread-002
      directory: spreads/002-discovery
      role: discovery

      layout:
        family: object-led
        variant: discovery-object-right

art_readiness:
  approved: 18
  review: 3
  draft: 5
  requested: 2
  unresolved: 2

validation:
  result: warning
  diagnostics: diagnostics.yaml
```

---

# 18. Assembly Guide

Generate:

```text
assembly-guide.html
```

This is not a rendered proof and must not attempt to reproduce the final spread.

Its purpose is to make manual Affinity assembly predictable.

## 18.1 Required content

For each spread, include:

* spread number
* spread ID
* narrative role
* energy
* narrative purpose
* page-turn intent
* semantic layout family
* semantic layout variant
* Affinity master hint
* relative path to `text.md`
* relative paths to art assets
* asset roles
* asset statuses
* focal subject
* quiet-region intent
* viewpoint
* unresolved warnings
* human notes

## 18.2 Example presentation

```text
Spread 1 — Opening Wonder

Affinity master convention:
EDGAR_OPENING_QUIET_UPPER_LEFT

Text:
spreads/001-opening-wonder/text.md

Art:
- royal-library-rain.tif — background — APPROVED
- lady-aster-dragon-box.png — primary subject — DRAFT
- dragon-box-detail — supporting detail — REQUESTED

Narrative purpose:
Establish the Royal Library as mysterious and inviting.

Composition intent:
- Edgar-height viewpoint
- quiet region in the upper left
- Lady Aster and the box are the focal subject
- preserve the sense of scale in the library
```

## 18.3 Restrictions

The assembly guide must not:

* generate a flattened spread
* display a rendered spread preview
* provide physical page coordinates
* attempt to duplicate Affinity geometry
* embed final composited artwork

Thumbnails of individual source assets may be added later, but are not required for the initial implementation.

---

# 19. Human Overrides and Locks

Support human-authored overrides without requiring edits to generated plans.

An override is a human choice that takes precedence over Codex's generated
semantic choice. A lock is an override field that later Codex runs must retain.
For version 1, overrides are limited to visual semantics and art selection:
layout family or variant, text density, illustration intent, and selected art
assets. They must not change a spread's source range or move prose between
spreads; those are Story Flow Plan decisions and remain subject to complete,
single source coverage.

Suggested filename:

```text
hardcover-primary.overrides.yaml
```

Schema identifier:

```yaml
schema: compositor.dev/composition-overrides/v1
```

## 19.1 Example

```yaml
schema: compositor.dev/composition-overrides/v1

spreads:
  spread-007:
    layout:
      variant: reveal-full-bleed

    illustration:
      quiet_region: upper-left

    locks:
      - layout.variant
      - illustration.quiet_region

    note: Preserve a clear area around Lady Aster's keys.
```

## 19.2 Requirements

* human overrides take precedence over generated values
* locked values must not be overwritten
* Codex skills must receive lock information
* incompatible locks must produce diagnostics
* overrides may change only the supported visual-semantic and art-selection fields
* arbitrary Affinity nudges should not be represented
* generated files must not overwrite the human-authored override file

---

# 20. Diagnostics

Support stable diagnostic codes.

## 20.1 Source diagnostics

```text
SOURCE_ID_DUPLICATE
SOURCE_ID_MALFORMED
SOURCE_ID_ORPHANED
SOURCE_REFERENCE_UNKNOWN
SOURCE_RANGE_INVALID
SOURCE_BLOCK_UNASSIGNED
SOURCE_BLOCK_ASSIGNED_MULTIPLE
SOURCE_ID_POSITIONAL
```

## 20.2 Flow diagnostics

```text
FLOW_ROLE_UNKNOWN
FLOW_ENERGY_INVALID
FLOW_PAGE_TURN_INVALID
FLOW_WORD_COUNT_HIGH
FLOW_WORD_COUNT_LOW
FLOW_REVEAL_WITHOUT_SETUP
ENERGY_CLUSTER
ENERGY_FLAT
```

## 20.3 Composition diagnostics

```text
LAYOUT_FAMILY_UNKNOWN
LAYOUT_VARIANT_UNKNOWN
LAYOUT_ROLE_INCOMPATIBLE
TEXT_DENSITY_INCOMPATIBLE
REPEATED_LAYOUT_FAMILY
REPEATED_LAYOUT_VARIANT
ILLUSTRATION_METADATA_MISSING
PHYSICAL_GEOMETRY_DISALLOWED
```

## 20.4 Art diagnostics

```text
ART_ASSET_REQUESTED
ART_FILE_MISSING
ART_STATUS_BELOW_POLICY
ART_ASSET_REJECTED
ART_ASSET_SUPERSEDED
ART_ASSET_UNKNOWN
ART_ROLE_MISMATCH
ART_FILENAME_COLLISION
DRAFT_ART_IN_BUILD
```

`DRAFT_ART_IN_BUILD` should be informational when the build policy is `draft`.

Example approved-only failure:

```yaml
- code: ART_STATUS_BELOW_POLICY
  severity: error
  spread: spread-001
  asset: lady-aster-dragon-box
  actual_status: draft
  required_status: approved
```

---

# 21. Schema and Versioning

All protocol files must contain a schema identifier.

Required schemas:

```yaml
schema: compositor.dev/story-flow/v1
schema: compositor.dev/composition-plan/v1
schema: compositor.dev/composition-overrides/v1
schema: compositor.dev/art-assets/v1
schema: compositor.dev/design-system/v1
schema: compositor.dev/resolved-spread/v1
schema: compositor.dev/production-package/v1
schema: compositor.dev/diagnostics/v1
```

Requirements:

* reject unsupported major versions
* allow additive compatible changes within a major version
* provide clear schema-validation errors
* keep protocol definitions separate from Rust implementation details
* provide example fixtures for every schema
* consider maintaining JSON Schema definitions even though YAML is the primary authoring format

---

# 22. Determinism

Given identical inputs and configuration, Compositor must produce identical outputs.

This includes:

* spread ordering
* directory names
* filenames
* manifests
* diagnostics ordering
* source hashes
* resolved asset paths
* copied asset contents
* semantic layout values
* assembly-guide ordering

Avoid timestamps in deterministic output.

If timestamps are needed for debugging, place them behind an explicit flag and exclude them from ordinary builds.

---

# 23. Output Safety

Compositor must not modify source files.

It must not modify:

* story Markdown
* flow plans
* composition plans
* overrides
* source artwork
* Affinity documents
* design-system files

Build output should be written only under the selected output directory.

A build may replace a previous generated package at that exact output path, but should do so safely.

Prefer:

1. build into a temporary sibling directory
2. validate the completed output
3. atomically replace the existing output directory where supported

---

# 24. Testing Requirements

Add unit and integration tests covering the following.

## 24.1 Markdown parsing

* valid comment-based IDs
* duplicate IDs
* malformed IDs
* orphaned identifier comments
* paragraph associations
* front matter
* page breaks
* preserved inline formatting
* comments excluded from reader-visible output

## 24.2 Flow validation

* complete source coverage
* overlapping ranges
* missing ranges
* reversed ranges
* invalid roles
* invalid energy values
* invalid page-turn transitions
* excessive energy clustering
* reveal without setup
* must-keep-together violations

## 24.3 Composition validation

* unknown layout family
* unknown layout variant
* incompatible role and layout family
* incompatible text density
* missing illustration metadata
* unknown art asset
* rejected art asset
* superseded art asset
* disallowed physical geometry

## 24.4 Art-policy handling

* draft build accepts draft, review, and approved
* review build accepts review and approved
* approved build accepts approved only
* requested asset warning in non-strict mode
* requested asset error in strict mode
* missing file warning in non-strict mode
* missing file error in strict mode
* stable generated filename across source revisions
* filename collision detection
* rejected assets are never selected
* superseded assets are never selected

## 24.5 Build output

* deterministic directory structure
* correct `text.md` extraction
* source comments preserved where expected
* correct art copying
* no asset modification
* valid `spread.yaml`
* valid root `manifest.yaml`
* valid diagnostics
* assembly guide contains required information
* build contains no excluded output formats

## 24.6 Overrides

* human overrides win
* locks are preserved
* incompatible locks produce errors
* unlocked fields may change
* provenance is correct
* generated operations do not modify override files

## 24.7 Golden tests

Include golden-file tests for:

* Story Flow Plan validation
* Composition Plan validation
* diagnostics
* resolved spread manifests
* production-package manifests
* assembly-guide output
* complete production-package directory trees

---

# 25. Migration Strategy

Do not rewrite the entire current system in one change.

Preserve the existing Edgar art-record workflow throughout this migration. Do
not introduce a replacement art workflow by implication: any asset-registry
integration begins only after its separate contract has been agreed.

Implement in the following phases.

## Phase 1: Durable source structure

* add discovery support for the target nested compendium/story layout
* plan and test a deliberate migration from flat story files before retiring the old layout
* implement comment-based source IDs
* implement prose-paragraph parsing and association
* implement source inspection
* implement source hashes
* implement source diagnostics
* implement the explicitly invoked source-preparation workflow

## Phase 2: Protocol foundations

* add schema identifiers
* add schema definitions
* add design-system vocabulary
* add Story Flow Plan validation
* add Composition Plan validation

## Phase 3: Codex skills

* implement `edgar-story-flow-planner`
* implement `edgar-layout-director`
* add example inputs and outputs
* ensure both skills use declared vocabulary only
* ensure skills preserve source IDs
* ensure skills understand human locks
* update `specify-story-art` and `specify-opener-art` to consume flow and composition intent without replacing their art-record workflow

## Phase 4: Artwork lifecycle integration

* hold a separate design discussion to decide the relationship between `art/briefs/*.yaml` and an asset registry
* document the accepted integration contract before implementation
* implement artwork lifecycle statuses
* implement stable asset IDs
* implement asset policy filtering
* implement strict and non-strict art modes

## Phase 5: Production package

* implement spread-oriented output
* emit `text.md`
* organize art assets
* emit `spread.yaml`
* emit root `manifest.yaml`
* emit diagnostics
* emit `assembly-guide.html`

## Phase 6: Overrides and reconciliation

* add override schema
* add locks
* add provenance
* add reconcile command

## Phase 7: Higher-level diagnostics

* pacing warnings
* repeated-layout warnings
* role imbalance
* missing quiet spreads
* reveal-without-setup warnings
* excessive text-density warnings
* art-readiness summary

Do not implement the following as part of this work:

* DOCX generation
* RTF generation
* proof rendering
* per-spread rendered previews
* flattened art
* `.afpub` generation
* physical layout coordinates

---

# 26. Acceptance Workflow

The following workflow must succeed.

## 26.1 Inspect the story

```bash
compositor inspect story.md
```

The story must already have complete paragraph identifiers. For an existing
unprepared manuscript, explicitly run the source-preparation workflow first,
review its Markdown changes, and then inspect the prepared source.

## 26.2 Generate the Story Flow Plan

```bash
codex skill run edgar-story-flow-planner \
  --story story.md \
  --design-system design-systems/edgar-v1 \
  --output story.flow.yaml
```

## 26.3 Validate the Story Flow Plan

```bash
compositor validate-flow \
  story.md \
  story.flow.yaml \
  --design-system design-systems/edgar-v1
```

## 26.4 Generate the Composition Plan

```bash
codex skill run edgar-layout-director \
  --story story.md \
  --flow story.flow.yaml \
  --design-system design-systems/edgar-v1 \
  --assets art/assets.yaml \
  --output hardcover-primary.composition.yaml
```

## 26.5 Validate the Composition Plan

```bash
compositor validate-composition \
  story.md \
  story.flow.yaml \
  hardcover-primary.composition.yaml \
  --design-system design-systems/edgar-v1 \
  --assets art/assets.yaml
```

## 26.6 Build an iterative package

```bash
compositor build \
  story.md \
  story.flow.yaml \
  hardcover-primary.composition.yaml \
  --design-system design-systems/edgar-v1 \
  --assets art/assets.yaml \
  --asset-policy draft \
  --output build/hardcover-primary/
```

## 26.7 Build an approved production package

```bash
compositor build \
  story.md \
  story.flow.yaml \
  hardcover-primary.composition.yaml \
  --design-system design-systems/edgar-v1 \
  --assets art/assets.yaml \
  --asset-policy approved \
  --strict-art \
  --output build/hardcover-primary-approved/
```

---

# 27. Acceptance Criteria

The work is complete when:

* Markdown comments provide durable IDs for every reader-visible prose paragraph.
* Custom Markdown attribute syntax is not required.
* Story Flow Plans can be generated and validated.
* Composition Plans can be generated and validated.
* Codex skills use only declared design-system vocabulary.
* Story Flow Planner never silently changes story Markdown; source preparation is explicit and reviewable.
* The existing art-record workflow remains intact while asset-lifecycle integration is decided and implemented.
* Compositor detects unassigned and multiply assigned source content.
* Compositor supports draft, review, and approved art policies.
* Draft artwork can be used during iterative builds.
* Approved-only builds fail when non-approved artwork is present.
* Stable asset IDs produce stable package filenames.
* Human overrides and locks survive regeneration.
* Build output is deterministic.
* Each spread contains:

```text
spread.yaml
text.md
art/
```

* The root package contains:

```text
manifest.yaml
diagnostics.yaml
assembly-guide.html
spreads/
```

* The build contains no:

```text
.afpub
.docx
.rtf
complete proof PDF
layout-reference PDF
flattened image
rendered spread preview
frame-specific prose file
physical page coordinates
```

---

# 28. Final Implementation Principle

The implementation should preserve this boundary:

> Compositor describes and validates the production intent, but it does not attempt to reproduce the visual editor.

Compositor should answer:

* What story content belongs on this spread?
* What is the narrative purpose of the spread?
* What semantic layout pattern should be used?
* What artwork belongs to the spread?
* What status is that artwork in?
* Is the plan structurally valid?
* Is the book's pacing coherent?
* What must the Affinity editor assemble?

Affinity Publisher should answer:

* Where exactly does the text sit?
* How large is the illustration?
* How is the image cropped?
* What typography is used?
* How do visual elements overlap?
* What final visual adjustments make the spread work?
