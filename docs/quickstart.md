# Quick Start

Arvak is a Rust-native quantum compilation platform with Python bindings,
a CLI, and adapters for eleven hardware backends.

> Every Python snippet on this page is executed by CI
> (`crates/arvak-python/tests/test_doc_snippets.py`). If you can read it,
> it runs against the current code.

## Install

```bash
# Python API (recommended)
pip install arvak

# With framework integrations
pip install arvak[qiskit]      # also: arvak[qrisp], arvak[cirq], arvak[pennylane]
```

Building the CLI from source requires the `hal-contract` sibling checkout
(it is a workspace path dependency):

```bash
git clone https://github.com/hiq-lab/arvak
git clone https://github.com/hiq-lab/hal-contract
cd arvak && ln -s ../hal-contract .hal-contract
cargo install --path crates/arvak-cli
```

## First circuit (Python)

```python
import arvak

# Build a Bell state
circuit = arvak.Circuit("bell", num_qubits=2)
circuit.h(0).cx(0, 1).measure_all()

# Run on the built-in statevector simulator (no network, no backend setup)
counts = arvak.run_sim(circuit, shots=1000)
print(counts)  # e.g. {'00': 498, '11': 502}

# Compile for IQM hardware (native prx/cz gate set)
compiled = arvak.compile(
    circuit,
    basis_gates=arvak.BasisGates.iqm(),
    optimization_level=2,
)
print(compiled.depth())
```

See [python-api.md](python-api.md) for the full API surface.

## First circuit (CLI)

Create `bell.qasm`:

```qasm
OPENQASM 3.0;
include "stdgates.inc";
qubit[2] q;
bit[2] c;
h q[0];
cx q[0], q[1];
c[0] = measure q[0];
c[1] = measure q[1];
```

```bash
arvak run -i bell.qasm --shots 1024          # run on the local simulator
arvak backends                               # list available backends
arvak compile -i bell.qasm --target iqm \
    --optimization-level 2 -o compiled.qasm  # compile for hardware
```

See [cli.md](cli.md) for all commands and flags.

## First circuit (Rust)

```rust
use arvak_ir::Circuit;
use arvak_hal::Backend;
use arvak_adapter_sim::SimulatorBackend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let circuit = Circuit::bell()?;

    let backend = SimulatorBackend::new();
    let job_id = backend.submit(&circuit, 1000, None).await?;
    let result = backend.wait(&job_id).await?;

    println!("Results: {:?}", result.counts);
    Ok(())
}
```

## Submitting to real hardware

Hardware access needs vendor credentials (environment variables, see the
adapter READMEs under [`adapters/`](../adapters)). The workflow is the same
for every vendor:

```python
# doc-test: skip  (needs vendor credentials)
import arvak

backend = arvak.backend_for("iqm_garnet")   # or "ibm_marrakesh", "sim", ...
result = backend.run(qasm_string, shots=1024)
print(result.counts)
```

Via the CLI, including HPC batch schedulers:

```bash
arvak auth login --provider csc --project project_462000xxx
arvak submit -i circuit.qasm --backend iqm \
    --scheduler slurm --partition q_fiqci \
    --account project_462000xxx --time "00:30:00" --wait
```

See [hpc-deployment.md](hpc-deployment.md) for LUMI/LRZ specifics.

## Next steps

- [Python API reference](python-api.md)
- [CLI reference](cli.md)
- [Compilation pipeline](compilation.md) — passes, optimization levels
- [Architecture](architecture.md) — crate layout, HAL design
