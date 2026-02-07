# Arvak Architecture

## Overview

Arvak is a Rust-native quantum compilation and orchestration stack designed for HPC environments. This document describes the system architecture, components, and design decisions.

## Design Principles

1. **Stateless Core** — All state externalized to storage
2. **Fail-safe Routing** — Jobs retry on transient failures, fallback backends
3. **Plugin Isolation** — Adapters run in subprocess or container for fault isolation
4. **Async-first** — Non-blocking I/O throughout
5. **HPC-native** — First-class support for on-premise QPUs (IQM at LUMI/LRZ)

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           User Experience                                │
│                                                                         │
│   from arvak import QuantumFloat, QuantumCircuit                          │
│   # Familiar Pythonic API, but backed by Rust                           │
│                                                                         │
└────────────────────────────────┬────────────────────────────────────────┘
                                 │
┌────────────────────────────────▼────────────────────────────────────────┐
│                      arvak-python (Thin Python Layer)                      │
│                                                                         │
│   - Pythonic wrappers around Rust types                                 │
│   - Compatibility shims for Qiskit circuit import                       │
│   - Compatibility shims for Qrisp session import                        │
│                                                                         │
└────────────────────────────────┬────────────────────────────────────────┘
                                 │ PyO3
┌────────────────────────────────▼────────────────────────────────────────┐
│                         arvak-core (Rust)                                  │
│                                                                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │
│  │   arvak-ir    │  │ arvak-compile │  │  arvak-auto   │  │  arvak-hal    │    │
│  │             │  │             │  │             │  │             │    │
│  │ Circuit DAG │  │ Pass Mgr    │  │ Uncompute   │  │ Backend     │    │
│  │ QASM3 Parse │  │ Layout      │  │ Memory Mgmt │  │ Abstraction │    │
│  │ Gate Lib    │  │ Routing     │  │ (Qrisp-like)│  │ IBM/IQM     │    │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘    │
│                                                                         │
│  ┌─────────────┐  ┌─────────────┐                                       │
│  │ arvak-sched   │  │ arvak-types   │                                       │
│  │             │  │             │                                       │
│  │ Slurm/PBS   │  │ QFloat      │                                       │
│  │ Integration │  │ QBool, etc  │                                       │
│  └─────────────┘  └─────────────┘                                       │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Crate Structure

```
arvak/
├── Cargo.toml                          # Workspace definition
├── crates/
│   ├── arvak-ir/                         # Circuit intermediate representation
│   ├── arvak-qasm3/                      # OpenQASM 3 parser/emitter
│   ├── arvak-compile/                    # Transpilation passes
│   ├── arvak-auto/                       # Automatic uncomputation
│   ├── arvak-types/                      # High-level quantum types
│   ├── arvak-hal/                        # Hardware abstraction layer
│   ├── arvak-sched/                      # HPC scheduler integration
│   ├── arvak-core/                       # Unified re-exports
│   ├── arvak-cli/                        # Command-line interface
│   └── arvak-python/                     # Python bindings (PyO3)
│
├── adapters/                           # Backend adapter implementations
│   ├── arvak-adapter-iqm/
│   ├── arvak-adapter-ibm/
│   └── arvak-adapter-sim/
│
├── examples/
├── benches/
├── tests/
└── docs/
```

## Component Responsibilities

### arvak-ir (Circuit Intermediate Representation)

The core circuit representation, providing:

- **Circuit** — User-facing API for building circuits
- **CircuitDag** — DAG-based IR for compilation passes
- **Gate/Instruction** — Gate definitions and operations
- **Qubit/Clbit** — Quantum and classical bit types
- **Parameter** — Symbolic parameter expressions

### arvak-qasm3 (OpenQASM 3 Parser)

OpenQASM 3 support:

- Lexer (using `logos`)
- Parser (using `chumsky`)
- AST representation
- Conversion to/from Circuit
- Subset focus: gates, qubits, measurements (no pulse/timing initially)

### arvak-compile (Compilation Framework)

Transpilation infrastructure:

- **Pass** — Trait for compilation passes
- **PassManager** — Orchestrates pass execution
- **PropertySet** — Inter-pass communication
- **Built-in Passes:**
  - Layout (TrivialLayout)
  - Routing (BasicRouting, NeutralAtomRouting)
  - BasisTranslation
  - Optimization (Optimize1qGates, CancelCX, CommutativeCancellation)
  - Verification (MeasurementBarrierVerification)

### arvak-auto (Automatic Uncomputation)

Qrisp-inspired features:

- Automatic qubit deallocation
- Uncomputation synthesis
- Memory management via permeability analysis
- Qfree gate detection

### arvak-types (High-Level Types)

High-level quantum types:

- **QuantumFloat** — Arbitrary precision quantum numbers
- **QuantumBool** — Boolean quantum variables
- **QuantumArray** — Arrays of quantum variables

### arvak-hal (Hardware Abstraction Layer)

Backend abstraction:

- **Backend** — Trait for quantum backends
- **Capabilities** — Device capability description
- **Job/JobStatus** — Job lifecycle management
- **ExecutionResult** — Measurement results

### arvak-sched (HPC Scheduler Integration)

HPC scheduler support:

- **Scheduler** — Trait for scheduler adapters
- **SlurmAdapter** — Slurm integration
- **PbsAdapter** — PBS Pro integration
- Job script generation
- Status monitoring

### arvak-cli (Command-Line Interface)

User-facing CLI:

- `arvak compile` — Compile circuits
- `arvak submit` — Submit to backends
- `arvak status` — Check job status
- `arvak result` — Retrieve results
- `arvak backends` — List backends

### arvak-python (Python Bindings)

PyO3-based Python interface:

- Pythonic wrappers around Rust types
- Qiskit circuit import/export
- Qrisp session import
- NumPy integration for results

## Data Flow

### Compilation Flow

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  OpenQASM3  │────▶│   Circuit   │────▶│ CircuitDag  │
│   (text)    │     │  (builder)  │     │   (IR)      │
└─────────────┘     └─────────────┘     └──────┬──────┘
                                               │
                    ┌──────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────┐
│                    PassManager                           │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐    │
│  │ Layout  │─▶│ Routing │─▶│ Basis   │─▶│Optimize │    │
│  └─────────┘  └─────────┘  └─────────┘  └─────────┘    │
└─────────────────────────────┬───────────────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │ Compiled Circuit│
                    └─────────────────┘
```

### Execution Flow

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Circuit   │────▶│   Backend   │────▶│    Job      │
│  (compiled) │     │   Adapter   │     │   (queued)  │
└─────────────┘     └─────────────┘     └──────┬──────┘
                                               │
                    ┌──────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────┐
│                 HPC Scheduler (Slurm)                    │
│  ┌─────────────────────────────────────────────────┐    │
│  │  sbatch script:                                 │    │
│  │  #!/bin/bash                                    │    │
│  │  #SBATCH --partition=quantum                    │    │
│  │  arvak-runner --job-id=$ARVAK_JOB_ID               │    │
│  └─────────────────────────────────────────────────┘    │
└─────────────────────────────┬───────────────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │ ExecutionResult │
                    │    (counts)     │
                    └─────────────────┘
```

## Technology Choices

### Why Rust?

1. **Performance** — Zero-cost abstractions, no GC overhead
2. **Memory Safety** — Critical for long-running HPC jobs
3. **Single Binary** — Easy deployment on HPC clusters
4. **LLVM Backend** — Natural path to QIR integration
5. **Community** — Active, enthusiastic developer community
6. **Qiskit Validation** — Qiskit already uses Rust internally

### Key Dependencies

| Dependency | Purpose |
|------------|---------|
| `petgraph` | Graph algorithms for circuit DAG |
| `num-complex` | Complex number support |
| `logos` | Fast lexer generation |
| `chumsky` | Parser combinators |
| `tokio` | Async runtime |
| `reqwest` | HTTP client for backend APIs |
| `pyo3` | Python bindings |
| `serde` | Serialization |
| `tracing` | Structured logging |
| `clap` | CLI argument parsing |

## Comparison with Existing Tools

### vs Qiskit

| Aspect | Arvak | Qiskit |
|--------|-----|--------|
| Language | Rust (with Python bindings) | Python (with Rust internals) |
| Focus | HPC integration, compilation | Full quantum stack |
| Scope | Compilation + orchestration | Algorithms + compilation + runtime |
| HPC Support | First-class | Limited |
| Binary | Single static binary | Python environment |

### vs Qrisp

| Aspect | Arvak | Qrisp |
|--------|-----|-------|
| Language | Rust | Python |
| Abstraction | Mid-level (IR focus) | High-level (variables) |
| Uncomputation | Planned (simplified) | Full automatic |
| Backend | Multiple (IQM, IBM) | Via Qiskit |

### vs XACC

| Aspect | Arvak | XACC |
|--------|-----|------|
| Language | Rust | C++ |
| Focus | Compilation + HPC | HPC acceleration |
| Maturity | New | Established |
| Community | Rust ecosystem | HPC/academic |

## Security Considerations

- API tokens stored in environment variables, not config files
- OIDC support for HPC center authentication
- No credential persistence in adapter memory
- Plugin sandboxing via subprocess isolation

## Future Directions

1. **QIR Integration** — Native QIR support via LLVM
2. **Distributed Quantum** — Multi-QPU coordination
3. **Pulse-Level Control** — Quil-T style pulse access
4. **Error Mitigation** — Compilation-time error mitigation passes
5. **Circuit Cutting** — Large circuit partitioning
