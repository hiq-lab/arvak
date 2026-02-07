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

## Pre-built Circuits

```python
# Bell state
bell = arvak.Circuit.bell()

# GHZ state
ghz = arvak.Circuit.ghz(5)

# Quantum Fourier Transform
qft = arvak.Circuit.qft(4)
```

## License

Apache-2.0
