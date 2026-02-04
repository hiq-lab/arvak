OPENQASM 3.0;

// Bell state circuit
// Creates maximally entangled state: (|00⟩ + |11⟩) / √2

qubit[2] q;
bit[2] c;

// Create superposition
h q[0];

// Entangle
cx q[0], q[1];

// Measure
c = measure q;
