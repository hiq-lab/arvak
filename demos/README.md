# Arvak Demo Suite

Demonstration suite showcasing Arvak's HPC-quantum orchestration capabilities.

## Overview

This crate provides end-to-end demonstrations of quantum algorithms that highlight the value of hybrid HPC-quantum computing:

| Demo | Algorithm | Qubits | Purpose |
|------|-----------|--------|---------|
| `demo-grover` | Grover's Search | 2-8 | Baseline quantum speedup |
| `demo-vqe` | VQE for H2/LiH | 2-4 | Molecular ground state energy |
| `demo-qaoa` | QAOA Max-Cut | 4-10 | Combinatorial optimization |
| `demo-multi` | Multi-job orchestration | Mixed | Arvak scheduling showcase |
| `demo-all` | All demos | Mixed | Complete demonstration |

## Quick Start

```bash
# Run Grover's search
cargo run -p arvak-demos --bin demo-grover -- --qubits 4 --marked 7

# Run VQE for H2 molecule
cargo run -p arvak-demos --bin demo-vqe -- --molecule h2 --iterations 50

# Run QAOA for Max-Cut
cargo run -p arvak-demos --bin demo-qaoa -- --graph square --layers 2

# Run multi-job orchestration demo
cargo run -p arvak-demos --bin demo-multi

# Run all demos
cargo run -p arvak-demos --bin demo-all
```

## Demos

### Grover's Search

Demonstrates quantum search with quadratic speedup over classical algorithms.

```bash
cargo run -p arvak-demos --bin demo-grover -- --qubits 4 --marked 7
```

Options:
- `--qubits, -n`: Number of qubits (default: 4)
- `--marked, -m`: Marked state to search for (default: 7)
- `--shots, -s`: Number of measurement shots (default: 1024)

### VQE Molecular Simulation

Variational Quantum Eigensolver for finding molecular ground state energies - a key application for drug discovery and materials science.

```bash
cargo run -p arvak-demos --bin demo-vqe -- --molecule h2 --iterations 50
```

Options:
- `--molecule, -m`: Molecule to simulate: `h2` or `lih` (default: h2)
- `--reps, -r`: Number of ansatz repetitions (default: 2)
- `--iterations, -i`: Maximum optimization iterations (default: 50)
- `--shots, -s`: Shots per energy evaluation (default: 1024)

Expected results:
- H2: Ground state energy ~ -1.169 Hartree
- LiH: Ground state energy ~ -7.882 Hartree

### QAOA for Max-Cut

Quantum Approximate Optimization Algorithm for graph partitioning - relevant to logistics, network design, and defense applications.

```bash
cargo run -p arvak-demos --bin demo-qaoa -- --graph petersen --layers 2
```

Options:
- `--graph, -g`: Graph type: `square`, `petersen`, `random` (default: square)
- `--layers, -p`: QAOA circuit depth (default: 2)
- `--iterations, -i`: Maximum optimization iterations (default: 50)
- `--shots, -s`: Shots per evaluation (default: 1024)

### Multi-Job Orchestration

Demonstrates Arvak's ability to manage multiple concurrent quantum workloads - the core value proposition for HPC-quantum integration.

```bash
cargo run -p arvak-demos --bin demo-multi
```

This demo submits multiple jobs (Grover, VQE, QAOA, batch circuits) and shows how Arvak orchestrates them across quantum resources.

## Library Usage

The demos crate can also be used as a library:

```rust
use arvak_demos::circuits::grover::grover_circuit;
use arvak_demos::problems::{h2_hamiltonian, Graph};
use arvak_demos::runners::{VqeRunner, QaoaRunner, ScheduledRunner};

// Create VQE runner
let hamiltonian = h2_hamiltonian();
let runner = VqeRunner::new(hamiltonian)
    .with_reps(2)
    .with_maxiter(50);
let result = runner.run();
println!("Ground state energy: {:.4} Ha", result.optimal_energy);

// Create QAOA runner
let graph = Graph::square_4();
let runner = QaoaRunner::new(graph)
    .with_layers(2)
    .with_maxiter(50);
let result = runner.run();
println!("Best cut: {}", result.best_cut);
```

### Scheduler Integration

For HPC cluster deployment, use the `ScheduledRunner`:

```rust
use std::sync::Arc;
use arvak_demos::runners::ScheduledRunner;
use arvak_sched::{HpcScheduler, SchedulerConfig, Priority};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = SchedulerConfig::default();
    let scheduler = Arc::new(HpcScheduler::new(config, backends, store).await?);
    let runner = ScheduledRunner::new(scheduler);

    // Submit jobs to HPC cluster
    let job_id = runner.submit_grover(4, 7, Priority::high()).await?;
    let result = runner.wait(&job_id).await?;

    // Submit a complete demo workflow
    let workflow_id = runner.submit_demo_workflow().await?;
    runner.wait_workflow(&workflow_id).await?;

    Ok(())
}
```

## Module Structure

```
demos/
├── src/
│   ├── lib.rs              # Library entry point
│   ├── circuits/           # Quantum circuit generators
│   │   ├── grover.rs       # Grover oracle and diffusion
│   │   ├── vqe.rs          # VQE ansatz (TwoLocal)
│   │   └── qaoa.rs         # QAOA cost and mixer layers
│   ├── problems/           # Problem definitions
│   │   ├── hamiltonian.rs  # Pauli Hamiltonian representation
│   │   ├── molecules.rs    # Molecular Hamiltonians (H2, LiH)
│   │   └── maxcut.rs       # Graph Max-Cut problems
│   ├── optimizers/         # Classical optimizers
│   │   └── cobyla.rs       # COBYLA and SPSA
│   └── runners/            # Algorithm runners
│       ├── vqe.rs          # VQE optimization loop
│       ├── qaoa.rs         # QAOA optimization loop
│       ├── orchestrator.rs # Multi-job demo
│       └── scheduled.rs    # Arvak scheduler integration
└── bin/
    ├── demo_grover.rs
    ├── demo_vqe.rs
    ├── demo_qaoa.rs
    ├── demo_multi.rs
    └── demo_all.rs
```

## Testing

```bash
# Run all tests
cargo test -p arvak-demos

# Run specific test
cargo test -p arvak-demos test_vqe_energy_bounds

# Run with output
cargo test -p arvak-demos -- --nocapture
```

## Demo Script (15 min presentation)

1. **Introduction (2 min)**: Run Grover's search to warm up
2. **VQE Deep Dive (5 min)**: Demonstrate molecular simulation
3. **QAOA (3 min)**: Show optimization capabilities
4. **Orchestration (5 min)**: Multi-job demo highlighting Arvak's value

## Dependencies

- `arvak-ir`: Quantum circuit intermediate representation
- `arvak-qasm3`: OpenQASM 3.0 code generation
- `arvak-hal`: Hardware abstraction layer
- `arvak-sched`: HPC scheduler with SLURM integration
