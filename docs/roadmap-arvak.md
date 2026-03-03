# Arvak Roadmap — Prompt Document

> Use this document as a prompt when planning Arvak work. It captures the current
> state, identified gaps, and prioritised next steps informed by the MQT ecosystem
> analysis (2026-03-03).

---

## Current State (v1.9.0)

### Compilation Pipeline
- **Layout:** TrivialLayout (logical i → physical i). No topology-aware placement.
- **Routing:** BasicRouting (greedy BFS SWAP insertion), NeutralAtomRouting (zoned).
- **Translation:** BasisTranslation covering IQM (PRX+CZ), IBM Eagle (ECR+RZ+SX+X),
  IBM Heron (CZ+RZ+SX+X), neutral-atom (RZ+RX+RY+CZ). 1062 lines, largest pass.
- **Optimization:** Optimize1qGates (ZYZ/ZSX/U3 decomposition), CancelCX,
  CommutativeCancellation. Three levels (0–3).
- **No verification pass.** Compiler output is not checked for semantic equivalence.

### HAL & Backends (10 adapters)
- Production: IQM, IBM, Simulator, Scaleway
- Beta: Quantinuum, AQT, Braket, CUDA-Q, QDMI
- Stub: Quandela (DEBT-Q4/Q5 open — photonic encoding + REST)
- HAL Contract v2.3 with capabilities endpoint + parameter binding (QUASI DEBT-24/25 fixed)

### Evaluation
- arvak-eval: input analysis, per-pass observer, QDMI contract checks,
  orchestration DAG, LRZ/LUMI scheduler context, emitter compliance, JSON reports.

### Simulation
- arvak-adapter-sim: statevector only, ≤26 qubits, no noise.
- arvak-sim (Rust): Hamiltonian simulation (Trotter, QDrift), exposed via PyO3.

### Python SDK (v1.9.0)
- Circuit builder, all backends, VQE solver, QAOA solver, NoisyBackend wrapper,
  Nathan research optimizer, Hamiltonian simulation bindings.

---

## Gaps & Priorities

### P0 — Compilation Correctness & Quality

**1. Compilation verification pass (MQT QCEC integration)**
- WHY: Arvak has zero post-compilation verification. A mis-decomposition in
  BasisTranslation (e.g., DEBT-03 ECR qubit-ordering convention) produces silently
  wrong circuits. QCEC catches this with DD + ZX + simulation-based falsification.
- HOW: Add `VerifyCompilation` pass in arvak-compile that exports before/after QASM3,
  calls `mqt.qcec.verify()` via Python subprocess or FFI, and fails the pipeline on
  `not_equivalent`. Gate cost: one DD construction per compilation — acceptable for
  circuits ≤ 50 qubits; sampling-based falsification for larger ones.
- DELIVERABLE: `PassManager` level 2+ includes verification. CI regression tests
  run QCEC on all existing test circuits.

**2. SABRE routing**
- WHY: BasicRouting's greedy BFS inserts far more SWAPs than necessary on real
  topologies (Heavy-Hex, crystal). SABRE's bidirectional lookahead is the industry
  standard (Qiskit, TKET, Staq all use variants).
- HOW: Port SABRE to Rust in `passes/target/routing.rs`. Use MQT QMAP's A*-based
  heuristic as reference for correctness validation. Compare SWAP counts on MQT Bench
  circuits.
- DELIVERABLE: `SabreRouting` pass, default at optimization level ≥ 1. Benchmark
  showing SWAP reduction vs BasicRouting on Heavy-Hex (127q) and crystal (20q).

**3. DenseLayout (topology-aware placement)**
- WHY: TrivialLayout wastes connectivity. Placing logical qubits into the
  best-connected subgraph reduces routing depth.
- HOW: Score physical qubit subsets by interaction density from the circuit's
  two-qubit gate graph. MQT QMAP's exact placement (small circuits) and heuristic
  placement (large) are reference implementations.
- DELIVERABLE: `DenseLayout` pass, default at optimization level ≥ 2.

### P1 — Simulation Capability

**4. Decision-diagram simulator backend (MQT DDSIM)**
- WHY: arvak-adapter-sim caps at 26 qubits with full statevector. DDSIM handles
  structured circuits (GHZ, QFT, Grover) at 100+ qubits using decision diagrams.
  Also provides noise simulation (amplitude damping, depolarization, phase flip)
  which Arvak lacks entirely.
- HOW: New `arvak-adapter-ddsim` wrapping `mqt.ddsim` via the Qiskit backend API
  or direct Python calls. Expose noise configuration through `NoiseProfile` in
  capabilities.
- DELIVERABLE: `ddsim` backend selectable via `--backend ddsim`. Noise-aware
  simulation available. Benchmark comparison vs statevector sim on GHZ(50), QFT(30).

**5. ConsolidateBlocks pass**
- WHY: Merging sequences of single-qubit gates into a single U3 is already done
  (Optimize1qGates), but two-qubit block consolidation (finding KAK decomposition
  of 2-qubit subcircuits) can reduce CX/CZ count significantly.
- HOW: Identify maximal 2-qubit blocks in the DAG, compute 4×4 unitary,
  KAK-decompose, replace if the decomposition uses fewer gates.
- DELIVERABLE: `ConsolidateBlocks` pass at optimization level 3.

### P2 — Ecosystem & Interop

**6. MQT Bench integration in arvak-eval**
- WHY: Arvak's benchmarks (GHZ, QFT, Grover, Random) are self-generated.
  MQT Bench provides 60,000+ benchmark circuits at 4 abstraction levels across
  multiple algorithms, enabling apples-to-apples comparison with Qiskit/TKET output.
- HOW: Import MQT Bench QASM3 files into `arvak-eval benchmark` suite. Add
  comparison mode: compile same circuit with Arvak vs Qiskit, diff gate counts.
- DELIVERABLE: `arvak eval --bench mqt --algorithm qaoa --qubits 16` command.

**7. ZX-calculus optimization pass (inspired by MQT Core ZX package)**
- WHY: ZX-calculus rewrites (spider fusion, pivoting, local complementation)
  can simplify Clifford+T circuits beyond what gate-level peephole optimizations
  achieve. MQT Core's ZX package and PyZX are mature references.
- HOW: Implement ZX graph extraction from CircuitDag, apply simplification rules,
  extract back to circuit. Start with Clifford-only circuits, extend to Clifford+T.
- DELIVERABLE: `ZXOptimize` pass at optimization level 3. Benchmark on
  Clifford-heavy circuits (QEC syndrome extraction, stabilizer circuits).

### P3 — Backend Expansion

**8. Quandela photonic adapter (DEBT-Q4/Q5)**
- Blocked on Perceval client availability. Dual-rail encoding pass + REST submission.

**9. NAViz integration for neutral-atom visualization**
- MQT NAViz is written in Rust — potential integration with arvak-dashboard
  for visualizing atom shuttling schedules from NeutralAtomRouting output.

---

## MQT Integration Summary

| MQT Tool | Arvak Use | Priority | Integration Path |
|----------|-----------|----------|-----------------|
| QCEC | Compilation verification | P0 | Python subprocess / FFI |
| QMAP | Routing reference + neutral-atom compiler | P0 (reference) | Benchmark comparison |
| DDSIM | Noise-aware simulation backend | P1 | New adapter crate |
| Bench | Standardized benchmarks | P2 | QASM3 import in arvak-eval |
| Core (ZX) | ZX optimization pass | P2 | Algorithm port to Rust |
| NAViz | Neutral-atom visualization | P3 | Dashboard integration |
| QECC | QEC compilation target | Future | When fault-tolerant compilation needed |

---

## Non-Goals (This Cycle)

- Replacing Arvak's IR with MQT Core IR (incompatible design: Arvak is DAG-first, MQT is list-first)
- MQT Predictor integration (Arvak's HAL capabilities endpoint + Garm handle this)
- Qudits support (MQT Qudits — Arvak is qubit-only for foreseeable future)
