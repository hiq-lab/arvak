# When Compilation Speed Matters

Arvak's compiler achieves sub-millisecond compilation times on small-to-medium circuits. This document identifies the applications where that speed is a qualitative advantage — specifically, workloads where the compiler sits inside a hot loop and compiles thousands to millions of circuit variants.

## Benchmark Reference

Measured on x86_64 Debian (arvak.io server):

| Circuit | O0 | O0 gates/s | O2 | O2 gates/s |
|---|---|---|---|---|
| BB84+PCCM (3q) | 35µs | 602K | 88µs | 228K |
| BBM92+PCCM (4q) | 38µs | 717K | 103µs | 253K |
| QEC [[4,2,2]] (6q) | 315µs | 953K | 3.4ms | 85K |

## Applications

### 1. Variational Quantum Eigensolver (VQE)

VQE minimizes the expectation value `<ψ(θ)|H|ψ(θ)>` by classical optimization over ansatz parameters θ. Each optimizer step requires recompiling the ansatz circuit. Furthermore, each Pauli term in the Hamiltonian decomposition needs a separate measurement circuit with different basis rotations. Total circuits = Pauli terms × optimizer iterations.

**Molecular Hamiltonians:**

| System | Pauli terms | VQE iterations | Circuits compiled |
|---|---|---|---|
| H₂ (minimal basis) | 15 | ~100 | ~1,500 |
| LiH (STO-3G) | 631 | ~500 | ~315,000 |
| H₂O (STO-3G) | 1,086 | ~1,000 | ~1,086,000 |
| BeH₂ (STO-3G) | 666 | ~800 | ~530,000 |
| N₂ (cc-pVDZ) | ~10⁴ | ~2,000 | ~20,000,000 |

At Arvak O2 speed (~100µs per small circuit), compiling 1M circuits for H₂O takes roughly 100 seconds. A typical Python-based transpiler at 50–200ms per circuit would take 14–55 hours for the same workload.

This is the single largest use case for fast compilation. Molecular electronic structure Hamiltonians combine many Pauli terms with many optimizer iterations, producing circuit counts in the millions.

### 2. QAOA for Combinatorial Optimization

QAOA maps combinatorial optimization problems to Ising Hamiltonians and sweeps over variational angle parameters (γ, β). Hyperparameter searches — grid sweeps, Bayesian optimization, or multi-start local optimization — require thousands of recompilations at different angle values.

| Problem | Qubits | QAOA depth p | Angle variations | Total circuits |
|---|---|---|---|---|
| MaxCut (20 nodes) | 20 | 1–5 | 100–1,000 | 500–5,000 |
| Portfolio optimization (30 assets) | 30 | 3–8 | 500+ | 4,000+ |
| Job scheduling (50 jobs) | 50+ | 5–12 | 1,000+ | 12,000+ |

QAOA circuits grow linearly with problem graph edges. At 50 qubits and moderate depth, circuits reach 200–500 gates. Circuit counts are lower than VQE but still benefit from sub-millisecond compilation for rapid prototyping.

### 3. Hamiltonian Simulation (Trotterization)

Simulating time evolution `e^{-iHt}` via Trotter-Suzuki decomposition generates many circuit variants when scanning over:

- **Time steps** — convergence studies varying dt and total simulation time
- **Trotter orders** — accuracy vs circuit depth tradeoffs (1st, 2nd, 4th order)
- **Hamiltonian parameter sweeps** — mapping phase diagrams across coupling constants

**Relevant Hamiltonians:**

| Model | Domain | Why many circuits |
|---|---|---|
| Hubbard model | Condensed matter, superconductivity | Sweep U/t ratio across phase diagram |
| Heisenberg chain | Spin dynamics | Time evolution at many dt values |
| Fermi-Hubbard | Material science | Vary lattice geometry and coupling |
| Schwinger model | Lattice gauge theory | Sweep mass and coupling parameters |

A phase diagram scan over a 50×50 parameter grid produces 2,500 distinct Trotter circuits. These circuits are typically large (hundreds of gates at moderate Trotter depth), so the per-circuit compilation time is higher, but the total count is manageable.

### 4. Quantum Error Correction — Real-Time Decoding

QEC syndrome extraction cycles require circuit generation at the decoder's clock rate. For surface codes on superconducting hardware, syndrome extraction happens approximately every 1µs and the decoder must respond within that window. Adaptive circuit generation based on syndrome history places the compiler in a real-time feedback loop.

Arvak's 35µs at O0 is in the right order of magnitude for contributing to a real-time QEC pipeline, though the decoder logic itself remains the primary bottleneck.

### 5. Quantum Machine Learning

Parameterized quantum circuits used as machine learning models require gradient computation via the parameter-shift rule: 2 circuit evaluations per parameter per gradient step. A 20-parameter model trained for 1,000 steps produces 40,000 circuits. Batch training over datasets and hyperparameter sweeps multiply this further.

| Model parameters | Training steps | Circuits (parameter-shift) |
|---|---|---|
| 10 | 500 | 10,000 |
| 20 | 1,000 | 40,000 |
| 50 | 2,000 | 200,000 |

## Summary

| Scenario | Circuits compiled | Arvak O2 | Typical transpiler (100ms) |
|---|---|---|---|
| VQE on LiH | ~315K | ~30s | ~9 hours |
| VQE on H₂O | ~1M | ~100s | ~28 hours |
| QAOA grid search | ~5K | ~10s | ~8 min |
| QML training | ~40K | ~4s | ~67 min |
| Phase diagram scan | ~2.5K | ~5s | ~4 min |

The strongest case for sub-millisecond compilation is **VQE on molecular Hamiltonians**, where circuit counts reach 10⁵–10⁶ and the difference between microsecond and millisecond compilation is the difference between minutes and days. QAOA and QML workloads also benefit, though their lower circuit counts mean slower compilers can still complete in reasonable time. QEC real-time decoding represents a future frontier where nanosecond-scale compilation may ultimately be required.

## Live Demos

Two runnable demos ship with Arvak that demonstrate these compilation speeds on realistic algorithm loops. Each generates thousands of circuits, compiles them through the full pass pipeline (layout, routing, basis translation, optimization), and reports per-circuit timing.

### VQE Compilation Throughput (`demo-speed-vqe`)

Simulates a 500-iteration VQE optimization loop for LiH (4 qubits, 15 Hamiltonian terms). Compiles 7,500 circuits at O0 and O2 targeting the IQM native gate set.

```
cargo run -p arvak-demos --release --bin demo-speed-vqe
```

Typical output (release mode, Apple M-series):

| Level | Total | Per-Circuit | Speedup vs 100ms baseline |
|---|---|---|---|
| O0 | 0.12s | 16µs | 6,358x |
| O2 | 0.56s | 74µs | 1,347x |

### QML Training Loop (`demo-speed-qml`)

Simulates parameter-shift gradient training of a 4-qubit, 3-layer quantum classifier over 1,000 training steps. With 12 trainable parameters, each step requires 25 circuit evaluations, totaling 25,000 circuits.

```
cargo run -p arvak-demos --release --bin demo-speed-qml
```

Both demos are also available inside the Docker image:

```
docker exec arvak-dashboard demo-speed-vqe
docker exec arvak-dashboard demo-speed-qml
```
