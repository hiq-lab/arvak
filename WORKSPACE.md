# Arvak Workspace Architecture

This document maps the monorepo structure and planned sub-project boundaries
for a future repository split.

## Dependency Tiers

```
Tier 0 (leaf)        arvak-types
                         │
Tier 1 (core IR)     arvak-ir
                       │   │
Tier 2 (transforms)  arvak-qasm3  arvak-compile
                       │       │       │
Tier 3 (runtime)     arvak-hal  arvak-sched  arvak-auto
                       │            │
Tier 4 (adapters)    sim  iqm  ibm  cudaq  qdmi
                       │
Tier 5 (services)    arvak-grpc  arvak-dashboard  arvak-bench  arvak-eval
                       │
Tier 6 (frontends)   arvak-cli  arvak-python
```

## Sub-Project Boundary Map

### `arvak-core` — compiler & IR (Rust library crate)

Crates that would move to a standalone `arvak-core` repository:

| Crate | Tier | Role |
|---|---|---|
| `arvak-types` | 0 | Shared type definitions |
| `arvak-ir` | 1 | DAG-based intermediate representation |
| `arvak-qasm3` | 2 | OpenQASM 3.0 parser/emitter |
| `arvak-compile` | 2 | Pass manager (layout, routing, translation, optimization, verification) |

These crates have **zero** runtime dependencies and form the pure-compiler core.

### `arvak-runtime` — HAL, scheduling, adapters

| Crate | Tier | Role |
|---|---|---|
| `arvak-hal` | 3 | `Backend` async trait, device properties |
| `arvak-sched` | 3 | SLURM/PBS job submission |
| `arvak-auto` | 3 | Auto-tuning over backends |
| `arvak-adapter-sim` | 4 | Local state-vector simulator |
| `arvak-adapter-iqm` | 4 | IQM REST adapter |
| `arvak-adapter-ibm` | 4 | IBM Quantum REST adapter |
| `arvak-adapter-cudaq` | 4 | CUDA-Q integration |
| `arvak-adapter-qdmi` | 4 | QDMI FFI adapter |

Depends on `arvak-core`. Adapters may become separate repos if they grow.

### `arvak-services` — gRPC, dashboard, evaluation

| Crate | Tier | Role |
|---|---|---|
| `arvak-grpc` | 5 | gRPC server (tonic) |
| `arvak-dashboard` | 5 | Web dashboard (axum) |
| `arvak-bench` | 5 | Benchmarking harness |
| `arvak-eval` | 5 | Evaluator framework |

Depends on `arvak-core` and `arvak-runtime`.

### `arvak-python` — Python bindings (maturin/PyO3)

| Crate | Tier | Role |
|---|---|---|
| `arvak-python` | 6 | PyO3 bindings to core compiler |
| `arvak-cli` | 6 | CLI frontend |

Depends on `arvak-core`. Published to PyPI as `arvak`.

### `arvak-grpc-client` — standalone Python package

Lives in `grpc-client/`. **Not** part of the Cargo workspace.

- Pure Python, depends only on `grpcio` + `protobuf`
- Orthogonal to `arvak-python` (remote gRPC client vs local compiler bindings)
- Shares zero code with the Rust crates
- Published separately to PyPI as `arvak-grpc`

## Coupling Notes

- **`arvak-types`** is the only crate depended on by everything — changes here propagate everywhere.
- **`arvak-ir` ↔ `arvak-compile`** are tightly coupled (compile operates on IR's `CircuitDag`). They should always be in the same repo.
- **`arvak-qasm3`** depends on `arvak-ir` for `Circuit`/`Gate` types but has no compile dependency. Could live in either core or its own repo.
- **Adapters** depend on `arvak-hal` traits only. They are loosely coupled and can be split individually.
- **`grpc-client/`** is fully independent — splitting it is a no-op (just move the directory).

## Current Version

All components are aligned at **v1.4.0** as of the `v1.4.0-pre-split` tag.
