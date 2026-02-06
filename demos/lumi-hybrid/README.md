# LUMI-Q Hybrid VQE Demo

**Quantum-Classical Hybrid Workflow on LUMI Supercomputer**

This demo showcases Arvak's ability to orchestrate quantum-classical hybrid workloads on LUMI, Europe's most powerful supercomputer. It implements the Variational Quantum Eigensolver (VQE) algorithm to compute the ground state energy of the H₂ molecule.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           LUMI Supercomputer                                 │
│                                                                              │
│  ┌─────────────────────────────┐    ┌─────────────────────────────────────┐ │
│  │         LUMI-G              │    │            LUMI-Q                   │ │
│  │    (AMD MI250X GPUs)        │    │      (IQM 20-qubit QPU)             │ │
│  │                             │    │                                     │ │
│  │  ┌───────────────────────┐  │    │  ┌───────────────────────────────┐ │ │
│  │  │  Classical Optimizer  │  │    │  │   Quantum Circuit Executor    │ │ │
│  │  │  ─────────────────────│  │    │  │   ───────────────────────────│ │ │
│  │  │  • COBYLA/SPSA        │◄─┼────┼──┤   • UCCSD Ansatz             │ │ │
│  │  │  • Parameter updates  │──┼────┼──►   • Expectation values       │ │ │
│  │  │  • Result analysis    │  │    │  │   • Shots: 1000+             │ │ │
│  │  └───────────────────────┘  │    │  └───────────────────────────────┘ │ │
│  │                             │    │                                     │ │
│  │  Partition: small-g         │    │  Partition: q_fiqci                 │ │
│  └─────────────────────────────┘    └─────────────────────────────────────┘ │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                          Arvak Orchestration                               ││
│  │  • SLURM job coordination    • Circuit compilation    • Result parsing  ││
│  └─────────────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────────────┘
```

## The Problem: H₂ Ground State Energy

Finding the ground state energy of molecules is a fundamental problem in quantum chemistry. For the H₂ molecule:

- **Classical approach**: Exact diagonalization is possible but scales exponentially
- **Quantum approach**: VQE uses a parameterized quantum circuit to prepare trial states

The VQE algorithm:
1. Prepare a parameterized trial state |ψ(θ)⟩
2. Measure energy expectation value ⟨ψ(θ)|H|ψ(θ)⟩
3. Update parameters θ using classical optimizer
4. Repeat until convergence

## Prerequisites

### LUMI Account Setup

1. **Apply for LUMI access** via your national allocation body
2. **Request quantum partition access** (q_fiqci for IQM quantum computer)
3. **Set up authentication**:
   ```bash
   # On LUMI, set your Helmi token
   export HELMI_TOKEN="your-token-from-myaccessid"
   ```

### Build Arvak on LUMI

```bash
# Clone Arvak
git clone https://github.com/hiq-lab/arvak.git
cd arvak

# Load modules
module load LUMI/23.09
module load cray-python/3.10.10
module load Rust/1.75.0

# Build
cargo build --release

# Set Arvak directory
export HIQ_DIR=$(pwd)
```

## Quick Start

### Option 1: Full Workflow (Recommended)

```bash
cd demos/lumi-hybrid/slurm

# Run the complete VQE workflow
./vqe_workflow.sh --bond-distance 0.735 --max-iterations 50 --shots 1000
```

This will:
1. Submit a classical optimizer job to LUMI-G
2. The optimizer will spawn quantum jobs to LUMI-Q as needed
3. Collect results and generate convergence plots

### Option 2: Simulator Mode (No Quantum Access)

```bash
# Run with local simulator
./vqe_workflow.sh --bond-distance 0.735

# Or directly:
cargo run --release --bin lumi_vqe -- \
    --backend sim \
    --bond-distance 0.735 \
    --max-iterations 50 \
    --shots 1000 \
    --output results
```

### Option 3: Bond Distance Scan

```bash
# Scan from 0.3 to 2.5 Å to find equilibrium geometry
cargo run --release --bin lumi_vqe -- \
    --backend sim \
    --scan \
    --output results
```

## Project Structure

```
demos/lumi-hybrid/
├── Cargo.toml              # Rust package manifest
├── README.md               # This file
├── src/
│   ├── main.rs             # VQE coordinator
│   ├── ansatz.rs           # UCCSD ansatz circuit
│   ├── hamiltonian.rs      # H2 Hamiltonian (Jordan-Wigner)
│   ├── optimizer.rs        # Classical optimizers (COBYLA)
│   └── quantum_worker.rs   # Quantum job executor
├── slurm/
│   ├── quantum_job.sh      # LUMI-Q job script
│   ├── classical_job.sh    # LUMI-G job script
│   └── vqe_workflow.sh     # Workflow orchestrator
├── scripts/
│   └── plot_results.py     # Visualization script
└── results/                # Output directory
```

## SLURM Configuration

### Quantum Job (LUMI-Q)

```bash
#SBATCH --partition=q_fiqci      # IQM quantum partition
#SBATCH --account=project_xxx    # Your LUMI project
#SBATCH --time=00:30:00          # 30 minutes max
#SBATCH --nodes=1
```

### Classical Job (LUMI-G)

```bash
#SBATCH --partition=small-g      # GPU partition
#SBATCH --account=project_xxx
#SBATCH --time=01:00:00
#SBATCH --gpus-per-node=1        # Request 1 MI250X GPU
```

## Expected Results

### Single Point Calculation (r = 0.735 Å)

```
═══════════════════════════════════════════════════════════════
VQE Optimization Complete
═══════════════════════════════════════════════════════════════
Final energy:    -1.136189 Ha
Exact energy:    -1.137270 Ha
Error:           1.0810 mHa (chemical accuracy: ~1.6 mHa)
Optimal θ:       0.1103 rad
Total shots:     50000
═══════════════════════════════════════════════════════════════
```

### Bond Distance Scan

The potential energy surface shows the equilibrium bond length at ~0.74 Å:

```
Bond Distance Scan Summary
┌──────────┬───────────────┬───────────────┬───────────────┐
│ r (Å)    │ VQE (Ha)      │ Exact (Ha)    │ Error (mHa)   │
├──────────┼───────────────┼───────────────┼───────────────┤
│   0.500  │    -1.055234  │    -1.057362  │       2.1281  │
│   0.700  │    -1.134567  │    -1.136189  │       1.6223  │
│   0.735  │    -1.135921  │    -1.137270  │       1.3489  │  ← Equilibrium
│   0.800  │    -1.132145  │    -1.133892  │       1.7467  │
│   1.000  │    -1.101234  │    -1.102987  │       1.7529  │
│   1.500  │    -1.028765  │    -1.029812  │       1.0470  │
│   2.000  │    -0.987234  │    -0.988012  │       0.7781  │
└──────────┴───────────────┴───────────────┴───────────────┘
```

## Algorithm Details

### UCCSD Ansatz

The minimal UCCSD ansatz for H₂ uses 2 qubits and 1 variational parameter:

```
|0⟩ ─[X]─[Ry(π/2)]─●─[Rz(θ)]─●─[Ry(-π/2)]─[M]─
                   │         │
|0⟩ ─[X]───────────X─────────X────────────[M]─
```

This circuit:
1. Prepares the Hartree-Fock state |01⟩
2. Applies the UCCSD excitation exp(θ(a†b - b†a))
3. Measures in the computational basis

### Hamiltonian

The H₂ Hamiltonian in Jordan-Wigner encoding:

```
H = g₀ I + g₁ Z₀ + g₂ Z₁ + g₃ Z₀Z₁ + g₄ X₀X₁ + g₅ Y₀Y₁
```

Where coefficients g_i depend on the bond distance (pre-computed from PySCF).

## Performance Considerations

### Quantum Hardware (LUMI-Q)

- **Qubit count**: 20 qubits (IQM)
- **Native gates**: PRX, CZ
- **Connectivity**: Star topology
- **Typical fidelity**: ~99% single-qubit, ~95% two-qubit

### Classical Compute (LUMI-G)

- **GPU**: AMD MI250X (128 GB HBM2e)
- **Use case**: Complex optimization, post-processing
- **Note**: For simple VQE, CPU is often sufficient

## Troubleshooting

### "No quantum token found"

```bash
# Set your Helmi token from MyAccessID
export HELMI_TOKEN="your-token"

# Or use simulator mode
./vqe_workflow.sh  # Will auto-detect and use simulator
```

### "Partition not available"

Check your LUMI project allocations:
```bash
saldo -b  # Check billing units
groups    # Check group membership
```

### "Circuit compilation failed"

Ensure Arvak is built correctly:
```bash
cd $HIQ_DIR
cargo build --release
cargo test -p arvak-adapter-iqm
```

## Further Reading

- [LUMI Documentation](https://docs.lumi-supercomputer.eu/)
- [IQM Quantum Computer](https://www.meetiqm.com/)
- [VQE Algorithm Paper](https://arxiv.org/abs/1304.3061)
- [Arvak Documentation](https://github.com/hiq-lab/arvak)

## License

Apache-2.0 — See [LICENSE](../../LICENSE) for details.
