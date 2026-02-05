// Grover's Algorithm for 2 qubits
// Searches for the marked state |11⟩
// One iteration is optimal for n=2

OPENQASM 3.0;

qubit[2] q;
bit[2] c;

// Initialize superposition
h q[0];
h q[1];

// Oracle: marks |11⟩ with a phase flip
// Implements Z ⊗ Z controlled phase
cz q[0], q[1];

// Diffusion operator (inversion about mean)
h q[0];
h q[1];
x q[0];
x q[1];
cz q[0], q[1];
x q[0];
x q[1];
h q[0];
h q[1];

// Measurement
c = measure q;
