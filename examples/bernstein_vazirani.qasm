// Bernstein-Vazirani Algorithm
// Finds hidden bit string s in one query
// Oracle computes f(x) = s·x (mod 2)
// Hidden string: s = 101 (5 in decimal)

OPENQASM 3.0;

qubit[4] q;  // q[0-2]: input register, q[3]: ancilla
bit[3] c;

// Initialize ancilla to |1⟩
x q[3];

// Apply Hadamard to all qubits
h q[0];
h q[1];
h q[2];
h q[3];

// Oracle for s = 101
// f(x) = x[0] XOR x[2]
cx q[0], q[3];
cx q[2], q[3];

// Apply Hadamard to input register
h q[0];
h q[1];
h q[2];

// Measure input register to reveal s
c[0] = measure q[0];
c[1] = measure q[1];
c[2] = measure q[2];

// Result should be 101 with 100% probability
