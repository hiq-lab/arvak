OPENQASM 3.0;

// GHZ state circuit (5 qubits)
// Creates state: (|00000⟩ + |11111⟩) / √2

qubit[5] q;
bit[5] c;

// Create superposition on first qubit
h q[0];

// CNOT chain to entangle all qubits
cx q[0], q[1];
cx q[1], q[2];
cx q[2], q[3];
cx q[3], q[4];

// Measure all
c = measure q;
