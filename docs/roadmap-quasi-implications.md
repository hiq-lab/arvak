# QUASI Implications — MQT Ecosystem Impact

> Analysis of what the MQT ecosystem means for QUASI quantum OS, given the
> existing Arvak↔QUASI integration surface (DEBT-24, DEBT-25) and the Afana
> Ehrenfest compiler.

---

## Context: QUASI↔Arvak Integration Surface

QUASI is an upstream quantum OS that depends on Arvak's HAL Contract. Two critical
integration points were resolved on 2026-03-01:

- **DEBT-24**: `GET /hal/backends/{name}` capabilities endpoint — QUASI's quasi-board
  and Afana compiler need to discover gate sets, topology, and noise profiles at
  runtime. Fixed: capabilities endpoint added to HAL HTTP REST surface.
- **DEBT-25**: Parameter binding in `submit()` — Ehrenfest v0.2 parametric programs
  (VQE, QAOA, Trotter sweeps) need parameter maps at submission time, not baked
  into the compiled circuit. Fixed: `parameters: Option<HashMap<String, f64>>` added
  to `Backend::submit()` + all 9 adapters + REST gateway.

The Afana Ehrenfest compiler emits CBOR binary programs that compile down to
OpenQASM 3.0 with `input float[64]` parameter declarations.

---

## Implications by MQT Tool

### 1. QCEC → Ehrenfest Compilation Verification (HIGH IMPACT)

**Problem QUASI has today:**
Afana compiles Ehrenfest programs (physics-layer Hamiltonian descriptions) down to
OpenQASM 3.0 circuits. There is no verification that this compilation preserves
the physics. A bug in Afana's decomposition rules produces wrong circuits that
run successfully on hardware but give wrong answers.

**What MQT QCEC enables:**
- Verify that Afana's compiled QASM3 output is equivalent to a reference circuit
  derived from the Ehrenfest Hamiltonian specification.
- For parametric circuits: QCEC supports parameterized equivalence checking
  (handles symbolic `input float[64]` declarations). This means a single
  verification run can cover the entire parameter space, not just one point.
- Post-DEBT-25: Since parameter binding now happens at HAL submission (not in Afana),
  QCEC can verify the *parametric* circuit once, and all subsequent parameter
  bindings are guaranteed correct.

**Concrete integration:**
```
Ehrenfest CBOR → Afana compile → QASM3 (parametric)
                                    ↓
                              QCEC.verify(reference, compiled)
                                    ↓
                              HAL submit(circuit, shots, parameters)
```

**QUASI action item:** Add QCEC verification step between Afana compilation and
HAL submission. Run once per parametric circuit (not per parameter set).

---

### 2. DDSIM → Ehrenfest Noise-Aware Simulation (HIGH IMPACT)

**Problem QUASI has today:**
DEBT-24 was caused by noise model information being incorrectly embedded in
Ehrenfest programs (wrong layer). The fix moved noise model discovery to the
HAL capabilities endpoint. But QUASI still lacks a way to simulate Ehrenfest
programs under realistic noise before committing to QPU execution.

**What MQT DDSIM enables:**
- Simulate the compiled QASM3 circuit with noise models matching the target
  backend's `noise_profile` from the capabilities endpoint.
- Decision-diagram simulation handles structured Hamiltonian evolution circuits
  (Trotter steps have repetitive structure that DDs exploit) at higher qubit
  counts than statevector.
- Noise modes: depolarization, amplitude damping, phase flip — match the
  `noise_profile` fields (T1, T2, single/two-qubit fidelity, readout fidelity)
  from `GET /hal/backends/{name}`.

**Concrete integration:**
```
GET /hal/backends/{name} → noise_profile
                              ↓
compiled QASM3 → DDSIM.simulate(circuit, noise=noise_profile, shots=1024)
                              ↓
                        expected fidelity estimate
                              ↓
                  decision: submit to QPU or abort
```

**QUASI action item:** Add DDSIM as a "dry-run" option in quasi-board. Before
QPU submission, estimate fidelity. If below threshold, warn user or suggest
alternative backend.

---

### 3. QMAP → Topology-Aware Routing for quasi-board (MEDIUM IMPACT)

**Problem QUASI has today:**
quasi-board needs topology-aware routing (DEBT-24 context: "gate-set selection,
topology-aware routing, and backend selection logic in quasi-board — all decisions
that require backend capabilities"). Currently depends on Arvak's BasicRouting
which is suboptimal.

**What MQT QMAP enables:**
- A*-based heuristic mapping that produces significantly fewer SWAPs than
  BasicRouting's greedy BFS.
- **Neutral-atom compiler** with zone-aware scheduling — directly relevant if
  QUASI targets IQM or other neutral-atom backends.
- **Exact optimal mapping** (MaxSAT) for small circuits — useful for verifying
  that routing decisions are correct on test cases.

**QUASI action item:** If quasi-board performs its own routing (instead of
delegating to Arvak's PassManager), consider calling QMAP directly. Otherwise,
benefit comes indirectly when Arvak upgrades to SabreRouting (Arvak roadmap P0).

---

### 4. Predictor → Backend Selection for quasi-board (MEDIUM IMPACT)

**Problem QUASI has today:**
quasi-board selects backends based on capability matching (gate set, qubit count).
This doesn't account for empirical performance — a backend might support the right
gates but have poor fidelity for the specific circuit structure.

**What MQT Predictor enables:**
- ML-based prediction of which backend gives best results for a given circuit,
  without trial compilation on every target.
- Combined with DEBT-24's capabilities endpoint, quasi-board can: (1) filter
  by capability, then (2) rank by Predictor's expected fidelity model.

**QUASI action item:** Train Predictor models on QUASI's job history. Integrate
as a ranking step after capability filtering in quasi-board's backend selection.

---

### 5. QECC → Fault-Tolerant Ehrenfest (FUTURE)

**Not immediately relevant**, but becomes critical when QUASI targets fault-tolerant
execution:

- MQT QECC's lattice surgery compiler can compile CNOT+T circuits for color codes.
- State preparation synthesis for CSS codes would apply to Ehrenfest's initial
  state preparation step.
- Noise threshold analysis: given QUASI's Hamiltonian and a backend's noise profile,
  estimate whether error correction at distance `d` would produce useful results.

**QUASI action item:** Track QECC maturity. When QUASI adds fault-tolerant mode,
QECC is the likely integration point for code selection and compilation.

---

### 6. MQT Core (ZX-calculus) → Ehrenfest Optimization (LOW-MEDIUM)

**Potential benefit:**
Ehrenfest-compiled circuits contain structured gate sequences from Trotter
decomposition. ZX-calculus simplification can reduce these structured patterns
more effectively than peephole optimisation. MQT Core's ZX package provides a
mature implementation.

**QUASI action item:** Evaluate ZX simplification on compiled Ehrenfest circuits.
If gate count reduction > 15%, consider adding a ZX pass to Afana's compilation
pipeline.

---

## Priority Matrix for QUASI

| MQT Tool | QUASI Benefit | Effort | Priority |
|----------|--------------|--------|----------|
| QCEC | Compilation correctness guarantee for Afana | Low (Python call) | **P0** |
| DDSIM | Noise-aware dry-run before QPU submission | Low (Python call) | **P0** |
| QMAP | Better routing (indirect via Arvak, or direct) | Medium | **P1** |
| Predictor | ML-based backend selection for quasi-board | Medium (training) | **P1** |
| Core (ZX) | Trotter circuit optimization | Medium | **P2** |
| QECC | Fault-tolerant mode | High | **Future** |

---

## Architectural Observation

The DEBT-24 fix (capabilities endpoint) is the linchpin. Almost every MQT tool
integration in QUASI depends on querying backend capabilities:

- DDSIM needs `noise_profile` to simulate realistically.
- Predictor needs gate set + topology to predict device fitness.
- QMAP needs topology for routing.
- QCEC needs basis gates to verify compilation correctness.

With DEBT-24 resolved, **all four P0/P1 integrations are unblocked on the HAL side**.
The remaining work is purely in QUASI/quasi-board/Afana.

Similarly, DEBT-25 (parameter binding) means QCEC can verify parametric circuits
once and the verification covers all subsequent parameter bindings — a massive
efficiency win for VQE/QAOA workloads that QUASI specialises in.

---

## Risk: MQT is C++/Python, QUASI is (?)

All MQT tools are C++20 with Python bindings. Integration paths:
1. **Python subprocess** — lowest friction, highest latency. Fine for compilation
   verification (runs once per circuit, not per shot).
2. **Python FFI** — medium friction. Use if QUASI has a Python runtime available.
3. **C++ FFI** — highest friction but lowest latency. Only justified for inner-loop
   operations (which none of these are).
4. **Rust port** — only NAViz is Rust. Porting QCEC/DDSIM to Rust is a major effort
   and not justified unless QUASI is Rust-only with no Python runtime.

**Recommendation:** Python subprocess for QCEC and DDSIM. Both are one-shot calls
per compilation, not per-shot, so latency is acceptable.
