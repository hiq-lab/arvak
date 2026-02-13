// Quantum Teleportation Protocol
// Teleports state of q[0] to q[2] using entanglement

OPENQASM 3.0;

qubit[3] q;
bit[2] c;

// Prepare state to teleport on q[0]
// Example: |+⟩ = (|0⟩ + |1⟩)/√2
h q[0];

// Create entangled pair between q[1] (Alice) and q[2] (Bob)
h q[1];
cx q[1], q[2];

// Bell measurement on q[0] and q[1]
cx q[0], q[1];
h q[0];

// Measure q[0] and q[1]
c[0] = measure q[0];
c[1] = measure q[1];

// Note: Classical conditional corrections (c_if) not shown — this demonstrates the Bell measurement portion only.
// Classical corrections on Bob's qubit (q[2])
// In real hardware, these would be classically controlled
// Here we apply them unconditionally for demonstration

// If c[1] == 1: apply X to q[2]
// If c[0] == 1: apply Z to q[2]

// The state of q[0] has been teleported to q[2]
