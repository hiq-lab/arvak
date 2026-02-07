# Arvak Example Circuits

This directory contains example OpenQASM 3.0 circuits for testing and demonstration.

## Basic Examples

### bell.qasm
Creates a Bell state (maximally entangled 2-qubit state):
```
|00⟩ → (|00⟩ + |11⟩)/√2
```

### ghz.qasm
Creates a 5-qubit GHZ (Greenberger-Horne-Zeilinger) state:
```
|00000⟩ → (|00000⟩ + |11111⟩)/√2
```

### variational.qasm
A simple variational circuit with parameterized rotations, useful for VQE-style algorithms.

## Quantum Algorithms

### grover_2qubit.qasm
Grover's search algorithm for 2 qubits, searching for |11⟩.

### qft_4qubit.qasm
4-qubit Quantum Fourier Transform.

### teleportation.qasm
Quantum teleportation protocol using 3 qubits.

## Running Examples

### Using the CLI

```bash
# Run on simulator
arvak run examples/bell.qasm --backend sim --shots 1000

# Compile for IQM
arvak compile examples/bell.qasm --target iqm --output bell_iqm.qasm

# Run on IQM (requires IQM_TOKEN)
arvak run examples/bell.qasm --backend iqm --shots 1000
```

### Using Rust

```rust
use arvak_qasm3::parse;
use arvak_adapter_sim::SimulatorBackend;
use arvak_hal::Backend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let qasm = std::fs::read_to_string("examples/bell.qasm")?;
    let circuit = parse(&qasm)?;

    let backend = SimulatorBackend::new();
    let job_id = backend.submit(&circuit, 1000).await?;
    let result = backend.wait(&job_id).await?;

    println!("{:?}", result.counts);
    Ok(())
}
```

### Using Python

```python
from arvak import parse_qasm, SimulatorBackend

circuit = parse_qasm(open("examples/bell.qasm").read())
backend = SimulatorBackend()
result = backend.run(circuit, shots=1000)
print(result.counts)
```
