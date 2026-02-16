# Arvak Project — Claude Code Guidelines

## Build & Test

```bash
# Standard check (excludes arvak-python which requires a Python venv)
cargo check --workspace --exclude arvak-python
cargo test --workspace --exclude arvak-python
cargo fmt --all

# Strict clippy (matches nightly CI — see .github/workflows/nightly.yml lines 247-299)
cargo clippy --workspace --exclude arvak-python --all-targets -- \
  -D warnings -D clippy::all -W clippy::pedantic \
  -A clippy::missing-errors-doc -A clippy::missing-panics-doc \
  -A clippy::module-name-repetitions -A clippy::must-use-candidate \
  -A clippy::return-self-not-must-use -A clippy::similar-names \
  -A clippy::many-single-char-names -A clippy::cast-possible-truncation \
  -A clippy::cast-precision-loss -A clippy::cast-sign-loss \
  -A clippy::cast-possible-wrap -A clippy::doc-markdown \
  -A clippy::wildcard-imports -A clippy::items-after-statements \
  -A clippy::implicit-hasher -A clippy::unnecessary-debug-formatting \
  -A clippy::struct-excessive-bools -A clippy::uninlined-format-args \
  -A clippy::trivially-copy-pass-by-ref -A clippy::float-cmp \
  -A clippy::unreadable-literal -A clippy::unused-self \
  -A clippy::match-same-arms -A clippy::needless-pass-by-value \
  -A clippy::format-push-string -A clippy::missing-fields-in-debug \
  -A clippy::too-many-lines -A clippy::no-effect-underscore-binding \
  -A clippy::unnecessary-wraps -A clippy::only-used-in-recursion \
  -A clippy::self-only-used-in-recursion -A clippy::needless-continue \
  -A clippy::redundant-else -A clippy::option-if-let-else \
  -A clippy::if-not-else -A clippy::manual-let-else \
  -A clippy::single-match-else -A clippy::used-underscore-binding \
  -A clippy::default-trait-access -A clippy::non-std-lazy-statics \
  -A clippy::unnecessary-sort-by -A clippy::type-complexity \
  -A clippy::too-many-arguments -A clippy::unused-async \
  -A clippy::ref-option -A clippy::semicolon-if-nothing-returned
```

Always run `cargo fmt --all` after editing Rust files. The nightly CI denies warnings with pedantic clippy.

## Coding Patterns (from nightly CI failures)

### Checked casts: use `try_from` instead of `as` with assertions
```rust
// BAD — triggers clippy::checked_conversions
debug_assert!(id <= u32::MAX as usize);
let val = id as u32;

// GOOD
let val = u32::try_from(id).expect("overflow: exceeds u32::MAX");
```

### Method references over redundant closures
```rust
// BAD — triggers clippy::redundant_closure
.unwrap_or_else(|e| e.into_inner())

// GOOD
.unwrap_or_else(std::sync::PoisonError::into_inner)
```

### `is_ok_and` over `map().unwrap_or(false)` on Result
```rust
// BAD — triggers clippy::map_unwrap_or
result.map(|a| a.is_available).unwrap_or(false)

// GOOD
result.is_ok_and(|a| a.is_available)
```

### Iterate references directly
```rust
// BAD — triggers clippy::explicit_iter_loop
for item in self.items.iter() { }

// GOOD
for item in &self.items { }
```

### Suppress `unnecessary_literal_bound` for trait impls returning `&'static str`
```rust
// When a Backend trait requires `fn name(&self) -> &str` and you return a literal:
#[allow(clippy::unnecessary_literal_bound)]
fn name(&self) -> &str {
    "my-backend"
}
```

## Rules (from Double Knuth audit)

### Security
- **Never use `innerHTML` with unsanitized data.** Always pass server-returned values through `escapeHtml()` or use `textContent`. The dashboard's `escapeHtml()` helper exists in `static/app.js`.
- **Never derive `Debug` on structs containing tokens or secrets.** Implement `Debug` manually with redacted fields (e.g., `field("token", &"[REDACTED]")`).
- **Create credential files with restrictive permissions atomically.** Use `OpenOptions::new().mode(0o600)` on Unix — never write then chmod (TOCTOU race).
- **CORS must be configurable**, not hardcoded to `Any`. Read from env var with graceful fallback — never `.expect()` on user-provided env vars.
- **HTTP clients must set timeouts.** Always add `.timeout()` and `.connect_timeout()` to reqwest client builders.

### Async & Concurrency
- **Use `tokio::sync::Mutex`/`RwLock` in async contexts**, not `std::sync::Mutex`. Holding a std Mutex across `.await` can deadlock.
- **Use `tokio::task::spawn_blocking()` for CPU-bound work** (e.g., simulation, heavy computation) inside async functions.
- **Caches need eviction.** Job caches, result caches, and in-memory storage must have a size limit (e.g., `MAX_CACHED_JOBS = 10_000`) with eviction of terminal-state entries.
- **Backend info caches need a TTL** (e.g., 5 minutes). Never cache indefinitely — backends go offline.

### Arithmetic & Overflow
- **Use `checked_add` for ID counters**, not `+=`. Wrapping on overflow silently produces duplicate IDs.
- **Use `try_from` for narrowing casts**, not `as`. This includes `u64 as u32`, config values, and shot counts. Provide a sensible fallback: `u32::try_from(v).unwrap_or(default)`.
- **Guard division** — check for zero denominators before dividing, especially for computed values like `total_shots`, `qubits_per_zone`.
- **Use `rem_euclid` for angle normalization**, not while loops. `while` loops on floats can hang for extreme values.

### API Consistency
- **`FromIterator`, `from_pairs`, and `insert` must have consistent semantics.** If `insert` accumulates, `FromIterator` and `from_pairs` must also accumulate.
- **Never silently skip unsupported operations.** Return an error instead — silently skipping gates/params produces incorrect results with no diagnostic.
- **`validate()` must check both qubit count and gate set.** Gate set validation catches unsupported gates at submission time rather than producing cryptic backend errors.
- **Batch operations must check resource limits per item**, not once before the loop. Increment counters after each successful item.
- **Gate set must cover all arities.** `GateSet::contains()` checks single-qubit, two-qubit, and three-qubit lists.

### Dependencies
- **Use workspace dependencies consistently.** Never use direct `path = "..."` when a `[workspace.dependencies]` entry exists.
- **Use `serde_yml`**, not `serde_yaml` (deprecated, RUSTSEC-2024-0320).
- **Use `tracing`**, not `log`. All crates use `tracing` — the `log` crate's macros won't appear in tracing subscriber output.

### DAG Operations
- **`dag.apply()` appends at wire ends, not at specific positions.** Passes that need positional insertion (routing SWAPs, noise channels) must document this limitation. A `CircuitDag::insert_before(node, inst)` API is needed for correct operation ordering.
- **`substitute_node` invalidates collected `NodeIndex` values** via petgraph's swap-remove. Process nodes in reverse order or re-discover after each substitution.

## Project Structure

- `crates/` — Core crates (arvak-ir, arvak-hal, arvak-compile, arvak-grpc, arvak-cli, etc.)
- `adapters/` — Backend adapters (arvak-adapter-sim, arvak-adapter-ibm, arvak-adapter-iqm, arvak-adapter-qdmi)
- `crates/arvak-python` — Python bindings (PyO3/maturin, needs Python venv — always exclude from workspace commands)

## Deployment

Use `/deploydemo` skill to deploy to arvak.io (Docker build + scp + ssh restart).
