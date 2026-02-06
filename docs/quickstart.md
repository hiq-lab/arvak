# Arvak Quick Start Guide

## Installation

### From Pre-built Binary (Recommended)

```bash
# Download latest release
curl -LO https://github.com/hiq-project/hiq/releases/latest/download/hiq-linux-x86_64.tar.gz

# Extract
tar xzf hiq-linux-x86_64.tar.gz

# Move to PATH
sudo mv hiq hiq-runner /usr/local/bin/

# Verify installation
hiq --version
```

### From Source

```bash
# Prerequisites: Rust 1.83+
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/hiq-project/hiq
cd hiq
cargo build --release

# Install
cargo install --path crates/hiq-cli
```

### Python Bindings

```bash
pip install hiq-python
```

## Your First Circuit

### Using the CLI

Create a file `bell.qasm`:

```qasm
OPENQASM 3.0;
qubit[2] q;
bit[2] c;

h q[0];
cx q[0], q[1];

c = measure q;
```

Compile and run:

```bash
# Compile for IQM
hiq compile -i bell.qasm -o bell_compiled.qasm --target iqm

# Run on simulator
hiq run -i bell_compiled.qasm --backend simulator --shots 1000

# Output:
# Results (1000 shots):
#   00: 498 (49.80%)
#   11: 502 (50.20%)
```

### Using the Rust API

```rust
use hiq_ir::Circuit;
use hiq_compile::{PassManagerBuilder, PropertySet, CouplingMap, BasisGates};
use hiq_hal::Backend;
use hiq_adapter_sim::SimulatorBackend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create Bell state circuit
    let circuit = Circuit::bell()?;
    println!("Created circuit with {} qubits", circuit.num_qubits());

    // Setup compilation for IQM
    let properties = PropertySet::new()
        .with_target(
            CouplingMap::star(5),
            BasisGates::iqm(),
        );

    let (pm, mut props) = PassManagerBuilder::new()
        .with_optimization_level(2)
        .with_properties(properties)
        .build();

    // Compile
    let mut dag = circuit.clone().into_dag();
    pm.run(&mut dag, &mut props)?;
    let compiled = Circuit::from_dag(dag);
    println!("Compiled circuit depth: {}", compiled.depth());

    // Run on simulator
    let backend = SimulatorBackend::new();
    let job_id = backend.submit(&compiled, 1000).await?;
    let result = backend.wait(&job_id).await?;

    // Print results
    println!("\nResults ({} shots):", result.shots);
    for (bitstring, count) in result.counts.iter() {
        let prob = *count as f64 / result.shots as f64;
        println!("  {}: {} ({:.2}%)", bitstring, count, prob * 100.0);
    }

    Ok(())
}
```

### Using the Python API

```python
from hiq import Circuit, compile_for_backend, SimulatorBackend

# Create Bell state circuit
circuit = Circuit.bell()
print(f"Created circuit with {circuit.num_qubits()} qubits")

# Compile for IQM
compiled = compile_for_backend(circuit, "iqm", optimization_level=2)
print(f"Compiled circuit depth: {compiled.depth()}")

# Run on simulator
backend = SimulatorBackend()
result = backend.run(compiled, shots=1000)

# Print results
print(f"\nResults ({result.shots} shots):")
for bitstring, count in result.counts.items():
    prob = count / result.shots * 100
    print(f"  {bitstring}: {count} ({prob:.2f}%)")
```

## Common Operations

### Create Custom Circuits

```rust
use hiq_ir::{Circuit, QubitId};
use std::f64::consts::PI;

let mut circuit = Circuit::new("my_circuit");

// Add qubits and classical bits
let q = circuit.add_qreg("q", 3);
let c = circuit.add_creg("c", 3);

// Apply gates
circuit
    .h(q[0])?
    .cx(q[0], q[1])?
    .cx(q[1], q[2])?
    .rz(PI / 4.0, q[2])?
    .measure(q[0], c[0])?
    .measure(q[1], c[1])?
    .measure(q[2], c[2])?;
```

### Compile for Different Backends

```rust
// For IQM (PRX + CZ basis)
let iqm_props = PropertySet::new()
    .with_target(CouplingMap::star(5), BasisGates::iqm());

// For IBM (RZ + SX + X + CX basis)
let ibm_props = PropertySet::new()
    .with_target(CouplingMap::linear(5), BasisGates::ibm());
```

### Submit to Real Hardware

```bash
# Set credentials
export IQM_TOKEN="your-token-here"

# Submit to IQM Resonance
hiq submit -i circuit.qasm \
    --backend iqm \
    --shots 1024 \
    --wait

# Check status
hiq status <job-id> --backend iqm

# Get results
hiq result <job-id> --backend iqm --format json
```

### HPC Scheduler Integration

```bash
# Submit via Slurm
hiq submit -i circuit.qasm \
    --backend iqm \
    --shots 1024 \
    --scheduler slurm \
    --partition quantum \
    --account my-project \
    --time 00:30:00
```

## Configuration

### Config File

Create `~/.hiq/config.yaml`:

```yaml
# Default backend
default_backend: simulator

# Backend configurations
backends:
  simulator:
    type: simulator

  iqm:
    type: iqm
    endpoint: https://cocos.resonance.meetiqm.com
    token: ${IQM_TOKEN}

  ibm:
    type: ibm
    token: ${IBM_QUANTUM_TOKEN}

# Scheduler configuration
scheduler:
  type: slurm
  partition: quantum
  account: my-project

# Compilation defaults
compilation:
  optimization_level: 2
  target: iqm
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `IQM_TOKEN` | IQM Resonance API token |
| `IBM_QUANTUM_TOKEN` | IBM Quantum API token |
| `HIQ_CONFIG` | Custom config file path |
| `HIQ_LOG` | Log level (error, warn, info, debug, trace) |

## Next Steps

- [Architecture Overview](architecture.md) — Understand the system design
- [IR Specification](ir-specification.md) — Learn the circuit representation
- [Compilation Guide](compilation.md) — Explore transpilation passes
- [HPC Deployment](hpc-deployment.md) — Deploy on HPC clusters
- [Contributing](contributing.md) — Join the development

## Getting Help

```bash
# CLI help
hiq --help
hiq compile --help
hiq submit --help

# Verbose output for debugging
hiq -vvv submit -i circuit.qasm --backend iqm
```

For issues and questions:
- GitHub Issues: https://github.com/hiq-project/hiq/issues
- Documentation: https://hiq-project.github.io/hiq/
