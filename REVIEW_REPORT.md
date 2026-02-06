# Arvak Code Review Council Report

**Repository**: Arvak — Rust-native Quantum Compilation Platform
**Review Date**: 2026-02-04
**Toolchain**: rustc 1.93.0, cargo 1.93.0
**Edition**: 2024 (rust-version 1.85)
**Total Lines**: ~10,337 across 53 .rs files
**Test Suite**: 89 tests passing
**Clippy**: Clean (no warnings)
**Unsafe Code**: None

---

## Executive Summary

Arvak is a well-architected quantum compilation platform with clean separation of concerns across its workspace crates. The codebase demonstrates strong Rust fundamentals: no unsafe code, clean clippy output, comprehensive test coverage for core functionality, and consistent use of the 2024 edition features.

**Strengths**:
- Clear layered architecture (IR → HAL → Compile → Adapters → CLI)
- Fluent builder APIs throughout (Circuit, PassManager, CustomGate)
- Comprehensive gate coverage with IQM-native PRX support
- DAG-based circuit representation enabling efficient compilation

**Areas for Improvement**:
- Error types could be more granular with context
- Several `#[allow(dead_code)]` annotations suggest incomplete features
- Missing `#[must_use]` on key builder methods
- Some API inconsistencies in return types

**Overall Assessment**: Production-ready foundation with minor API polish needed before 1.0.

---

## Risk Register

| ID | Risk | Severity | Likelihood | Mitigation |
|----|------|----------|------------|------------|
| R1 | Parser incomplete for control flow | Medium | High | Document limitations; implement gradually |
| R2 | No circuit validation before compilation | Medium | Medium | Add validation pass or builder-time checks |
| R3 | Statevector exponential memory | High | Low | Document qubit limits; add safeguards |
| R4 | Custom gates skip simulation | Low | Medium | Document behavior; add matrix support |
| R5 | HashMap iteration order non-deterministic | Low | Low | Use IndexMap for reproducible outputs |

---

## Findings by Severity

### BLOCKER (0 findings)

No blocking issues found. The codebase is in good shape for its current development stage.

---

### MAJOR (3 findings)

#### M1: Error Types Lack Context for Debugging

**Location**: `crates/arvak-ir/src/error.rs:8-40`

**Lens**: A (API/Ergonomics)

**Description**: Error types use raw IDs without positional context, making debugging difficult for users.

```rust
// Current
#[error("Qubit {0:?} not found in circuit")]
QubitNotFound(QubitId),
```

**Recommendation**: Include operation context in errors.

```rust
#[error("Qubit {qubit:?} not found in circuit at operation {op_index}")]
QubitNotFound { qubit: QubitId, op_index: Option<usize> },
```

**Evidence**: [RS-API § C-GOOD-ERR] — Error messages should be clear and actionable.

**Effort**: Medium

---

#### M2: Parser Silently Ignores Unsupported Constructs

**Location**: `crates/arvak-qasm3/src/parser.rs:770-798`

**Lens**: C (Correctness)

**Description**: Control flow statements (if/for) and custom gate definitions return errors but some assignments silently succeed doing nothing.

```rust
Statement::Assignment { .. } => {
    // Classical assignments - skip for now
    Ok(())
}
```

**Recommendation**: Either implement the feature or return a clear `Unimplemented` error variant with the construct name.

**Evidence**: [RS-BOOK § Error Handling] — Functions should not silently ignore failures.

**Effort**: Low

---

#### M3: Missing Qubit Count Validation in DAG

**Location**: `crates/arvak-ir/src/dag.rs:159-228`

**Lens**: C (Correctness)

**Description**: The `apply` method validates qubits exist but doesn't validate that gate qubit count matches the instruction qubit count.

```rust
pub fn apply(&mut self, instruction: Instruction) -> IrResult<NodeIndex> {
    // Validates qubits exist - good
    // Does NOT validate: gate.num_qubits() == instruction.qubits.len()
}
```

**Recommendation**: Add explicit arity validation.

```rust
if let InstructionKind::Gate(gate) = &instruction.kind {
    if gate.num_qubits() as usize != instruction.qubits.len() {
        return Err(IrError::QubitCountMismatch {
            expected: gate.num_qubits(),
            got: instruction.qubits.len() as u32,
        });
    }
}
```

**Evidence**: [RS-API § C-VALIDATE] — APIs should validate invariants at boundaries.

**Effort**: Low

---

### MINOR (8 findings)

#### m1: Missing `#[must_use]` on Builder Methods

**Location**: Multiple files

**Lens**: A (API/Ergonomics)

**Description**: Builder pattern methods that return `Self` should be marked `#[must_use]` to prevent accidental discarding.

**Files Affected**:
- `crates/arvak-ir/src/gate.rs:270-279` — `CustomGate::with_params`, `with_matrix`
- `crates/arvak-ir/src/gate.rs:334-343` — `Gate::with_label`, `with_condition`
- `crates/arvak-compile/src/manager.rs:101-117` — `PassManagerBuilder` methods

**Recommendation**: Add `#[must_use]` attribute.

```rust
#[must_use]
pub fn with_params(mut self, params: Vec<ParameterExpression>) -> Self {
    self.params = params;
    self
}
```

**Evidence**: [RS-CLIPPY § must_use_candidate] — Builder methods should be marked must_use.

**Effort**: Trivial

---

#### m2: Inconsistent Return Types in Circuit API

**Location**: `crates/arvak-ir/src/circuit.rs`

**Lens**: A (API/Ergonomics)

**Description**: Gate methods return `IrResult<&mut Self>` for chaining, but accessors don't follow a consistent pattern. Some return owned values, some return references.

```rust
pub fn qubits(&self) -> &[Qubit]           // Returns slice
pub fn num_qubits(&self) -> usize          // Returns owned
pub fn dag(&self) -> &CircuitDag           // Returns ref
pub fn into_dag(self) -> CircuitDag        // Consumes
```

**Recommendation**: Document the accessor pattern explicitly in module docs and consider adding `qubits_len()` alias for consistency with `is_empty()` patterns.

**Evidence**: [RS-API § C-CONV] — Conversion methods should follow consistent naming.

**Effort**: Low

---

#### m3: `FxHashMap` Iteration Order Non-Deterministic

**Location**: `crates/arvak-ir/src/dag.rs:364-371`

**Lens**: B (Performance) / C (Correctness)

**Description**: `qubits()` and `clbits()` iterate over `FxHashMap` keys, which has non-deterministic order. This could cause non-reproducible compilation outputs.

```rust
pub fn qubits(&self) -> impl Iterator<Item = QubitId> + '_ {
    self.qubit_inputs.keys().copied()
}
```

**Recommendation**: Consider using `indexmap::IndexMap` or sort the output for reproducibility in tests and debugging.

**Evidence**: [RS-PERF § Hash Tables] — Use appropriate map types for determinism requirements.

**Effort**: Medium

---

#### m4: Dead Code Markers Suggest Incomplete Features

**Location**: Multiple files

**Lens**: D (Architecture)

**Description**: Several `#[allow(dead_code)]` annotations indicate planned but unfinished functionality:

- `crates/arvak-qasm3/src/parser.rs:19` — `parse_ast` function
- `adapters/arvak-adapter-sim/src/statevector.rs:29` — `num_qubits` method

**Recommendation**: Either remove dead code or track completion in issues. Dead code increases maintenance burden.

**Evidence**: [RS-CLIPPY § dead_code] — Unused code should be removed or gated behind features.

**Effort**: Low

---

#### m5: Missing `Clone` Derive on `PassManager`

**Location**: `crates/arvak-compile/src/manager.rs:13-16`

**Lens**: A (API/Ergonomics)

**Description**: `PassManager` cannot be cloned because it holds `Box<dyn Pass>`. This limits flexibility for users who want to run the same passes on multiple circuits.

```rust
pub struct PassManager {
    passes: Vec<Box<dyn Pass>>,
}
```

**Recommendation**: Either:
1. Accept `Arc<dyn Pass>` instead of `Box<dyn Pass>`
2. Document that `PassManager` is not clonable by design
3. Add a builder that can produce multiple instances

**Evidence**: [RS-API § C-COMMON-TRAITS] — Consider implementing Clone where sensible.

**Effort**: Medium

---

#### m6: Statevector Memory Grows Exponentially

**Location**: `adapters/arvak-adapter-sim/src/statevector.rs:18-26`

**Lens**: B (Performance)

**Description**: Statevector allocates `2^n` complex numbers. At 30 qubits this is 16GB. No safeguard exists.

```rust
pub fn new(num_qubits: usize) -> Self {
    let size = 1 << num_qubits;  // Exponential!
    let mut amplitudes = vec![Complex64::new(0.0, 0.0); size];
}
```

**Recommendation**: Add a maximum qubit check (e.g., 26 qubits = 1GB) or use a sparse representation for states with limited entanglement.

```rust
const MAX_STATEVECTOR_QUBITS: usize = 26;

pub fn new(num_qubits: usize) -> Result<Self, SimError> {
    if num_qubits > MAX_STATEVECTOR_QUBITS {
        return Err(SimError::TooManyQubits { max: MAX_STATEVECTOR_QUBITS, got: num_qubits });
    }
    // ...
}
```

**Evidence**: [RS-PERF § Heap Allocations] — Avoid unbounded allocations.

**Effort**: Low

---

#### m7: Topology `is_connected` Does Linear Scan

**Location**: `crates/arvak-hal/src/capability.rs:216-220`

**Lens**: B (Performance)

**Description**: `is_connected` does O(E) scan for each query. For routing algorithms this could be called O(V²) times.

```rust
pub fn is_connected(&self, q1: u32, q2: u32) -> bool {
    self.edges
        .iter()
        .any(|&(a, b)| (a == q1 && b == q2) || (a == q2 && b == q1))
}
```

**Recommendation**: Build an adjacency set on construction for O(1) lookups.

```rust
pub struct Topology {
    pub kind: TopologyKind,
    pub edges: Vec<(u32, u32)>,
    adjacency: FxHashSet<(u32, u32)>,  // Add this
}
```

**Evidence**: [RS-PERF § Data Structures] — Choose data structures appropriate for access patterns.

**Effort**: Medium

---

#### m8: Missing Documentation on Public Types

**Location**: Various

**Lens**: A (API/Ergonomics)

**Description**: Several public types lack crate-level documentation or examples:

- `arvak_compile::PropertySet` — no usage examples
- `arvak_hal::Capabilities` — fields documented but no constructor examples
- `arvak_ir::ParameterExpression` — symbolic math not explained

**Recommendation**: Add `# Examples` sections to key types.

**Evidence**: [RS-API § C-EXAMPLE] — Public items should have examples.

**Effort**: Medium

---

### NIT (5 findings)

#### n1: Consider `#[non_exhaustive]` on Public Enums

**Location**: `crates/arvak-ir/src/error.rs`, `crates/arvak-hal/src/capability.rs:224-236`

**Lens**: D (Architecture)

**Description**: Public enums like `IrError` and `TopologyKind` should be `#[non_exhaustive]` to allow adding variants without breaking changes.

**Evidence**: [RS-API § C-STRUCT-PRIVATE] — Use non_exhaustive for future-proofing.

**Effort**: Trivial

---

#### n2: Use `std::mem::take` Instead of Clone + Clear

**Location**: None found — included as proactive check

**Description**: No instances found, but keep in mind for future code.

**Evidence**: [RS-CLIPPY § mem_replace_with_default]

---

#### n3: Consider `Cow<str>` for Gate Names

**Location**: `crates/arvak-ir/src/gate.rs:248`

**Lens**: B (Performance)

**Description**: `CustomGate::name` is always owned. For standard gates that return `&'static str`, there's an allocation cost when converting.

**Recommendation**: Use `Cow<'static, str>` if allocation-free paths are needed.

**Evidence**: [RS-PERF § String Handling] — Use Cow for mixed owned/borrowed strings.

**Effort**: Low

---

#### n4: Test Helpers Should Be in Separate Module

**Location**: `adapters/arvak-adapter-sim/src/statevector.rs:474-476`

**Lens**: D (Architecture)

**Description**: `approx_eq` helper is defined inline in tests. Consider a `test_utils` module for shared test infrastructure.

**Evidence**: [RS-CARGO § Test Organization]

**Effort**: Trivial

---

#### n5: Use `expect` Instead of `unwrap` with Messages

**Location**: `crates/arvak-ir/src/dag.rs:233-234`

**Lens**: C (Correctness)

**Description**: `toposort` returns `Result` but is handled with `unwrap_or_default()`. Consider using `expect` with a message or propagating the error for better debugging.

```rust
let sorted: Vec<_> = petgraph::algo::toposort(&self.graph, None)
    .unwrap_or_default()  // Silently returns empty on cycle
```

**Evidence**: [RS-CLIPPY § unwrap_used] — Prefer expect for clearer panic messages.

**Effort**: Trivial

---

## Architecture Recommendations

### 1. Consider Trait-Based Gate Representation

The current `StandardGate` enum with 27+ variants works but makes adding new gates cumbersome. Consider a trait-based approach for extensibility:

```rust
pub trait GateDefinition: Send + Sync {
    fn name(&self) -> &str;
    fn num_qubits(&self) -> u32;
    fn matrix(&self) -> Option<Matrix>;
    fn parameters(&self) -> &[ParameterExpression];
}
```

**Trade-off**: More flexible but loses exhaustive pattern matching. Current approach is fine for a controlled gate set.

### 2. Add Circuit Validation Layer

Currently, invalid circuits can be constructed and will fail during compilation. Consider a `Circuit::validate()` method or compile-time validation:

```rust
impl Circuit {
    pub fn validate(&self) -> IrResult<()> {
        // Check qubit indices in range
        // Verify gate arities
        // Detect disconnected qubits
    }
}
```

### 3. Separate Parsing from Lowering

`arvak-qasm3` combines parsing and lowering. Separating them enables:
- AST-level transformations
- Better error recovery
- Multiple lowering targets

The `parse_ast` function exists but is marked `#[allow(dead_code)]`. Expose and document it.

### 4. Consider Feature-Gated Simulation Backends

The statevector simulator is always compiled. For CLI builds that don't need simulation, consider feature-gating:

```toml
[features]
default = ["simulator"]
simulator = ["num-complex", "rand"]
```

---

## 2-Week Refactor Plan

### Week 1: API Polish

| Day | Task | Files | Effort |
|-----|------|-------|--------|
| 1-2 | Add `#[must_use]` to all builder methods | gate.rs, manager.rs | S |
| 2 | Add `#[non_exhaustive]` to public enums | error.rs, capability.rs | XS |
| 3 | Implement qubit arity validation in DAG::apply | dag.rs | S |
| 4 | Add context to error types (op_index) | error.rs, dag.rs | M |
| 5 | Document `PropertySet` with examples | property.rs | S |

### Week 2: Robustness

| Day | Task | Files | Effort |
|-----|------|-------|--------|
| 1 | Add max qubit check to Statevector::new | statevector.rs | XS |
| 2 | Build adjacency set in Topology | capability.rs | M |
| 3 | Return error instead of Ok(()) for unimplemented QASM | parser.rs | S |
| 4 | Add Circuit::validate() method | circuit.rs | M |
| 5 | Clean up dead code or add feature gates | Various | S |

**Effort Key**: XS (<1hr), S (1-2hr), M (2-4hr), L (4-8hr)

---

## Appendix

### A. Files Reviewed

```
crates/arvak-ir/src/lib.rs
crates/arvak-ir/src/circuit.rs
crates/arvak-ir/src/dag.rs
crates/arvak-ir/src/gate.rs
crates/arvak-ir/src/error.rs
crates/arvak-hal/src/lib.rs
crates/arvak-hal/src/capability.rs
crates/arvak-hal/src/backend.rs
crates/arvak-compile/src/lib.rs
crates/arvak-compile/src/pass.rs
crates/arvak-compile/src/manager.rs
crates/arvak-qasm3/src/parser.rs
adapters/arvak-adapter-sim/src/statevector.rs
crates/arvak-cli/src/commands/backends.rs
```

### B. Tooling Output

```
$ cargo fmt --check
(no output - clean)

$ cargo clippy --all-features --all-targets
(no warnings)

$ cargo test --all-features
running 89 tests
test result: ok. 89 passed; 0 failed
```

### C. Dependency Highlights

| Crate | Version | Purpose | Notes |
|-------|---------|---------|-------|
| petgraph | 0.7 | DAG representation | Stable, well-maintained |
| num-complex | 0.4 | Complex numbers | Standard for quantum |
| rustc-hash | 2.1 | Fast hashing | Good choice for internal maps |
| serde | 1.0 | Serialization | Standard |
| thiserror | 2.0 | Error derives | Standard |
| tokio | 1.0 | Async runtime | Used in adapters |

### D. Council Methodology

This review was conducted using the Code Review Council framework with four analytical lenses:

- **Lens A (API/Ergonomics)**: Evaluated public API design against Rust API Guidelines
- **Lens B (Performance)**: Analyzed allocations, algorithmic complexity, and hot paths
- **Lens C (Correctness)**: Checked error handling, edge cases, and invariant maintenance
- **Lens D (Architecture)**: Assessed modularity, extensibility, and separation of concerns

All recommendations cite authoritative sources from SOURCES.md.

---

*Generated by Code Review Council — 2026-02-04*
