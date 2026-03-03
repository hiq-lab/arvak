# Nathan Roadmap — Prompt Document

> Use this document as a prompt when planning Nathan work. It captures the current
> state, identified gaps, and prioritised next steps informed by the MQT ecosystem
> analysis (2026-03-03).

---

## Current State (shipped in Arvak v1.9.0)

### What Nathan Is
Nathan is an AI-powered research optimizer that analyses quantum circuits against
a knowledge base of 7,900+ peer-reviewed papers and 680+ algorithm-to-hardware
mappings. It provides problem classification, hardware fitness assessment,
optimization suggestions with concrete QASM3 rewrites, and relevant paper references.

### API Surface
```python
arvak.nathan.analyze(circuit, backend=None, language=None, anonymize=True) -> AnalysisReport
arvak.nathan.chat(message, context="") -> ChatResponse
arvak.nathan.configure(api_key=None, api_url=None)
```

### Key Data Classes
- `AnalysisReport`: problem_type, suitability (0.0–1.0), recommended_algorithm,
  estimated_qubits, circuit_stats, papers[], suggestions[]
- `Suggestion`: title, description, qasm3 (optional rewrite), impact (high/medium/low)
- `Paper`: title, arxiv_url, problem_type, algorithm, relevance
- `CircuitStats`: num_qubits, gate_count, depth, gate_breakdown, detected_pattern

### Features
- Code anonymisation (strips PII from QASM3/Python before LLM analysis)
- Multi-framework input (Arvak, Qiskit, PennyLane, Cirq — auto-detected + converted)
- Hardware-specific analysis (IQM Garnet, IBM Eagle/Heron, etc.)
- Rich HTML rendering in Jupyter notebooks
- Conversational follow-up via `chat()`
- Web interface at arvak.io/nathan

### Infrastructure
- Backend: `POST https://arvak.io/api/nathan/analyze`, `POST .../chat`
- Auth: Bearer token (optional — 30 req/min free tier)
- Client: httpx, 60s timeout

---

## Gaps & Priorities

### P0 — Suggestion Credibility

**1. Verified rewrites via MQT QCEC**
- WHY: Nathan suggests QASM3 circuit rewrites, but there is zero guarantee that a
  rewrite preserves circuit semantics. An LLM can hallucinate a "simpler" circuit
  that computes something different. This is Nathan's biggest credibility gap.
- HOW: Before returning a `Suggestion` with a `qasm3` rewrite to the user:
  1. Parse both original circuit and suggested rewrite into QASM3.
  2. Run `mqt.qcec.verify(original, suggestion)`.
  3. If `equivalent`: tag the suggestion as "verified" with a green badge.
  4. If `not_equivalent`: discard the rewrite or tag as "unverified (semantic change)".
  5. If verification times out (large circuits): tag as "unverified (too large to check)".
- IMPACT: Transforms Nathan from "AI suggestion tool" to "verified optimization engine".
  Dramatically increases trust for production use.
- DELIVERABLE: `Suggestion.verified: bool` field. Verification badge in notebook
  rendering. Only verified suggestions get the `.circuit` property that converts
  back to `arvak.Circuit`.

**2. Optimal Clifford rewriting via MQT QMAP**
- WHY: Nathan's LLM generates approximate rewrites. For Clifford subcircuits
  (H, S, CX — very common in QEC, state prep, stabiliser circuits), MQT QMAP's
  SAT-based Clifford synthesis produces **provably optimal** decompositions.
- HOW: When Nathan detects a Clifford-heavy circuit region:
  1. Extract the Clifford subcircuit.
  2. Call `mqt.qmap.optimize_clifford(subcircuit)` for depth-optimal or
     gate-optimal synthesis.
  3. Return the result as a verified suggestion (QCEC confirms equivalence).
- DELIVERABLE: Clifford-specific suggestions marked as "optimal (SAT-proven)".
  Higher impact rating than LLM-generated suggestions.

### P1 — Knowledge & Analysis Depth

**3. MQT Bench corpus integration**
- WHY: Nathan's knowledge base has 7,900 papers + 680 algorithm-hardware mappings.
  MQT Bench provides 60,000+ canonical algorithm implementations at 4 abstraction
  levels. Cross-referencing against Bench gives Nathan concrete circuit exemplars —
  not just paper descriptions, but actual reference implementations.
- HOW: Index MQT Bench circuits by algorithm type, qubit count, and gate composition.
  When Nathan identifies a problem type (e.g., "this looks like QAOA"), show the
  user the MQT Bench reference implementation at the same scale for comparison.
- DELIVERABLE: `AnalysisReport.reference_circuits[]` field linking to MQT Bench
  exemplars. "Compare to reference" button in notebook rendering.

**4. Noise-aware hardware assessment via DDSIM**
- WHY: Nathan's hardware suitability score is currently LLM-estimated. With DDSIM's
  noise simulation, Nathan can actually simulate the circuit under the target
  backend's noise model and report an empirical expected fidelity.
- HOW: On `analyze(circuit, backend="ibm_torino")`:
  1. Fetch backend's noise_profile from HAL capabilities.
  2. Simulate with DDSIM using matching noise model (depolarization, amplitude damping).
  3. Compare noisy output distribution to ideal.
  4. Report empirical fidelity as `suitability` score.
- DELIVERABLE: `suitability` becomes simulation-backed, not LLM-guessed.
  `AnalysisReport.simulated_fidelity: Optional[float]` field.

**5. QEC recommendation engine (MQT QECC)**
- WHY: When a circuit exceeds a backend's fidelity threshold (too deep, too many
  2-qubit gates), Nathan currently says "consider error mitigation." It should
  instead recommend specific QEC schemes.
- HOW: When Nathan detects a circuit-backend mismatch (low suitability):
  1. Estimate logical error rate from circuit depth and backend noise profile.
  2. Query QECC for suitable codes at that error rate (color codes, surface codes).
  3. Report code distance, overhead (physical/logical qubit ratio), and decoder.
- DELIVERABLE: `Suggestion` type "error_correction" with code recommendation,
  overhead estimate, and link to QECC documentation.

### P2 — User Experience

**6. Multi-turn optimization workflow**
- WHY: `chat()` supports context strings but has no session management. A real
  optimization workflow is iterative: analyze → apply suggestion → re-analyze →
  compare. Nathan should guide this loop.
- HOW: Add `Session` class that tracks circuit versions, applied suggestions,
  and metric history. `session.apply(suggestion)` applies a rewrite and
  re-analyzes. `session.compare()` shows before/after metrics.
- DELIVERABLE: `nathan.Session(circuit)` with `.analyze()`, `.apply(idx)`,
  `.compare()`, `.history` API.

**7. Suggestion auto-application**
- WHY: `Suggestion.circuit` converts QASM3 back to `arvak.Circuit`, but the user
  must manually replace their circuit. Nathan should offer one-click application
  with undo.
- HOW: `report.apply(suggestion_index)` returns a new `Circuit` with the rewrite
  applied. Works by parsing the suggestion's QASM3 target region, replacing it in
  the original circuit, and running QCEC verification (P0 item 1).
- DELIVERABLE: `AnalysisReport.apply(idx) -> Circuit`. Undo via
  `report.original_circuit`.

**8. Custom knowledge sources**
- WHY: Enterprise users have proprietary circuit libraries and internal papers
  that Nathan's public knowledge base doesn't cover.
- HOW: Allow users to register additional paper/circuit sources via
  `nathan.configure(knowledge_sources=[...])`. Sources are private to the user's
  API key scope.
- DELIVERABLE: `nathan.add_source(path_or_url)` for private knowledge bases.

### P3 — Platform Integration

**9. Nathan-in-Garm (orchestration-time analysis)**
- WHY: When Garm receives a circuit for routing, Nathan can pre-analyze it and
  feed the analysis into routing decisions (e.g., "this is a QAOA circuit,
  route to a backend with good Rz fidelity").
- HOW: Garm calls Nathan's analyze endpoint as part of its routing pipeline.
  Nathan's `problem_type` and `suitability` per-backend feed the routing score.
- DELIVERABLE: `POST /garm/jobs` optionally includes Nathan analysis in routing.

**10. Compiler-integrated suggestions**
- WHY: Nathan suggests gate-level optimizations that Arvak's compiler could
  apply automatically. Bridge the gap between Nathan's intelligence and
  Arvak's execution.
- HOW: Nathan produces `Suggestion` objects with QASM3 rewrites. Arvak's
  `PassManager` accepts a `NathanPass` that applies verified suggestions as
  a compilation pass.
- DELIVERABLE: `NathanOptimize` pass in arvak-compile. Opt-in at level 3.

---

## MQT Integration Summary

| MQT Tool | Nathan Use | Priority | Integration Path |
|----------|------------|----------|-----------------|
| QCEC | Verify suggested rewrites | P0 | Server-side Python call before returning suggestions |
| QMAP | Optimal Clifford synthesis | P0 | Server-side call for Clifford subcircuit suggestions |
| Bench | Reference circuit corpus | P1 | Index + cross-reference in knowledge base |
| DDSIM | Noise-aware fidelity scoring | P1 | Server-side simulation for suitability score |
| QECC | QEC scheme recommendations | P1 | Error correction suggestions when fidelity is low |

---

## Non-Goals (This Cycle)

- Replacing the LLM backend with deterministic analysis (LLM remains for natural
  language understanding and paper-to-circuit reasoning — MQT tools augment, not replace)
- Offline mode (Nathan requires API connectivity; MQT tools would run server-side)
- Training a custom ML model (MQT Predictor handles device selection — Nathan handles
  circuit-level optimisation advice)
