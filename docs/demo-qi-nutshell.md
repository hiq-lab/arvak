# QI-Nutshell Demo: Quantum Communication Protocol Compilation

## What This Demo Proves

This demo takes the quantum communication protocols from the paper *"Quantum Internet in a Nutshell"* (Hilder et al., arXiv:2507.14383v2) and runs them through Arvak's full compilation stack: circuit construction, QASM3 emission, and transpilation to ion-trap native gates.

The purpose is straightforward: if Arvak can express these real-world QKD protocols as circuits and compile them to hardware-executable instructions without errors, it can handle the protocol-to-circuit mapping that quantum networking research requires.

## Protocols Covered

### BB84 (Prepare-and-Measure QKD)

The foundational QKD protocol. Alice prepares a qubit in a randomly chosen basis (Z or X), sends it to Bob, who measures in his own randomly chosen basis. When their bases match, the measurement outcome becomes a shared secret key bit.

The demo builds four scenarios:

| Scenario | Configuration | Qubits | Gates |
|---|---|---|---|
| Clean channel, matching bases | Z basis, bit=1, no Eve | 1 | 5 |
| Basis mismatch | X→Z, no Eve | 1 | 5 |
| PCCM eavesdropping | Z basis, θ=π/4 | 3 | 9 |
| Variational PCCM | Symbolic θ parameter | 3 | 8 |

### BBM92 (Entanglement-Based QKD)

Instead of state preparation, a source distributes Bell pairs to Alice and Bob. Both measure in independently chosen bases. This is the entanglement-based counterpart to BB84.

| Scenario | Configuration | Qubits | Gates |
|---|---|---|---|
| Clean entangled channel | Z basis, no Eve | 2 | 5 |
| Eve clones Bob's half | PCCM, θ=π/4 | 4 | 10 |

### Phase Covariant Cloning Machine (PCCM) Sweep

The PCCM is an optimal eavesdropping attack on equatorial qubits. The attack angle θ controls a fundamental trade-off: how much information Eve extracts versus how much she disturbs the channel.

The demo sweeps θ from 0 to π/2 and computes the theoretical fidelities at each point:

```
     θ/π    F(A→B)    F(A→E)     QBER%
  0.0000    1.0000    0.5000      0.0%     ← no attack
  0.1111    0.9698    0.6710      3.0%
  0.2222    0.8830    0.8214     11.7%
  0.2500    0.8536    0.8536     14.6%     ← symmetric optimum
  0.3333    0.7500    0.9330     25.0%
  0.5000    0.5000    1.0000     50.0%     ← full attack
```

At θ=π/4, both fidelities equal 0.8536 — the optimal symmetric cloning point where Eve extracts maximum information without creating more disturbance than necessary.

### QEC-Integrated QKD ([[4,2,2]] Error Detection)

The paper's key insight: integrating quantum error correction into QKD provides "privacy authentication." The [[4,2,2]] code encodes 2 logical qubits into 4 physical qubits. Stabilizer syndrome measurements serve double duty — they detect transmission errors *and* reveal eavesdropping through statistical deviations from expected syndrome patterns.

| Scenario | Configuration | Qubits | Gates |
|---|---|---|---|
| Clean channel | bits=[1,0], Z basis | 6 | 37 |
| Injected X error | Single bit-flip on data qubit 2 | 6 | 38 |

## How to Run

```bash
# All protocols, default settings
cargo run -p arvak-demos --bin demo-qi-nutshell

# All protocols with QASM3 output and ion-trap compilation
cargo run -p arvak-demos --bin demo-qi-nutshell -- --show-qasm --compile

# Single protocol
cargo run -p arvak-demos --bin demo-qi-nutshell -- --protocol bb84
cargo run -p arvak-demos --bin demo-qi-nutshell -- --protocol bbm92
cargo run -p arvak-demos --bin demo-qi-nutshell -- --protocol pccm-sweep
cargo run -p arvak-demos --bin demo-qi-nutshell -- --protocol qec

# Custom PCCM angle (in units of π)
cargo run -p arvak-demos --bin demo-qi-nutshell -- --protocol bb84 --theta 0.1

# Compilation with specific optimization level (0-3)
cargo run -p arvak-demos --bin demo-qi-nutshell -- --compile -O 3
```

### Options

| Flag | Default | Description |
|---|---|---|
| `--protocol` | `all` | Protocol to run: `bb84`, `bbm92`, `pccm-sweep`, `qec`, `all` |
| `--theta` | `0.25` | PCCM attack angle in units of π (0.0 to 0.5) |
| `--show-qasm` | off | Print generated OpenQASM 3.0 code |
| `--compile` | off | Compile circuits to ion-trap native gates (CZ + PRX) |
| `-O` | `2` | Optimization level for compilation (0-3) |

### Unit Tests

```bash
cargo test -p arvak-demos --lib circuits::qi_nutshell
```

Runs 12 tests covering circuit construction, qubit counts, and PCCM fidelity calculations.

## Compilation Results

When run with `--compile`, Arvak transpiles each protocol circuit to the IQM ion-trap native gate set (CZ + PRX) on a linear chain topology. The full pass pipeline runs: layout mapping, SWAP-based routing, basis translation, and gate optimization.

| Circuit | Pre-depth | Pre-gates | Post-depth | Post-gates | Expansion |
|---|---|---|---|---|---|
| BB84 + PCCM (3q) | 9 | 9 | 15 | 20 | 2.22× |
| BBM92 + PCCM (4q) | 10 | 10 | 18 | 26 | 2.60× |
| BB84 + QEC clean (6q) | 19 | 37 | 122 | 288 | 7.78× |
| BB84 + QEC noisy (6q) | 20 | 38 | 122 | 289 | 7.61× |

The QEC circuits show the highest expansion because the [[4,2,2]] code requires all-to-all connectivity between the 4 data qubits and 2 syndrome ancillae, which on a linear chain topology requires SWAP insertion for routing. This is expected behavior for any compiler targeting constrained connectivity.

### Example QASM3 Output (BB84 + PCCM)

```
OPENQASM 3.0;

qubit[3] q;
bit[1] c;

barrier q[0], q[1], q[2];
x q[0];
barrier q[0], q[1], q[2];
cx q[0], q[1];
ry(pi/4) q[0];
cx q[0], q[1];
cx q[1], q[2];
barrier q[0], q[1], q[2];
c[0] = measure q[0];
```

The barriers delimit protocol phases (preparation → channel → measurement), the CNOT+Ry+CNOT sequence implements the PCCM cloning unitary, and the final measurement extracts Bob's key bit.

## What This Demonstrates About Arvak

1. **Named register support.** Protocols map naturally to registers (`a` for Alice, `b` for Bob, `e` for Eve, `syn` for syndrome ancillae) rather than raw qubit indices.

2. **Parameterized gates.** The variational PCCM uses a symbolic `theta` parameter that can be bound at runtime, enabling hybrid quantum-classical optimization loops (as demonstrated in the paper using COBYLA).

3. **Barrier-delimited protocol phases.** Barriers enforce the logical structure of communication protocols (prepare → transmit → measure) and survive through compilation.

4. **End-to-end compilation.** Circuits go from high-level protocol description through layout, routing, basis translation, and optimization to hardware-executable native gates — without manual intervention.

5. **QASM3 emission.** Every circuit produces valid OpenQASM 3.0, the standard interchange format for quantum programs.

6. **Correct physics.** The PCCM fidelity sweep reproduces the theoretical trade-off curve, the symmetric cloning point lands at θ=π/4, and the QEC stabilizer structure is correctly constructed.
