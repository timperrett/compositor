# Rust Engineering Rules

These rules apply to all Rust code created or modified in this repository.

The objective is to produce code that is correct, explicit, testable, maintainable, and unsurprising. Prefer simple, composable designs over clever abstractions.

---

## 1. General Engineering Principles

### 1.1 Correctness before convenience

Prioritize, in order:

1. Correctness
2. Clarity
3. Testability
4. Maintainability
5. Performance
6. Concision

Do not sacrifice correctness or readability for small reductions in code size or speculative performance improvements.

### 1.2 Make invalid states difficult to represent

Use Rust’s type system to encode meaningful domain constraints.

Prefer:

* enums over loosely related booleans
* newtypes over interchangeable primitive values
* validated constructors over unrestricted public fields
* exhaustive `match` expressions over implicit fallthrough
* domain-specific types over unstructured strings
* explicit state transitions over mutating several related fields independently

Avoid “stringly typed” interfaces where a finite or structured domain is known.

### 1.3 Prefer explicit behavior

Avoid hidden control flow, surprising global state, or implicit side effects.

A caller should be able to understand:

* what data a function reads
* what data it modifies
* what errors it can produce
* whether it performs I/O
* whether it blocks
* whether it spawns background work

---

## 2. Functional Core, Imperative Shell

### 2.1 Prefer pure functions

Prefer functions that are referentially transparent:

* the return value depends only on the inputs
* the function does not mutate external state
* the function does not perform I/O
* the function does not read clocks, random generators, environment variables, or global configuration directly

Pure functions should contain the majority of business and domain logic.

```rust
pub fn calculate_total(items: &[LineItem], tax_rate: TaxRate) -> Money {
    let subtotal = items.iter().map(LineItem::total).sum::<Money>();
    subtotal + tax_rate.apply(subtotal)
}
```

### 2.2 Isolate side effects

Keep filesystem, network, database, clock, randomness, and process interaction at clearly defined boundaries.

Prefer this structure:

```text
input adapters
    ↓
parsing and validation
    ↓
pure domain logic
    ↓
result or command model
    ↓
output adapters
```

Do not intermingle parsing, I/O, domain decisions, and output formatting in a single function.

### 2.3 Pass dependencies explicitly

Do not access dependencies through global variables or hidden singletons.

Pass dependencies through:

* function arguments
* constructor parameters
* small context structs
* narrowly scoped traits at system boundaries

For nondeterministic inputs, inject the value or capability:

```rust
pub fn expire_session(
    session: &Session,
    now: DateTime<Utc>,
) -> SessionStatus {
    if now >= session.expires_at {
        SessionStatus::Expired
    } else {
        SessionStatus::Active
    }
}
```

Prefer passing `now` to reading the system clock inside domain logic.

### 2.4 Minimize mutation

Prefer immutable bindings.

Use `mut` only where mutation makes the implementation materially clearer or more efficient.

Keep mutation:

* local
* short-lived
* narrowly scoped
* invisible outside the function where possible

Do not use interior mutability to avoid designing clear ownership boundaries.

---

## 3. Error Handling

### 3.1 Use `Result` for recoverable failures

Functions that can fail in an expected or operationally meaningful way must return `Result<T, E>`.

Examples include:

* parsing
* validation
* filesystem access
* network calls
* database operations
* serialization
* configuration loading
* user-provided input
* external command execution

Do not represent failure using sentinel values such as:

* empty strings
* negative numbers
* magic enum variants unrelated to failure
* `false` where the reason matters

### 3.2 Use `Option` only for absence

Use `Option<T>` when a value may legitimately be absent and the absence is not itself an error.

Use `Result<Option<T>, E>` when both absence and failure are possible.

### 3.3 Avoid panics in production paths

Do not use the following in normal production code:

* `unwrap()`
* `expect()`
* `panic!()`
* `unreachable!()`
* indexing that may panic
* arithmetic that may overflow unexpectedly

These may be used in tests where the invariant under test is explicit.

A production `expect()` is acceptable only when the invariant is statically or structurally guaranteed and the accompanying message explains why failure is impossible.

### 3.4 Define meaningful error types

Library and domain code should use typed errors.

Prefer `thiserror` for concrete error enums:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration file was not found at {path}")]
    NotFound { path: PathBuf },

    #[error("configuration at {path} is invalid")]
    Invalid {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}
```

Application entry points may use `anyhow` to add context and simplify top-level error propagation.

Do not expose `anyhow::Error` from reusable library APIs unless the API is intentionally application-specific.

### 3.5 Preserve error sources

When converting errors:

* preserve the original source
* add actionable context
* do not discard diagnostic information
* do not convert every error to an opaque string

Use `#[source]`, `map_err`, or context methods appropriately.

### 3.6 Errors must be actionable

Error messages should explain:

* what operation failed
* which input or resource was involved
* why it failed, when known
* what the user or operator may need to correct

Avoid vague errors such as:

```text
invalid input
operation failed
something went wrong
```

### 3.7 Do not log and return the same error repeatedly

An error should normally be logged once, near the boundary where it is handled.

Lower layers should return enriched errors rather than logging every propagation step.

---

## 4. Async Rust

### 4.1 Use async only for asynchronous work

Use `async` for operations that spend meaningful time waiting on:

* network I/O
* asynchronous filesystem APIs
* database clients
* timers
* message queues
* subprocesses with async support

Do not make pure computation async.

Do not add `async` merely because the caller is async.

### 4.2 Keep domain logic synchronous

Async functions should generally orchestrate I/O and delegate computation to synchronous pure functions.

Prefer:

```rust
pub async fn load_and_evaluate(
    client: &Client,
    request: Request,
) -> Result<Decision, ServiceError> {
    let input = client.fetch(request).await?;
    let validated = validate_input(input)?;
    Ok(evaluate(validated))
}
```

### 4.3 Never block the async runtime

Do not call blocking operations directly from async tasks.

Blocking operations include:

* synchronous filesystem traversal
* long CPU-bound calculations
* blocking database clients
* `std::thread::sleep`
* blocking subprocess waits
* large compression or image-processing jobs

Use:

* an async-native API
* `spawn_blocking`
* a dedicated worker pool
* a separate synchronous subsystem

### 4.4 Avoid holding locks across `.await`

Never hold a `std::sync` or async lock guard across `.await` unless there is a documented and unavoidable reason.

Prefer:

1. acquire the lock
2. copy or move out the needed state
3. release the lock
4. await the external operation
5. reacquire the lock only if necessary

### 4.5 Bound concurrency

Do not spawn unbounded tasks for arbitrary-size input.

Use bounded concurrency mechanisms such as:

* semaphores
* bounded channels
* buffered streams
* worker pools
* explicit batch sizes

```rust
stream::iter(items)
    .map(|item| process(item))
    .buffer_unordered(MAX_CONCURRENCY)
    .collect::<Vec<_>>()
    .await
```

Concurrency limits should be named constants or configuration values.

### 4.6 Make task ownership explicit

Every spawned task must have a clear lifecycle.

Determine:

* who owns the task
* how errors are observed
* how cancellation occurs
* whether shutdown waits for completion
* whether abandoned work is acceptable

Do not silently discard `JoinHandle`s unless the task is intentionally detached and that decision is documented.

### 4.7 Support cancellation

Long-running async operations should be cancellation-safe where practical.

For services and workers, use an explicit cancellation mechanism such as a cancellation token or shutdown channel.

Select loops should handle shutdown deliberately:

```rust
loop {
    tokio::select! {
        _ = cancellation.cancelled() => break,
        Some(message) = receiver.recv() => {
            handle_message(message).await?;
        }
        else => break,
    }
}
```

### 4.8 Apply timeouts at external boundaries

Network calls, subprocesses, and remote service operations should normally have explicit timeouts.

Timeout values should be:

* configurable where operationally relevant
* named
* documented
* tested where practical

### 4.9 Do not use async traits indiscriminately

Introduce async traits only at meaningful architectural boundaries.

Prefer concrete types when polymorphism is not required.

Avoid creating traits solely to mock every dependency.

---

## 5. Ownership, Borrowing, and Data Design

### 5.1 Borrow when ownership is unnecessary

Prefer:

* `&str` over `&String`
* `&[T]` over `&Vec<T>`
* borrowed references over cloning
* iterators over intermediate collections

Accept owned values when the function:

* stores the value
* transforms it into another owned value
* transfers ownership
* simplifies a public boundary meaningfully

### 5.2 Avoid unnecessary cloning

Do not use `.clone()` merely to satisfy the borrow checker without understanding the ownership problem.

A clone is acceptable when:

* ownership genuinely must be duplicated
* the data is cheap to clone
* the clone simplifies an otherwise disproportionate design
* performance impact is understood

For non-obvious clones, structure the code so the ownership reason is clear.

### 5.3 Prefer domain structs over large parameter lists

Functions with many related parameters should use a typed input struct.

```rust
pub struct RenderRequest {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub format: OutputFormat,
    pub overwrite: bool,
}
```

As a guideline, reconsider any function with more than five parameters.

### 5.4 Keep public fields deliberate

Default to private struct fields.

Expose fields publicly only for simple data-transfer types where unrestricted construction and mutation are intentional.

Use constructors and accessor methods where invariants must be maintained.

---

## 6. Unsafe Rust

### 6.1 Avoid `unsafe`

Do not introduce `unsafe` code unless it is necessary to satisfy a concrete requirement that cannot reasonably be met with safe Rust.

Performance speculation is not sufficient justification.

### 6.2 Require documented justification

Every `unsafe` block must include a `SAFETY:` comment explaining the invariants that make the operation sound.

```rust
// SAFETY: `pointer` was created from `buffer`, remains within its allocation,
// is correctly aligned for `Header`, and `buffer` outlives the returned reference.
unsafe { &*(pointer.cast::<Header>()) }
```

### 6.3 Minimize unsafe scope

Unsafe blocks must be as small as possible.

Wrap unsafe implementation details behind a safe API whose contract is documented and tested.

Do not mark an entire function `unsafe` when only one operation requires an unsafe block.

### 6.4 Test unsafe code aggressively

Code containing unsafe operations requires:

* unit tests for documented invariants
* boundary tests
* property tests where applicable
* Miri execution where supported
* sanitizer testing where practical
* review of aliasing, lifetime, alignment, and initialization assumptions

### 6.5 Prefer maintained safe abstractions

Before introducing unsafe code, evaluate whether a mature crate already provides a safe and well-tested abstraction.

Avoid writing custom unsafe containers, synchronization primitives, or pointer abstractions without a compelling project-specific need.

---

## 7. Module and File Structure

### 7.1 Keep files focused

A Rust source file should generally not exceed approximately 500 lines of code.

This is a design threshold, not a target.

When a file approaches 500 lines, inspect whether it contains:

* multiple responsibilities
* unrelated types
* distinct adapters
* excessive implementation detail
* tests that should move to a dedicated test module
* parsing and domain logic that should be separated

Generated files and declarative lookup tables may exceed this limit when splitting them would reduce clarity.

### 7.2 Organize by responsibility

Prefer modules that represent domain concepts or cohesive capabilities.

Avoid generic dumping-ground modules such as:

* `utils`
* `helpers`
* `common`
* `misc`

A utility should normally live beside the concept it supports.

### 7.3 Keep module depth modest

Avoid deeply nested module hierarchies.

Use nesting where it communicates architecture, not merely to reduce file size.

### 7.4 Control the public API

Use the narrowest visibility that works:

* private by default
* `pub(crate)` for crate-internal interfaces
* `pub` only for intentional external API

Do not expose implementation details unnecessarily.

### 7.5 Separate major concerns

Where applicable, separate:

* domain models
* parsing
* validation
* application orchestration
* persistence
* external service clients
* presentation or serialization
* CLI handling
* configuration

---

## 8. Function and Type Design

### 8.1 Keep functions cohesive

A function should perform one coherent operation at one level of abstraction.

Extract logic when a function combines several distinct phases, such as:

* reading
* parsing
* validating
* transforming
* persisting
* reporting

### 8.2 Prefer understandable control flow

Use early returns to reduce nesting.

```rust
fn process(record: Record) -> Result<Output, Error> {
    if record.is_empty() {
        return Err(Error::EmptyRecord);
    }

    let validated = validate(record)?;
    transform(validated)
}
```

Avoid deeply nested `if`, `match`, or iterator chains that obscure the sequence of operations.

### 8.3 Use iterator chains judiciously

Iterator chains are preferred when they remain easy to read.

Split a chain into named intermediate values when:

* several transformations have different meanings
* error handling becomes difficult to follow
* closures become long
* type inference becomes obscure
* debugging would benefit from named stages

### 8.4 Avoid boolean arguments where meaning is unclear

Do not write:

```rust
render(document, true, false)
```

Prefer an options struct or expressive enum.

```rust
render(
    document,
    RenderOptions {
        overwrite: Overwrite::Allowed,
        diagnostics: Diagnostics::Disabled,
    },
)
```

### 8.5 Prefer exhaustive enums

Use enums for finite state and exhaustive matching.

Avoid wildcard match arms when adding a new variant should require revisiting the logic.

```rust
match status {
    Status::Pending => ...
    Status::Running => ...
    Status::Complete => ...
}
```

---

## 9. Testing Strategy

Testing should emphasize observable behavior, invariants, and failure modes rather than implementation details.

### 9.1 Testing layers

Use a balanced test suite containing:

1. Unit tests for pure functions and local behavior
2. Property tests for invariants and critical transformations
3. Integration tests for subsystem boundaries
4. End-to-end tests for a small number of essential workflows
5. Regression tests for every fixed defect

### 9.2 Unit tests

Unit tests should cover:

* normal behavior
* boundary values
* empty inputs
* malformed inputs
* state transitions
* error variants
* domain invariants

Keep unit tests deterministic and fast.

Test public or meaningful internal behavior. Avoid asserting incidental implementation details.

### 9.3 Property-based testing

Use property-based tests for critical-path code where broad input exploration provides meaningful assurance.

Strong candidates include:

* parsers
* serializers
* format conversions
* normalization
* ordering and grouping logic
* pagination
* state machines
* arithmetic
* identifier generation
* path manipulation
* round-trip transformations
* deterministic layout or allocation logic

Use a framework such as `proptest`.

Examples of useful properties:

#### Round-trip property

```text
decode(encode(value)) == value
```

#### Idempotence property

```text
normalize(normalize(value)) == normalize(value)
```

#### Conservation property

```text
sum(partition(input)) == sum(input)
```

#### Ordering property

```text
sort(sort(input)) == sort(input)
```

#### Parser safety property

```text
The parser never panics for arbitrary byte input.
```

#### Determinism property

```text
The same valid input and configuration always produce the same output.
```

Property tests must define meaningful invariants. Do not create property tests that merely reproduce the implementation.

### 9.4 Critical-path requirements

Critical-path code must have:

* direct unit coverage
* property tests for core invariants
* negative and boundary cases
* explicit regression fixtures
* integration coverage at relevant boundaries

Critical-path code includes anything that may:

* lose or corrupt data
* produce persistent output
* control authorization
* calculate money or quotas
* determine state transitions
* parse project source files
* generate identifiers or paths
* orchestrate irreversible actions

### 9.5 Integration tests

Integration tests should validate behavior across real module boundaries.

Prefer realistic implementations when they are deterministic and inexpensive.

Use fakes or test doubles for:

* remote services
* clocks
* random sources
* expensive infrastructure
* failure injection

Do not mock internal implementation details excessively.

### 9.6 End-to-end tests

Maintain a small end-to-end suite for essential user workflows.

End-to-end tests should verify:

* process-level behavior
* CLI exit codes where applicable
* output files
* externally visible diagnostics
* configuration loading
* compatibility between major components

Keep the suite small enough to remain reliable.

### 9.7 Regression tests

Every bug fix should include a test that:

1. fails before the fix
2. passes after the fix
3. captures the underlying failure mode

Name or comment the test so the historical reason is discoverable.

### 9.8 Snapshot testing

Snapshot tests are appropriate for:

* diagnostics
* generated text
* structured output
* CLI help
* stable serialized representations

Snapshots must be reviewed carefully.

Do not accept broad snapshot updates without determining why each output changed.

Prefer structured assertions when only a few fields matter.

### 9.9 Test determinism

Tests must not depend implicitly on:

* wall-clock time
* network access
* process scheduling
* random seeds
* local timezone
* machine-specific paths
* global environment state
* test execution order

Inject or fix these inputs.

### 9.10 Test naming

Test names should describe behavior and expected outcome.

Prefer:

```rust
#[test]
fn rejects_duplicate_scene_identifiers() {}

#[test]
fn preserves_input_order_for_equal_priorities() {}
```

Avoid names such as:

```rust
#[test]
fn test_parser() {}
```

### 9.11 Test structure

Use Arrange–Act–Assert where it improves readability.

Keep setup focused. Create reusable fixtures or builders when repeated setup obscures the behavior being tested.

---

## 10. Validation and Parsing

### 10.1 Separate parsing from validation

Parsing answers:

> Can this input be structurally interpreted?

Validation answers:

> Is the interpreted value allowed by the domain?

Keep these phases distinct where practical.

### 10.2 Validate at system boundaries

Validate untrusted data when it enters the system.

After validation, prefer domain types that allow internal code to rely on established invariants.

### 10.3 Return complete diagnostics where useful

For document, configuration, or source validation, consider collecting multiple independent validation failures instead of returning only the first one.

Do not continue after errors that make subsequent validation unreliable.

### 10.4 Preserve source location

For source-file diagnostics, retain location information where possible:

* file
* line
* column
* byte span
* field or section name

Diagnostics should help the user correct the input.

---

## 11. Collections and Ordering

### 11.1 Choose collections deliberately

Use:

* `Vec<T>` for ordered sequences
* `HashMap<K, V>` for unordered key lookup
* `BTreeMap<K, V>` when deterministic key ordering matters
* `HashSet<T>` for unordered uniqueness
* `BTreeSet<T>` for deterministic ordered uniqueness
* `VecDeque<T>` for queue behavior

Do not rely on `HashMap` or `HashSet` iteration order.

### 11.2 Preserve determinism

Generated output should be deterministic unless nondeterminism is an explicit requirement.

Sort data before serialization or output when the source collection has unstable iteration order.

Deterministic output is especially important for:

* generated files
* manifests
* lock-like data
* diagnostics
* snapshots
* tests
* reproducible builds

---

## 12. Logging and Observability

### 12.1 Use structured logging

Prefer structured fields over interpolated prose.

```rust
tracing::info!(
    document_id = %document.id,
    page_count = pages.len(),
    "document rendered"
);
```

### 12.2 Use appropriate log levels

* `error`: operation failed and requires attention
* `warn`: degraded or suspicious behavior
* `info`: meaningful lifecycle or business events
* `debug`: detailed diagnostic information
* `trace`: highly granular execution detail

Do not use `error` for expected user-input failures unless the application context warrants it.

### 12.3 Do not log secrets

Never log:

* passwords
* API keys
* authentication tokens
* private keys
* session cookies
* unredacted credentials
* sensitive configuration values

Treat user-provided content according to the project’s data sensitivity requirements.

### 12.4 Instrument boundaries

Add tracing around important boundaries such as:

* external requests
* persistent writes
* long-running jobs
* parsing and compilation phases
* task execution
* retry loops

Avoid instrumenting every trivial function.

---

## 13. Performance

### 13.1 Measure before optimizing

Do not introduce complexity based on assumed bottlenecks.

Use profiling, benchmarking, or representative measurements before making non-obvious performance changes.

### 13.2 Benchmark critical algorithms

Use Criterion or an equivalent framework for performance-sensitive code.

Benchmarks should use representative inputs and record the scenario being measured.

### 13.3 Avoid accidental quadratic behavior

Review loops involving:

* repeated linear searches
* repeated string concatenation
* inserting at the front of vectors
* rescanning entire collections
* repeated sorting
* nested iteration over large inputs

Use a more appropriate data structure when input size makes it necessary.

### 13.4 Avoid premature allocation optimization

Prefer clear owned data initially.

Introduce borrowing, arenas, compact representations, or custom allocation only where measurements demonstrate meaningful benefit.

---

## 14. Dependencies

### 14.1 Minimize dependencies

Add a dependency only when it provides clear value over a small local implementation or standard-library capability.

Evaluate:

* maintenance activity
* adoption
* license
* transitive dependency count
* compile-time impact
* security history
* feature granularity
* MSRV compatibility

### 14.2 Disable unnecessary default features

Where appropriate:

```toml
dependency = { version = "...", default-features = false, features = ["required-feature"] }
```

Do not enable broad feature sets without need.

### 14.3 Keep dependency roles clear

Avoid adding multiple crates that solve the same problem unless there is a documented architectural reason.

### 14.4 Review dependency changes

Any dependency addition or major version change should include:

* the reason it is needed
* why the selected crate is appropriate
* relevant feature flags
* any security or portability implications

---

## 15. Documentation

### 15.1 Document public APIs

Public types and functions should explain:

* purpose
* important invariants
* errors
* panics, if any
* safety requirements, if any
* notable side effects

### 15.2 Explain why, not what

Comments should explain:

* non-obvious constraints
* architectural decisions
* compatibility behavior
* invariant reasoning
* safety assumptions
* why a simpler-looking alternative is incorrect

Do not narrate straightforward code.

### 15.3 Keep documentation accurate

When behavior changes, update:

* rustdoc
* module documentation
* examples
* README files
* architecture notes
* CLI help
* configuration references

Outdated documentation is a defect.

---

## 16. Formatting and Linting

All code must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Do not suppress Clippy warnings without understanding them.

A lint suppression must:

* be narrowly scoped
* include a reason when it is not self-evident
* not conceal a broader design problem

Prefer fixing the underlying issue.

Repository configuration should enable useful lints centrally where practical.

Recommended baseline:

```toml
[lints.rust]
unsafe_code = "deny"

[lints.clippy]
all = "warn"
pedantic = "warn"
```

Enable or relax individual pedantic lints deliberately based on project needs.

If the repository contains justified unsafe code, replace the crate-wide `unsafe_code = "deny"` policy with narrowly scoped allowances and documented review requirements.

---

## 17. Required Verification

Before considering a change complete, run the checks relevant to the affected workspace.

Default verification:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
```

Where supported, also run:

```bash
cargo test --doc --workspace
cargo nextest run --workspace --all-features
cargo audit
cargo deny check
```

For code with unsafe behavior or complex aliasing:

```bash
cargo miri test
```

For property-test-heavy code, run enough cases to provide meaningful coverage in CI, while allowing larger case counts in scheduled or pre-release test jobs.

---

## 18. Continuous Integration Regime

### 18.1 Pull-request checks

Every pull request should run:

1. formatting check
2. compilation of all targets
3. Clippy with warnings denied
4. unit tests
5. integration tests
6. documentation tests
7. property tests using the standard case count
8. dependency policy checks
9. security advisory checks

### 18.2 Main-branch or scheduled checks

Run broader checks on the main branch or on a schedule:

* larger property-test case counts
* tests under multiple feature combinations
* tests on supported operating systems
* MSRV validation
* Miri
* sanitizer runs
* fuzz testing
* ignored or expensive tests
* performance regression benchmarks

### 18.3 Feature testing

If the crate exposes Cargo features:

* test default features
* test no default features
* test meaningful feature combinations
* ensure optional features remain independently compilable where intended

Do not assume `--all-features` is sufficient when features are mutually exclusive.

---

## 19. Fuzzing

Use fuzz testing for externally supplied or structurally complex input, especially:

* parsers
* decoders
* binary formats
* source-file processing
* path handling
* decompression
* protocol handling

Fuzz targets should assert at minimum that:

* the code does not panic
* the code does not violate safety assumptions
* invalid input returns an error
* successful parsing satisfies expected invariants

Promote discovered failures into permanent regression tests.

---

## 20. Security and Resource Limits

### 20.1 Treat external input as untrusted

Validate:

* lengths
* counts
* nesting depth
* paths
* identifiers
* encoding
* numeric bounds

### 20.2 Bound resource consumption

Where input is untrusted or potentially large, enforce limits on:

* file size
* allocation size
* recursion depth
* collection length
* concurrent operations
* retries
* timeouts
* generated output size

### 20.3 Avoid path traversal

When handling filesystem paths:

* normalize carefully
* define whether absolute paths are allowed
* reject traversal outside configured roots
* do not assume string-prefix checks establish containment
* handle symlinks according to an explicit policy

### 20.4 Avoid command injection

Prefer direct process argument APIs:

```rust
Command::new("tool")
    .arg("--input")
    .arg(input_path)
```

Do not construct shell command strings from untrusted input.

---

## 21. Change Discipline

### 21.1 Keep changes focused

Do not combine unrelated refactoring, formatting, dependency updates, and behavior changes in one change unless necessary.

### 21.2 Preserve behavior during refactoring

A refactor should not change externally visible behavior unless the behavior change is intentional and tested.

### 21.3 Avoid speculative abstractions

Do not introduce:

* generic frameworks for one implementation
* traits with only one foreseeable implementation
* configurable behavior without a requirement
* extension points based solely on hypothetical future needs

Prefer the simplest design that cleanly supports current requirements.

### 21.4 Remove dead code

Do not leave:

* commented-out implementations
* obsolete compatibility paths
* unused abstractions
* abandoned feature flags
* stale TODOs

Use version control rather than retaining dead code in source files.

---

## 22. Codex Implementation Procedure

When implementing a change, follow this sequence.

### Step 1: Understand the existing design

Before editing:

* inspect relevant modules
* identify existing conventions
* locate tests
* understand public APIs
* identify invariants and side effects
* check for repository-specific instructions

Do not introduce a new architectural pattern when an adequate existing pattern is already used.

### Step 2: Define the behavioral change

State internally:

* expected inputs
* expected outputs
* failure modes
* invariants
* compatibility requirements
* testing strategy

### Step 3: Design the smallest coherent change

Prefer:

* pure domain logic
* explicit typed errors
* narrow interfaces
* deterministic behavior
* isolated I/O
* bounded async work

### Step 4: Write or update tests

For defect fixes, create the regression test before or alongside the fix.

For new critical-path logic, include property tests.

### Step 5: Implement

Keep functions and modules focused.

Do not broaden scope without a concrete requirement.

### Step 6: Verify

Run formatting, compilation, linting, and tests.

Do not claim successful verification for commands that were not run.

### Step 7: Review the diff

Before completion, inspect for:

* accidental public API changes
* unnecessary clones
* hidden blocking in async code
* discarded errors
* nondeterministic output
* files approaching the LOC threshold
* missing tests
* stale comments
* new dependencies
* introduced unsafe code

---

## 23. Completion Criteria

A change is complete only when:

* behavior is implemented
* relevant tests exist
* critical invariants are tested
* errors are typed and actionable
* async work is bounded and cancellation-aware where relevant
* no unnecessary unsafe code was introduced
* source files remain reasonably focused
* formatting passes
* Clippy passes with warnings denied
* relevant tests pass
* documentation is updated
* verification results are reported accurately

---

## 24. Prohibited Patterns

Do not introduce the following without explicit justification:

* production `unwrap()` or `expect()` on fallible input
* unbounded task spawning
* blocking operations on an async executor
* locks held across `.await`
* ignored `Result` values
* opaque string errors in reusable library APIs
* undocumented unsafe blocks
* global mutable state
* reliance on hash iteration order
* shell command construction from user input
* large multipurpose modules
* files materially exceeding 500 lines without a structural reason
* mocks for pure functions
* traits created solely for speculative flexibility
* nondeterministic tests
* test assertions tied to irrelevant implementation details
* silent fallback behavior that conceals invalid input
* broad lint suppressions
* unrelated refactoring during targeted fixes

---

## 25. Default Decision Rules

When several implementations are possible, prefer the one that:

1. uses safe Rust
2. keeps domain logic pure
3. makes errors explicit with `Result`
4. preserves deterministic behavior
5. minimizes shared mutable state
6. keeps async orchestration at system boundaries
7. is straightforward to unit test
8. supports property testing of important invariants
9. has the smallest reasonable public API
10. can be understood without extensive commentary
