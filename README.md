# HIQ: Rust-Native Quantum Compilation Stack

**HPC-Integrated Quantum Orchestration Platform**

HIQ is a Rust-native quantum compilation and orchestration stack designed for HPC environments. It provides fast compilation, first-class HPC scheduler integration, and unified access to quantum backends including IQM and IBM Quantum.

## Vision

HIQ is **not** a Qiskit replacement. It's a complementary tool that:

1. **Doesn't compete** with Qiskit/Cirq/Qrisp at the algorithm level
2. **Provides** a Rust-native compilation core for performance-critical paths
3. **Prioritizes** HPC integration (Slurm, PBS) as first-class citizens
4. **Targets** European HPC quantum installations (LUMI, LRZ)
5. **Offers** Python bindings for ecosystem compatibility

## Current Status

| Component | Status | Notes |
|-----------|--------|-------|
| Circuit IR (`hiq-ir`) | ✅ Complete | DAG-based representation |
| QASM3 Parser (`hiq-qasm3`) | ✅ Complete | Parse & emit |
| Compilation (`hiq-compile`) | ✅ Complete | Pass manager, layout, routing, optimization |
| HAL (`hiq-hal`) | ✅ Complete | Backend trait, capabilities, job management |
| CLI (`hiq-cli`) | ✅ Complete | compile, run, backends commands |
| Quantum Types (`hiq-types`) | ✅ Complete | QuantumInt, QuantumFloat, QuantumArray |
| Auto-Uncompute (`hiq-auto`) | ✅ Complete | Automatic ancilla uncomputation |
| Simulator (`hiq-adapter-sim`) | ✅ Complete | Statevector simulation |
| IQM Adapter (`hiq-adapter-iqm`) | ✅ Complete | Resonance API integration |
| IBM Adapter (`hiq-adapter-ibm`) | ✅ Complete | Qiskit Runtime API |
| HPC Scheduler (`hiq-sched`) | ✅ Complete | SLURM & PBS integration, workflows, persistence |
| Python Bindings (`hiq-python`) | ✅ Complete | PyO3 bindings for circuits & compilation |
| Demos | ✅ Complete | Grover, VQE, QAOA examples |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Python SDK Layer                                      │
│              (Qiskit, Qrisp, user scripts)                              │
└──────────────────────────┬──────────────────────────────────────────────┘
                           │ PyO3 bindings
┌──────────────────────────▼──────────────────────────────────────────────┐
│                    hiq-python (PyO3)                                     │
│              Circuit building, compilation, QASM export                  │
└──────────────────────────┬──────────────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────────────┐
│                    hiq-core (Rust)                                       │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌───────────┐         │
│  │  hiq-ir    │  │ hiq-compile│  │  hiq-hal   │  │ hiq-sched │         │
│  │            │  │            │  │            │  │           │         │
│  │ Circuit IR │  │ Pass mgr   │  │ Backend    │  │ SLURM/PBS │         │
│  │ QASM3 parse│  │ Optimizer  │  │ abstraction│  │ Workflows │         │
│  └────────────┘  └────────────┘  └────────────┘  └───────────┘         │
└──────────────────────────┬──────────────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────────────┐
│                    hiq-cli (Rust)                                        │
│                 Single static binary                                     │
└─────────────────────────────────────────────────────────────────────────┘
```

## Project Structure

```
HIQ/
├── crates/
│   ├── hiq-ir/          # Circuit intermediate representation
│   ├── hiq-qasm3/       # OpenQASM 3.0 parser and emitter
│   ├── hiq-compile/     # Compilation pass manager
│   ├── hiq-hal/         # Hardware abstraction layer
│   ├── hiq-cli/         # Command-line interface
│   ├── hiq-python/      # Python bindings (PyO3)
│   ├── hiq-sched/       # HPC job scheduler (SLURM, PBS, workflows)
│   ├── hiq-types/       # Qrisp-like quantum types (QuantumInt, QuantumFloat)
│   └── hiq-auto/        # Automatic uncomputation
├── adapters/
│   ├── hiq-adapter-sim/ # Local statevector simulator
│   ├── hiq-adapter-iqm/ # IQM Resonance API adapter
│   └── hiq-adapter-ibm/ # IBM Quantum API adapter
├── demos/               # Demo applications (Grover, VQE, QAOA)
└── examples/            # Example QASM circuits
```

## Quick Start

### Building

```bash
# Build all crates
cargo build

# Build with IQM backend support
cargo build --features iqm

# Build with IBM backend support
cargo build --features ibm

# Build with all backends
cargo build --features all-backends

# Build release version
cargo build --release

# Install CLI
cargo install --path crates/hiq-cli
```

### CLI Usage

```bash
# Show help
hiq --help

# List available backends
hiq backends

# Run a circuit on the simulator
hiq run examples/bell.qasm --backend sim --shots 1000

# Compile a circuit for IQM hardware
hiq compile examples/bell.qasm --target iqm --output bell_compiled.qasm

# Run on IQM hardware (requires IQM_TOKEN)
export IQM_TOKEN="your-api-token"
hiq run examples/bell.qasm --backend iqm --shots 1000

# Run on IBM Quantum (requires IBM_QUANTUM_TOKEN)
export IBM_QUANTUM_TOKEN="your-api-token"
hiq run examples/bell.qasm --backend ibm --shots 1000
```

### Example Circuits

**Bell State** (`examples/bell.qasm`):
```qasm
OPENQASM 3.0;
qubit[2] q;
bit[2] c;
h q[0];
cx q[0], q[1];
c = measure q;
```

**GHZ State** (`examples/ghz.qasm`):
```qasm
OPENQASM 3.0;
qubit[5] q;
bit[5] c;
h q[0];
cx q[0], q[1];
cx q[1], q[2];
cx q[2], q[3];
cx q[3], q[4];
c = measure q;
```

### Rust API

```rust
use hiq_ir::Circuit;
use hiq_qasm3::{parse, emit};
use hiq_adapter_sim::SimulatorBackend;
use hiq_hal::Backend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse QASM3
    let source = r#"
        OPENQASM 3.0;
        qubit[2] q;
        bit[2] c;
        h q[0];
        cx q[0], q[1];
        c = measure q;
    "#;

    let circuit = parse(source)?;
    println!("Parsed: {} qubits, depth {}", circuit.num_qubits(), circuit.depth());

    // Run on simulator
    let backend = SimulatorBackend::new();
    let job_id = backend.submit(&circuit, 1000).await?;
    let result = backend.wait(&job_id).await?;

    println!("Results: {:?}", result.counts);

    // Emit back to QASM3
    let qasm_out = emit(&circuit)?;
    println!("{}", qasm_out);

    Ok(())
}
```

### Compilation Example

```rust
use hiq_ir::Circuit;
use hiq_compile::{PassManagerBuilder, CouplingMap, BasisGates};

fn main() -> anyhow::Result<()> {
    // Create circuit
    let circuit = Circuit::bell()?;

    // Setup compilation for IQM target
    let (pm, mut props) = PassManagerBuilder::new()
        .with_optimization_level(2)
        .with_target(CouplingMap::star(5), BasisGates::iqm())
        .build();

    // Compile
    let mut dag = circuit.into_dag();
    pm.run(&mut dag, &mut props)?;

    let compiled = Circuit::from_dag(dag);
    println!("Compiled: depth {}", compiled.depth());

    Ok(())
}
```

### Python API

```bash
# Install from source
cd crates/hiq-python
pip install maturin
maturin develop
```

```python
from hiq import Circuit, compile_circuit, to_qasm3

# Build a Bell state circuit
circuit = Circuit(2)
circuit.h(0)
circuit.cx(0, 1)
circuit.measure_all()

print(f"Qubits: {circuit.num_qubits()}, Depth: {circuit.depth()}")

# Compile for IQM hardware
compiled = compile_circuit(circuit, target="iqm", optimization_level=2)

# Export to QASM3
qasm = to_qasm3(compiled)
print(qasm)
```

### Quantum Types (Qrisp-inspired)

```rust
use hiq_types::{QuantumInt, QuantumFloat, QuantumArray};
use hiq_ir::Circuit;

fn main() -> anyhow::Result<()> {
    let mut circuit = Circuit::new("arithmetic");

    // Create quantum integers
    let a = QuantumInt::<4>::new(&mut circuit);  // 4-bit integer [0, 15]
    let b = QuantumInt::<4>::new(&mut circuit);

    // Initialize values
    a.initialize(5, &mut circuit)?;  // a = |5⟩
    b.initialize(3, &mut circuit)?;  // b = |3⟩

    // Create quantum floats (sign + mantissa + exponent)
    let x = QuantumFloat::<4, 3>::new(&mut circuit);  // 4-bit mantissa, 3-bit exponent

    // Create quantum arrays
    let arr = QuantumArray::<4, 8>::new(&mut circuit);  // 4 elements, 8 qubits each

    Ok(())
}
```

### Automatic Uncomputation

```rust
use hiq_auto::{UncomputeContext, uncompute};
use hiq_ir::Circuit;

fn main() -> anyhow::Result<()> {
    let mut circuit = Circuit::new("with_uncompute");

    // Mark the start of computation (tracks ops from this point)
    let ctx = UncomputeContext::begin(&circuit)
        .with_label("ancilla_block");

    // Perform operations on ancilla qubits
    circuit.h(QubitId(0))?;
    circuit.cx(QubitId(0), QubitId(1))?;

    // Automatically uncompute - appends inverse operations
    uncompute(&mut circuit, ctx)?;

    // Circuit now has: H, CX, CX†, H† (ancillas back to |0⟩)
    Ok(())
}
```

### Demo Applications

The `demos/` directory contains example quantum algorithms:

```bash
# Run all demos
cargo run --bin demo_all

# Run specific algorithms
cargo run --bin demo_grover   # Grover's search algorithm
cargo run --bin demo_vqe      # Variational Quantum Eigensolver
cargo run --bin demo_qaoa     # Quantum Approximate Optimization
```

## Supported Gates

### Single-Qubit Gates
| Gate | Description | Syntax |
|------|-------------|--------|
| Identity | No operation | `id q[0];` |
| Pauli-X | Bit flip | `x q[0];` |
| Pauli-Y | Y rotation | `y q[0];` |
| Pauli-Z | Phase flip | `z q[0];` |
| Hadamard | Superposition | `h q[0];` |
| S/Sdg | π/2 phase | `s q[0];` |
| T/Tdg | π/4 phase | `t q[0];` |
| SX/SXdg | √X gate | `sx q[0];` |
| RX | X rotation | `rx(θ) q[0];` |
| RY | Y rotation | `ry(θ) q[0];` |
| RZ | Z rotation | `rz(θ) q[0];` |
| Phase | Phase gate | `p(θ) q[0];` |
| U | Universal | `u(θ,φ,λ) q[0];` |
| PRX | Phased RX (IQM) | `prx(θ,φ) q[0];` |

### Two-Qubit Gates
| Gate | Description | Syntax |
|------|-------------|--------|
| CNOT | Controlled-X | `cx q[0], q[1];` |
| CY | Controlled-Y | `cy q[0], q[1];` |
| CZ | Controlled-Z | `cz q[0], q[1];` |
| SWAP | Qubit swap | `swap q[0], q[1];` |
| iSWAP | Imaginary swap | `iswap q[0], q[1];` |
| CRZ | Controlled-RZ | `crz(θ) q[0], q[1];` |
| CP | Controlled-phase | `cp(θ) q[0], q[1];` |

### Three-Qubit Gates
| Gate | Description | Syntax |
|------|-------------|--------|
| Toffoli | CCX | `ccx q[0], q[1], q[2];` |
| Fredkin | CSWAP | `cswap q[0], q[1], q[2];` |

## Backend Support

| Backend | Status | Auth | Notes |
|---------|--------|------|-------|
| Simulator | ✅ | None | Local statevector, up to ~20 qubits |
| IQM Resonance | ✅ | `IQM_TOKEN` | Cloud API |
| IBM Quantum | ✅ | `IBM_QUANTUM_TOKEN` | Cloud API (Qiskit Runtime) |
| IQM LUMI | ✅ | OIDC | On-premise (CSC Finland) |
| IQM LRZ | ✅ | OIDC | On-premise (Germany) |

## Compilation Targets

| Target | Basis Gates | Topology |
|--------|-------------|----------|
| `iqm`, `iqm5` | PRX, CZ | Star (5 qubits) |
| `iqm20` | PRX, CZ | Star (20 qubits) |
| `ibm`, `ibm5` | RZ, SX, X, CX | Linear (5 qubits) |
| `ibm27` | RZ, SX, X, CX | Linear (27 qubits) |
| `simulator` | Universal | Full connectivity |

## HPC Deployment

HIQ provides first-class support for HPC environments with both SLURM and PBS schedulers.

### LUMI (CSC, Finland)

```yaml
# ~/.hiq/config.yaml
site: lumi
scheduler:
  type: slurm
  partition: q_fiqci
  account: project_462000xxx

backend:
  type: iqm
  endpoint: https://qpu.lumi.csc.fi
  auth_method: oidc
```

```bash
# Authenticate via OIDC
hiq auth login --provider csc

# Submit job to LUMI
hiq run circuit.qasm --backend iqm --shots 1000
```

### PBS-Based HPC Sites

```yaml
# ~/.hiq/config.yaml
scheduler:
  type: pbs
  queue: quantum
  account: your-project

backend:
  type: iqm
  endpoint: https://your-qpu.example.com
```

### Scheduler Support

| Scheduler | Commands | Features |
|-----------|----------|----------|
| SLURM | sbatch, squeue, sacct, scancel | QOS mapping, array jobs |
| PBS/Torque | qsub, qstat, qdel, qhold, qrls | Array jobs, job holds |

## Testing

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p hiq-ir
cargo test -p hiq-qasm3
cargo test -p hiq-compile

# Run with verbose output
cargo test -- --nocapture
```

## Roadmap

### Phase 1: Foundation ✅
- [x] Circuit IR and DAG
- [x] QASM3 parser (core subset)
- [x] Basic CLI
- [x] IQM adapter
- [x] Local simulator

### Phase 2: Compilation ✅
- [x] Pass manager
- [x] Layout and routing passes
- [x] Basis translation
- [x] IBM adapter
- [x] Python bindings (PyO3)

### Phase 3: HPC Integration ✅
- [x] SLURM adapter
- [x] PBS adapter (Torque/PBS Pro)
- [x] Workflow orchestration
- [x] Job persistence (JSON/SQLite)
- [x] Demo applications (VQE, QAOA, Grover)
- [x] OIDC authentication for LUMI/LRZ
- [x] LUMI integration tests

### Phase 4: Production (In Progress)
- [x] Advanced optimization passes (1q optimization, CX cancellation, commutative cancellation)
- [x] Qrisp-like quantum types (QuantumInt, QuantumFloat, QuantumArray)
- [x] Automatic uncomputation framework (gate inversion, context management)
- [ ] Full documentation
- [ ] 1.0 release

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Acknowledgments

HIQ builds on ideas from:

- [Qiskit](https://qiskit.org/) — Circuit representation and transpiler architecture
- [Qrisp](https://qrisp.eu/) — High-level abstractions and automatic uncomputation
- [XACC](https://github.com/eclipse-xacc/xacc) — HPC integration patterns

## Contact

- GitHub Issues: [hiq-project/hiq](https://github.com/hiq-project/hiq/issues)
