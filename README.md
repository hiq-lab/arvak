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
| Simulator (`hiq-adapter-sim`) | ✅ Complete | Statevector simulation |
| IQM Adapter (`hiq-adapter-iqm`) | ✅ Complete | Resonance API integration |
| IBM Adapter (`hiq-adapter-ibm`) | ✅ Complete | Qiskit Runtime API |
| HPC Scheduler (`hiq-sched`) | 🚧 Planned | Slurm/PBS integration |
| Python Bindings | 🚧 Planned | PyO3 bindings |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Python SDK Layer                                      │
│              (Qiskit, Qrisp, user scripts)                              │
└──────────────────────────┬──────────────────────────────────────────────┘
                           │ PyO3 bindings (planned)
┌──────────────────────────▼──────────────────────────────────────────────┐
│                    hiq-core (Rust)                                       │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌───────────┐         │
│  │  hiq-ir    │  │ hiq-compile│  │  hiq-hal   │  │ hiq-sched │         │
│  │            │  │            │  │            │  │           │         │
│  │ Circuit IR │  │ Pass mgr   │  │ Backend    │  │ Slurm/PBS │         │
│  │ QASM3 parse│  │ Optimizer  │  │ abstraction│  │ interface │         │
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
│   └── hiq-cli/         # Command-line interface
├── adapters/
│   ├── hiq-adapter-sim/ # Local statevector simulator
│   ├── hiq-adapter-iqm/ # IQM Resonance API adapter
│   └── hiq-adapter-ibm/ # IBM Quantum API adapter
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
| IQM LUMI | 🚧 | OIDC | On-premise |
| IQM LRZ | 🚧 | OIDC | On-premise |

## Compilation Targets

| Target | Basis Gates | Topology |
|--------|-------------|----------|
| `iqm`, `iqm5` | PRX, CZ | Star (5 qubits) |
| `iqm20` | PRX, CZ | Star (20 qubits) |
| `ibm`, `ibm5` | RZ, SX, X, CX | Linear (5 qubits) |
| `ibm27` | RZ, SX, X, CX | Linear (27 qubits) |
| `simulator` | Universal | Full connectivity |

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
- [ ] Python bindings

### Phase 3: HPC Integration (Next)
- [ ] Slurm adapter
- [ ] PBS adapter
- [ ] Large circuit handling
- [ ] LUMI deployment testing

### Phase 4: Production
- [ ] Advanced optimization passes
- [ ] Qrisp-like features
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
