# Arvak Python Bindings

Python bindings for the Arvak quantum compilation platform.

## Installation

```bash
pip install arvak
```

## Quick Start

```python
import arvak

# Create a Bell state circuit
qc = arvak.Circuit("bell", num_qubits=2)
qc.h(0).cx(0, 1).measure_all()

# Check circuit properties
print(f"Depth: {qc.depth()}")
print(f"Qubits: {qc.num_qubits}")

# Convert to QASM
qasm = arvak.to_qasm(qc)
print(qasm)

# Parse QASM
qc2 = arvak.from_qasm("""
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
""")
```

## Features

- **Circuit Building**: Fluent API for building quantum circuits
- **Standard Gates**: H, X, Y, Z, S, T, CX, CZ, and many more
- **IQM Native Gates**: PRX gate support
- **QASM3 I/O**: Parse and emit OpenQASM 3.0
- **Compilation Types**: Layout, CouplingMap, BasisGates for compilation
- **Hamiltonian Simulation** (`arvak.sim`): Trotter-Suzuki and QDrift time-evolution synthesis
- **Variational Solvers** (`arvak.optimize`): VQE, QAOA, PCE QUBO solver, spectral partition
- **Noise Threading**: `NoisyBackend` wraps any backend with a Qiskit noise model

## Pre-built Circuits

```python
# Bell state
bell = arvak.Circuit.bell()

# GHZ state
ghz = arvak.Circuit.ghz(5)

# Quantum Fourier Transform
qft = arvak.Circuit.qft(4)
```

## Hamiltonian Simulation

```python
from arvak.sim import PauliOp, HamiltonianTerm, Hamiltonian, TrotterEvolution

h = Hamiltonian.from_terms([
    HamiltonianTerm.zz(0, 1, -1.0),
    HamiltonianTerm.x(0, -0.5),
])
circuit = TrotterEvolution.new(h, t=1.0, n_steps=4).first_order()
counts = arvak.run_sim(circuit, shots=1024)
```

## Variational Algorithms

```python
from arvak.optimize import VQESolver, SparsePauliOp, QAOASolver, BinaryQubo

# VQE
h = SparsePauliOp([(-1.0, {0: "Z", 1: "Z"}), (-0.5, {0: "X", 1: "X"})])
result = VQESolver(h, n_qubits=2, n_layers=2, seed=0).solve()
print(result.energy)

# QAOA
qubo = BinaryQubo.from_dict(n=4, quadratic={(0,1):-1,(1,2):-1,(2,3):-1,(0,3):-1})
result = QAOASolver(qubo, p=2, seed=0).solve()
print(result.solution, result.cost)
```

## Install Extras

```bash
pip install arvak[optimize]         # VQE, QAOA, PCE (numpy + scipy)
pip install arvak[optimize-sklearn] # + scikit-learn k-means
pip install arvak[qiskit]           # Qiskit backend integration
pip install arvak[all]              # Everything
```

## License

Apache-2.0
