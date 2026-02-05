// 4-Qubit Quantum Fourier Transform
// Transforms computational basis to frequency basis

OPENQASM 3.0;

qubit[4] q;
bit[4] c;

// Prepare an input state (|5⟩ = |0101⟩)
x q[0];
x q[2];

// QFT on qubit 0
h q[0];
cp(pi/2) q[1], q[0];
cp(pi/4) q[2], q[0];
cp(pi/8) q[3], q[0];

// QFT on qubit 1
h q[1];
cp(pi/2) q[2], q[1];
cp(pi/4) q[3], q[1];

// QFT on qubit 2
h q[2];
cp(pi/2) q[3], q[2];

// QFT on qubit 3
h q[3];

// Swap to reverse bit order (QFT convention)
swap q[0], q[3];
swap q[1], q[2];

// Measurement
c = measure q;
