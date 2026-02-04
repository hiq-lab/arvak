OPENQASM 3.0;

// Variational Quantum Eigensolver (VQE) ansatz
// Simple two-qubit example with parameterized rotations

qubit[2] q;
bit[2] c;

// Layer 1: Single-qubit rotations
ry(pi/4) q[0];
ry(pi/3) q[1];

// Entangling layer
cx q[0], q[1];

// Layer 2: Single-qubit rotations
rz(pi/6) q[0];
rz(pi/8) q[1];

// Measure
c = measure q;
