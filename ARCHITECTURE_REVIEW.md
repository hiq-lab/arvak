# Arvak Architecture Review

**Date:** 2026-02-12
**Scope:** Full codebase inspection — structure, patterns, quality, dependencies, testing, security
**Version reviewed:** 1.5.1

---

## Executive Summary

Arvak is a Rust-native quantum compilation and orchestration stack targeting HPC environments. It is organized as a workspace of 20 crates (14 core + 5 adapters + 1 demo suite) following a clean compiler-pipeline architecture: IR → Compilation → HAL → Adapters.

**Overall assessment:** The codebase is professionally built with strong architectural principles, proper use of Rust's type system, and clean separation of concerns. The build passes all 553 tests with zero clippy warnings. However, there are actionable issues in test coverage, dependency freshness, hot-path performance, and module sizing that should be addressed systematically.

| Area | Rating | Notes |
|------|--------|-------|
| Architecture & Layering | Excellent | Clean compiler-pipeline, proper dependency inversion |
| Design Patterns | Excellent | Plugin, Pass Manager, Builder, Factory — all well-applied |
| Type Safety | Excellent | Newtypes, sum types, non-exhaustive errors |
| Error Handling | Excellent | Typed errors with context, `thiserror` throughout |
| Build & CI/CD | Excellent | 3 pipelines (CI, nightly, release), cargo-deny, cargo-audit |
| Code Quality | Good | Zero clippy warnings, zero TODO/FIXME, zero unwrap() in prod |
| Test Coverage | Needs Work | 553 tests pass, but 2 full crates untested, adapter gaps |
| Dependency Freshness | Needs Work | OpenTelemetry 10 versions behind, PyO3 5 versions behind |
| Hot-Path Performance | Needs Work | Unnecessary clones in compilation passes |
| Module Sizing | Needs Work | 11 files exceed 500 LOC, parser at 1285 LOC |

---

## 1. Architecture & Layering

### Dependency Graph

```
┌─────────────────────────────────────────────────────────┐
│  Application Layer                                      │
│  CLI · gRPC Server · Dashboard · Python Bindings        │
├─────────────────────────────────────────────────────────┤
│  Orchestration Layer                                    │
│  HPC Scheduler (SLURM / PBS) · Workflows                │
├─────────────────────────────────────────────────────────┤
│  Compilation & Optimization Layer                       │
│  PassManager · 20+ Passes · Type System · Auto-Uncomp   │
├─────────────────────────────────────────────────────────┤
│  Hardware Abstraction Layer                              │
│  Backend trait · Registry · Capabilities · Auth          │
├─────────────────────────────────────────────────────────┤
│  Backend Implementation Layer                           │
│  Simulator · IQM · IBM · CUDA-Q · QDMI adapters         │
├─────────────────────────────────────────────────────────┤
│  Intermediate Representation Layer                      │
│  Circuit · DAG · Gates · Parameters · Instructions       │
└─────────────────────────────────────────────────────────┘
```

**Strengths:**
- Strict downward-only dependencies — no circular references
- HAL defines interfaces; adapters implement them (dependency inversion)
- Feature-gated adapters minimize compile-time for unused backends
- Dynamic plugin loading support via `libloading`

**No issues found** in the layering structure.

---

## 2. Design Patterns

| Pattern | Location | Assessment |
|---------|----------|------------|
| Plugin/Registry | `arvak-hal` BackendRegistry | Proper trait-object dispatch, dynamic loading support |
| Pass Manager | `arvak-compile` PassManager | LLVM-inspired, analysis/transformation separation |
| Builder | Circuit API, PassManagerBuilder | Fluent APIs, consistent `.with_*()` naming |
| Factory | BackendFactory trait | Config-driven instantiation |
| Type-erased Property Store | PropertySet with `TypeId` keys | Extensible without modifying core structs |
| State Machine | JobStatus enum | Queued → Running → Completed/Failed/Cancelled |
| Newtype | QubitId, ClbitId, JobId | Prevents type confusion at compile time |

All patterns are applied consistently and idiomatically. No anti-patterns detected.

---

## 3. Code Quality Findings

### 3.1 Zero-Issue Areas

- **No TODO/FIXME/HACK/XXX comments** in the entire codebase
- **No `unwrap()` calls** in production code — all error handling uses `?` and `Result`
- **Zero clippy warnings** under `clippy::all`
- **Zero compilation errors** or warnings
- **All 553 tests pass**, 22 ignored (feature-gated/external-service tests)

### 3.2 Unsafe Code (34 blocks — all justified)

All `unsafe` blocks are confined to two FFI boundary files:
- `crates/arvak-qdmi/src/session.rs` (18 blocks) — QDMI C library calls
- `adapters/arvak-adapter-qdmi/src/backend.rs` (14 blocks) — QDMI system interface
- `crates/arvak-grpc/examples/config_example.rs` (2 blocks) — example code only

Each block has synchronization via `RwLock`, null-pointer checks, and justification comments. **No action needed**, but these should be periodically audited as the QDMI spec evolves.

### 3.3 Hardcoded `/tmp` Path (HIGH)

**File:** `crates/arvak-sched/src/scheduler.rs:74`
```rust
state_dir: PathBuf::from("/tmp/arvak-scheduler"),
```

This path is not persistent, may be cleaned by the OS, and is not configurable. On shared HPC nodes, this creates a collision risk between users.

**Recommendation:** Accept `state_dir` via `SchedulerConfig` or environment variable, falling back to `std::env::temp_dir().join("arvak-scheduler")`.

### 3.4 Unnecessary Clones in Hot Paths (MEDIUM)

Compilation passes run on every circuit. The following clones are avoidable:

| File | Lines | Clone Target | Suggested Fix |
|------|-------|-------------|---------------|
| `passes/target/translation.rs` | 42, 81, 99 | `Instruction`, gate params | Use references or `Cow<T>` |
| `passes/target/translation.rs` | 151–296 | Gate parameters in decomposition | Pre-allocate and reuse |
| `passes/agnostic/optimization.rs` | 355, 369 | `new_gates[i].clone()` in merge loop | Use indexed access or `drain()` |
| `passes/agnostic/verification.rs` | 219 | `circuit.clone().into_dag()` — full circuit copy | Accept `&CircuitDag` directly |

### 3.5 Large Files Needing Decomposition (MEDIUM)

Files exceeding 500 LOC that would benefit from splitting:

| File | LOC | Recommended Split |
|------|-----|-------------------|
| `arvak-qasm3/src/parser.rs` | 1285 | statement parsing, expression parsing, lowering |
| `arvak-compile/src/passes/agnostic/optimization.rs` | 914 | 1q optimization, 2q optimization, commutation |
| `arvak-grpc/src/server/service.rs` | 878 | job service, backend service, circuit service |
| `arvak-sched/src/scheduler.rs` | 827 | trait definitions, SLURM impl, PBS impl |
| `arvak-ir/src/dag.rs` | 827 | DAG structure, traversal, validation |
| `arvak-ir/src/circuit.rs` | 785 | builder API, internal state, conversion |
| `arvak-hal/src/auth.rs` | 731 | token management, OIDC, caching |

### 3.6 Dead Code Suppressions (LOW)

22 `#[allow(dead_code)]` instances found. All are justified:
- API response structs deserialized but not all fields consumed (adapter APIs)
- Alternative parser entry points kept for future use
- Mock-mode-only functions in QDMI adapter

No action needed, but consider removing truly unused alternative APIs in a cleanup pass.

---

## 4. Dependency Health

### 4.1 Significantly Outdated Dependencies

| Dependency | Current | Latest | Versions Behind | Risk |
|-----------|---------|--------|----------------|------|
| opentelemetry | 0.21 | 0.31 | 10 major | HIGH — API breaking changes accumulate |
| opentelemetry-otlp | 0.14 | 0.31 | 17 minor | HIGH — coupled with above |
| pyo3 | 0.23 | 0.28 | 5 major | HIGH — blocks musllinux/aarch64 wheels |
| rand | 0.8 | 0.10 | 2 major | LOW — API stable, sim-only usage |
| rusqlite | 0.31 | 0.38 | 7 minor | MEDIUM — SQLite bundled, security patches |
| tonic/prost | 0.12/0.13 | 0.14/0.14 | 2/1 minor | MEDIUM — gRPC ecosystem |
| petgraph | 0.7 | 0.8 | 1 major | LOW — DAG representation, well-abstracted |
| reqwest | 0.12 | 0.13 | 1 minor | LOW |

### 4.2 Version Inconsistencies

The dashboard and gRPC server use different major versions of the same libraries:

| Library | gRPC | Dashboard |
|---------|------|-----------|
| axum | 0.7 | 0.8 |
| tower | 0.4 | 0.5 |
| tower-http | 0.5 | 0.6 |

These are documented and allowlisted in `deny.toml`. Should be unified when the gRPC crate is upgraded.

### 4.3 Security Posture

- `cargo-audit`: No known vulnerabilities
- `cargo-deny`: License compliance enforced (MIT/Apache-2.0, no GPL/AGPL)
- TLS: rustls over OpenSSL (pure Rust, auditable)
- Supply chain: All crates from crates.io registry only

---

## 5. Test Coverage Analysis

### 5.1 Coverage by Crate

| Crate | Unit Tests | Integration Tests | Assessment |
|-------|-----------|-------------------|------------|
| arvak-sched | 76 | 1 file | Excellent |
| arvak-eval | 55 | — | Strong |
| arvak-ir | 40 | — | Strong |
| arvak-compile | 36 | 1 file (10 tests) | Strong |
| arvak-grpc | 36 | 1 file (15+ tests) | Adequate |
| arvak-hal | 24 | — | Adequate |
| arvak-types | 19 | — | Good |
| arvak-auto | 17 | — | Good |
| arvak-qasm3 | 16 | — | Adequate |
| arvak-bench | 15 | — | Good |
| arvak-qdmi | 12 | 2 files | Good |
| **arvak-cli** | **0** | **0** | **CRITICAL GAP** |
| **arvak-dashboard** | **0** | **0** | **CRITICAL GAP** |
| arvak-python | 2 | Python-side only | Severe gap |

### 5.2 Adapter Coverage

| Adapter | Tests | Gaps |
|---------|-------|------|
| arvak-adapter-sim | 9 | Adequate for simulator |
| arvak-adapter-cudaq | 9 | No error path tests |
| arvak-adapter-ibm | 5 | No auth tests, no job lifecycle |
| arvak-adapter-iqm | 4 | No auth tests, no job lifecycle |
| arvak-adapter-qdmi | 5 | No FFI edge case tests |

### 5.3 Critical Gaps

1. **arvak-cli (13 source files, 0 tests):** Command parsing, argument validation, error display — entirely untested
2. **arvak-dashboard (16 source files, 0 tests):** API endpoints, WebSocket, state management — entirely untested
3. **Error path testing:** Only 37 error assertions across 457 unit tests (8%). Most error enum variants are never exercised in tests
4. **Adapter auth/network failures:** No tests for authentication failures, timeouts, or network errors in any cloud adapter
5. **Property-based testing:** `proptest` is in dev-dependencies but appears underutilized

### 5.4 Missing Test Infrastructure

- No shared test fixtures or circuit builders (each test constructs circuits manually)
- No mock backend implementation for testing CLI/gRPC without a real backend
- No reusable test data generators

---

## 6. Multi-Step Action Plan

### Phase 1: Critical Fixes (immediate)

**1.1 Fix hardcoded `/tmp` path in scheduler**
- File: `crates/arvak-sched/src/scheduler.rs:74`
- Make `state_dir` configurable via `SchedulerConfig`
- Fall back to `std::env::temp_dir().join("arvak-scheduler")`
- Estimated scope: 1 file, ~20 lines changed

**1.2 Eliminate full circuit clone in verification pass**
- File: `crates/arvak-compile/src/passes/agnostic/verification.rs:219`
- `circuit.clone().into_dag()` copies the entire circuit for read-only analysis
- Refactor to accept `&CircuitDag` or `&Circuit` directly
- Estimated scope: 1-2 files, ~15 lines changed

### Phase 2: Test Coverage (high priority)

**2.1 Create shared test infrastructure**
- Add `tests/common/` module with reusable circuit builders (Bell, GHZ, QFT fixtures)
- Create a `MockBackend` implementation of the `Backend` trait for CLI/gRPC testing
- Create test helpers for common assertion patterns
- Estimated scope: 2-3 new files

**2.2 Add CLI tests**
- Test command parsing and argument validation for all 10 commands
- Test error display and exit codes
- Test with `MockBackend` to avoid external dependencies
- Target: 30+ tests across 13 source files

**2.3 Add dashboard API tests**
- Test all REST endpoints with mock state
- Test WebSocket connection lifecycle
- Test error responses for malformed requests
- Target: 40+ tests across 16 source files

**2.4 Add adapter error path tests**
- Test authentication failures for IBM and IQM adapters
- Test network timeout handling
- Test malformed API response handling
- Test job status polling edge cases
- Target: 20+ tests across 5 adapters

**2.5 Expand error path coverage**
- Systematically test every `Error` enum variant in each crate
- Target: increase error assertions from 37 to 150+

### Phase 3: Dependency Upgrades (medium priority)

**3.1 Upgrade OpenTelemetry stack (0.21 → 0.31)**
- This is the largest version gap and affects observability
- Expect API breaking changes; plan for 0.21 → 0.25 → 0.31 staged upgrade
- Scope: `arvak-grpc`, `arvak-eval`, `arvak-dashboard`

**3.2 Upgrade PyO3 (0.23 → 0.28)**
- Unblocks musllinux and aarch64 wheel builds (currently commented out in release.yml)
- Broadens Python version support
- Scope: `arvak-python`, release CI pipeline

**3.3 Unify axum/tower versions across gRPC and dashboard**
- Upgrade `arvak-grpc` from axum 0.7 → 0.8, tower 0.4 → 0.5
- Remove `skip` entries from `deny.toml`
- Scope: `arvak-grpc`, `arvak-dashboard`

**3.4 Upgrade remaining outdated dependencies**
- `rusqlite` 0.31 → 0.38 (security patches)
- `tonic`/`prost` 0.12/0.13 → 0.14 (gRPC improvements)
- `rand` 0.8 → 0.10 (simulator only, low risk)

### Phase 4: Code Structure Improvements (lower priority)

**4.1 Split `parser.rs` (1285 LOC)**
- Extract statement parsing into `parser/statement.rs`
- Extract expression parsing into `parser/expression.rs`
- Extract AST-to-Circuit lowering into `parser/lowering.rs`
- Keep `parser/mod.rs` as the public facade

**4.2 Split `optimization.rs` (914 LOC)**
- Extract `Optimize1qGates` into `passes/optimize_1q.rs`
- Extract `CancelCX` / `CommutativeCancellation` into `passes/cancel.rs`
- Keep shared utilities (matrix math, epsilon) in `passes/opt_utils.rs`

**4.3 Split `service.rs` (878 LOC)**
- Extract job-related RPCs into `server/job_service.rs`
- Extract backend-related RPCs into `server/backend_service.rs`
- Extract circuit operations into `server/circuit_service.rs`

**4.4 Reduce clones in compilation hot path**
- Replace `Instruction::clone()` with references in translation pass
- Use `Cow<[ParameterExpression]>` for gate parameters
- Replace `circuit.clone().into_dag()` with borrow-based analysis
- Benchmark before/after with criterion

### Phase 5: Continuous Improvement

**5.1 Enable property-based testing**
- `proptest` is already in dev-dependencies but underutilized
- Add property tests for: circuit→QASM3→circuit roundtrip, DAG invariants, parameter binding commutativity

**5.2 Add `protoc` to documented prerequisites**
- `protobuf-compiler` is required by `arvak-grpc` build script but not documented in setup instructions
- Add to `make setup-tooling` and README prerequisites

**5.3 Track test coverage metrics**
- Add `cargo-llvm-cov` or `cargo-tarpaulin` to nightly CI
- Set minimum coverage thresholds per crate
- Report coverage trends over time

---

## Appendix: File Inventory

- **Rust source files:** 162
- **Test files:** 80+ with tests, 40+ without
- **Total workspace dependencies:** 474 packages
- **Direct dependencies:** ~192
- **Lines of documentation:** 12 docs files, 5 Jupyter notebooks, 7 QASM examples
- **CI/CD workflows:** 3 (ci.yml, nightly.yml, release.yml)
