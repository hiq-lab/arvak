# Garm Roadmap — Prompt Document

> Use this document as a prompt when planning Garm work. It captures the current
> integration surface with Arvak, identified gaps, and prioritised next steps
> informed by the MQT ecosystem analysis (2026-03-03).

---

## Current State

### What Garm Is
Garm is the multi-instance quantum orchestration platform. It routes quantum
workloads across multiple Arvak deployments and external services, owns the
BQuant REST gateway (migrated from Arvak v1.8.1), and provides multi-tenant
job scheduling.

### Integration Surface with Arvak
- **HAL Contract v2.3** is the sole interface. Garm consumes:
  - `GET /hal/backends` — discover available backends
  - `GET /hal/backends/{name}` — query per-backend capabilities (DEBT-24, fixed)
  - `POST /hal/jobs` — submit circuits with optional parameter bindings (DEBT-25, fixed)
  - `GET /hal/jobs/{id}` — poll job status and results
  - `DELETE /hal/jobs/{id}` — cancel running jobs
- **Capabilities struct** exposes: gate_set, topology, noise_profile, max_shots,
  num_qubits, features (mid_circuit_measurement, dynamic_circuits), is_simulator.
- **Parameter binding** at submission: `parameters: { "theta_0": 1.57 }` in POST body.

### Shared Patterns (Valiant Ops)
- Arvak is upstream for: strict clippy config, `try_from` pattern, DAG operations,
  API consistency rules.
- Garm is upstream for: `.clamp()` at SQL boundaries, service `main.rs` template,
  proto enum sync checklist, match arm logging.

---

## Gaps & Priorities

### P0 — Intelligent Backend Selection

**1. ML-based device selection (MQT Predictor integration)**
- WHY: Garm currently routes jobs based on availability and manual configuration.
  MQT Predictor uses supervised ML to predict which device yields the best results
  for a given circuit — without trial compilations on every target. Evaluated on
  500+ circuits across 7 devices.
- HOW: Integrate `mqt.predictor` as a Garm routing advisor. On job submission:
  1. Extract circuit features (qubit count, depth, gate mix, connectivity).
  2. Query Predictor model for device ranking by expected fidelity.
  3. Cross-reference with live `GET /hal/backends/{name}` capabilities + availability.
  4. Route to best available device.
- TRAINING: MQT Predictor v2.0+ requires user-trained models. Train on Garm's
  historical job results (circuit → backend → fidelity/success).
- DELIVERABLE: `garm route --strategy ml` mode. Fallback to capability-matching
  when model confidence is low.

**2. Pre-submission federated validation**
- WHY: A circuit submitted to the wrong backend fails after queueing (wasted QPU
  time and money). Garm should validate against all candidate backends before routing.
- HOW: Fan out `validate()` calls to candidate backends via HAL Contract. Return a
  compatibility matrix: `{ "ibm_torino": "valid", "iqm_garnet": "requires_transpilation", "aqt_pine": "invalid(qubit_count)" }`.
- DELIVERABLE: `POST /garm/validate` endpoint returning compatibility matrix.
  Automatically excludes invalid backends from routing decisions.

**3. Compilation caching**
- WHY: The same logical circuit compiled for the same backend produces the same
  output. Garm sees repeated submissions (VQE/QAOA parameter sweeps on same ansatz).
  Caching avoids redundant compilation.
- HOW: Content-hash the (circuit QASM3 + backend capabilities + optimization level)
  triple. Cache compiled circuits with TTL (capabilities change when backends update).
  Use Arvak's `content_fingerprint` function for hashing.
- CONSTRAINTS: Cache needs eviction (per CLAUDE.md rules — max entries + TTL).
  Invalidate on backend capability changes.
- DELIVERABLE: Compilation cache with hit-rate metrics. Target: 80%+ hit rate on
  variational workloads.

### P1 — Orchestration Quality

**4. Cost-aware routing**
- WHY: Different backends have different pricing (QPU-seconds, per-shot, per-job).
  Users need cost-optimal routing, not just fidelity-optimal.
- HOW: Extend backend capabilities with optional cost metadata. Garm routing
  considers a composite score: `w1 * expected_fidelity + w2 * (1/cost) + w3 * (1/queue_time)`.
  User-configurable weights via job submission parameters.
- DELIVERABLE: `POST /garm/jobs` accepts `routing_strategy: { "optimize": "cost" | "fidelity" | "speed" | "balanced" }`.

**5. Dry-run simulation (MQT DDSIM integration)**
- WHY: Before spending QPU credits, users want to verify circuit behaviour.
  Garm should offer a "dry-run" mode that simulates locally using DDSIM's
  decision-diagram simulator (handles structured circuits at 100+ qubits).
- HOW: Garm-side DDSIM instance as a virtual backend. On `routing_strategy: "dry_run"`,
  route to DDSIM instead of hardware. Return simulated counts + estimated fidelity
  based on noise model from the intended target backend's capabilities.
- DELIVERABLE: `--dry-run` flag on job submission. Returns simulated results +
  noise-aware fidelity estimate for the target backend.

**6. Multi-backend ensemble execution**
- WHY: Error mitigation via cross-platform comparison. Run the same circuit on
  multiple backends, compare results, flag outliers.
- HOW: `POST /garm/jobs` with `backends: ["ibm_torino", "iqm_garnet"]`.
  Garm fans out, collects results, returns per-backend counts + correlation analysis.
- DELIVERABLE: Ensemble mode with result comparison. Statistical test for
  distribution agreement across backends.

### P2 — Verification & Observability

**7. Post-compilation equivalence checking (MQT QCEC)**
- WHY: Garm compiles circuits for target backends before submission. If compilation
  introduces bugs (especially on unfamiliar backends), the job produces wrong results
  with no diagnostic. QCEC verifies compilation correctness.
- HOW: After Arvak compiles a circuit for the target backend, Garm optionally runs
  `mqt.qcec.verify(original, compiled)` before submission. If not equivalent, reject
  and alert. Enable by default on first submission to a new backend, optional thereafter.
- DELIVERABLE: `verify_compilation: true` flag on job submission. Verification
  result included in job metadata.

**8. SLA tracking & backend health scoring**
- WHY: Garm needs to deprioritise unreliable backends. Track success rate,
  average queue time, and result quality over time.
- HOW: Record per-backend metrics from HAL responses. Compute rolling health
  score. Auto-deprioritise backends below threshold. Alert on availability drops.
- DELIVERABLE: `GET /garm/backends/health` dashboard endpoint. Backend health
  score feeds into routing decisions.

**9. Benchmark-driven backend ranking (MQT Bench)**
- WHY: New backends need calibration. Run standardized MQT Bench circuits
  (GHZ, QFT, Grover at various qubit counts) to establish baseline performance.
- HOW: Periodic benchmark jobs using MQT Bench suite. Store results as backend
  performance profiles. Use profiles to initialize Predictor model for new backends.
- DELIVERABLE: `garm benchmark --backend ibm_torino --suite mqt-bench` command.
  Results feed backend health and ML routing model.

### P3 — Advanced Scheduling

**10. Parameter sweep orchestration**
- WHY: VQE/QAOA require hundreds of circuit evaluations with different parameters.
  Garm should handle the outer loop, parallelising parameter evaluations across
  multiple backends using HAL's parameter binding (DEBT-25).
- HOW: Accept a parametric circuit + parameter grid. Fan out submissions using
  `parameters: { "theta_0": value }` in POST body. Collect results. Return
  energy landscape / cost function values.
- DELIVERABLE: `POST /garm/sweeps` endpoint for parameter sweep orchestration.

**11. Hybrid classical-quantum workflow DAG**
- WHY: Real algorithms (VQE, error mitigation, circuit knitting) alternate between
  quantum execution and classical post-processing. Garm should orchestrate the
  full workflow, not just individual circuit submissions.
- HOW: Accept a DAG of tasks (compile → submit → classical-postprocess → resubmit).
  Use Arvak's orchestration DAG concept (arvak-eval) as the spec.
- DELIVERABLE: Workflow DAG engine. Integrate with Arvak's VQE/QAOA solvers.

---

## MQT Integration Summary

| MQT Tool | Garm Use | Priority | Integration Path |
|----------|----------|----------|-----------------|
| Predictor | ML-based device selection | P0 | Python service, trained on Garm job history |
| QCEC | Post-compilation verification | P2 | Python subprocess per compilation |
| DDSIM | Dry-run simulation backend | P1 | Virtual backend in Garm |
| Bench | Backend calibration + ranking | P2 | Periodic benchmark jobs |
| QMAP | Reference for routing quality | — | Indirect (via Arvak compiler improvements) |

---

## Non-Goals (This Cycle)

- Replacing Arvak's compiler (Garm orchestrates, Arvak compiles — separation of concerns)
- Direct MQT Core integration (Garm doesn't manipulate circuits, only routes them)
- QEC-aware scheduling (requires fault-tolerant compilation support in Arvak first)
