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

## Project Structure

- `crates/` — Core crates (arvak-ir, arvak-hal, arvak-compile, arvak-grpc, arvak-cli, etc.)
- `adapters/` — Backend adapters (arvak-adapter-sim, arvak-adapter-ibm, arvak-adapter-iqm, arvak-adapter-qdmi)
- `crates/arvak-python` — Python bindings (PyO3/maturin, needs Python venv — always exclude from workspace commands)

## Deployment

Use `/deploydemo` skill to deploy to arvak.io (Docker build + scp + ssh restart).
