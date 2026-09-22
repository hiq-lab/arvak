# Vendor integration readiness

Assessment date: **2026-09-22**. Scope: quantum hardware vendors that are *not*
yet behind an Arvak adapter, scored against what `arvak-hal`'s `Backend` trait
actually needs. Triggered by a question about QuiX Quantum and SaxonQ.

This is a snapshot of a fast-moving market. Re-run the checks in
[Method](#method) before acting on a "not ready" verdict that is more than a
quarter old.

## Readiness gates

An adapter is buildable when all seven hold. They come straight from the HAL
contract ([hal-contract.md](hal-contract.md)), not from a generic vendor
checklist.

| # | Gate | Why the HAL needs it |
|---|------|----------------------|
| 1 | Remote endpoint reachable without on-prem hardware | Adapters are network clients; there is no local-transport adapter shape except QDMI FFI |
| 2 | Wire contract a Rust `reqwest` client can target (REST/gRPC/OpenAPI) | Adapters are Rust. A Python-only SDK forces a subprocess bridge — see `arvak-adapter-quandela`'s `perceval_bridge.py`, which we do not want to repeat |
| 3 | Gate-based ingest, ideally OpenQASM 3 | `Circuit` → QASM3 is the universal hand-off in `arvak-qasm3` |
| 4 | Device metadata: qubit count, gate set, topology, shot limits | `capabilities()` is **sync and infallible** — metadata must be fetchable once at `connect()` and cached |
| 5 | Async job lifecycle: submit → id → status → result → cancel | The five required async trait methods |
| 6 | Shot results as bitstring counts | `ExecutionResult.counts` |
| 7 | Obtainable credentials | Required for the live round-trip tests we hold every adapter to |

## Verdicts

| Vendor | Technology | Verdict | Blocking gate |
|--------|-----------|---------|---------------|
| **QuiX Quantum** (Bia) | Photonic | ❌ Not ready | 2, 3 |
| **SaxonQ** | NV centres (diamond) | ✅ Ready — **via DLR QCI Connect**, not via SaxonQ | none (7 is gated on an account) |
| Pasqal | Neutral atoms | 🟡 Partial | 3 (analog-native) |
| OQC | Superconducting | ✅ Ready | none |
| Alice & Bob | Cat qubits | ✅ Ready | none |
| Rigetti | Superconducting | 🟡 Medium effort | 3 (Quil, not QASM) |
| planqc | Neutral atoms | ❌ Not ready | 1, 2 |
| eleQtron | Trapped ions | ❌ Not ready | 2 |
| Quantum Brilliance | NV centres | ❌ Not ready | 1, 2 |
| Xanadu | Photonic (CV) | 🟡 Partial | 3 (not gate-based) |
| QpiAI | Superconducting | ✅ Ready | none |

### QuiX Quantum — not ready, two independent blockers

QuiX launched **Bia**, its first QaaS offering. Two things stop an adapter:

1. **Bia is not a gate-based machine.** It is positioned for problems that map
   natively onto photonic sampling — boson sampling, quantum walks, quantum
   Monte Carlo. Arvak submits a gate-based `Circuit` and expects bitstring
   counts back; there is no meaningful translation from our IR onto a sampler
   without a universal gate set. QuiX's ~8-qubit *universal* photonic machine
   with a complete gate set is 2026 roadmap, with a 64-qubit follow-on not
   before 2027.
2. **No public API surface.** Access is either direct "scientific access"
   arranged by contact, or resold through QMware's `basiq`/`qognite` stack for
   industrial customers. There is no published REST contract and no SDK:
   `quix`, `quix-quantum`, `quixsdk` and `bia-sdk` are all absent from PyPI.

Note the encoding half of the problem is not novel for us —
`arvak-adapter-quandela` already handles dual-rail photonic encoding, and
`Capabilities` carries a `"photonic"` feature flag. What is missing is any
Perceval-equivalent programming layer for QuiX plus a machine that accepts
gates at all.

**Recommendation:** hold. Re-evaluate when the universal 8-qubit system ships
with a documented interface, or if QMware exposes a documented REST surface we
could target as a gateway.

### SaxonQ — ready, but the adapter is not a SaxonQ adapter

SaxonQ sells room-temperature NV-diamond machines (QC2026 dual-core, 2×5
qubits; SXQ128 and SXQ512 announced) rather than running a public cloud.
Systems are deployed at DLR and Fraunhofer IWU, programmed through their
**Gemstone** control software, which does expose an API with a job queue and
OpenQASM/Qiskit ingest — but that is a system-integration API delivered with a
machine. There is no public SaxonQ SDK (`saxonq`, `saxon-q`, `gemstone-qc` are
all absent from PyPI), so gates 1 and 7 fail against SaxonQ directly.

**SaxonQ's 4-qubit mobile system is, however, live on DLR QCI Connect** — and
QCI Connect passes every gate. It is a real, versioned, OpenAPI-specified
platform:

- Python SDK `qciconnect` 0.11.0 and API client `qciconnect-client` 1.4.0 on
  PyPI, the latter generated from an `openapi.yml` — so the wire contract is
  published, and a Rust client is a normal port rather than reverse
  engineering.
- REST base path `/api/v5`, OAuth2 password-bearer auth with
  `/token/create` and `/token/invalidate`.
- OpenQASM 2 **and** OpenQASM 3 ingest (`CircuitFormat` enum), which is exactly
  our hand-off format.

The endpoint set maps onto the `Backend` trait with no impedance mismatch:

| `Backend` method | QCI Connect surface |
|------------------|---------------------|
| `capabilities()` | `GET /qpu/all` → `QPUInfo` + `QPURestrictedFields`: `number_of_qubits_maximum`, `upper_shot_limit`/`lower_shot_limit`, `gate_set.native_gate_set` and `accepted_gate_set`, `base_coupling_list` → `Topology` edges, `gate_fidelities` → `NoiseProfile`, `technology` → `features` |
| `availability()` | `QPUStatus`, `allowed_statuses_for_job_submission`, `number_of_scheduled_tasks` — the last gives us real queue depth for `BackendAvailability` |
| `validate()` | `POST /job/validate_job`, on top of the local capability checks the contract requires |
| `submit()` | `POST /job/submit` — `JobSubmission { circuit, circuit_format: OPENQASM3, primitive: sampling, tasks[] }` |
| `status()` | `GET /job/{id}/status` — `PENDING`/`RUNNING`/`SUCCEEDED`/`FAILED`/`CANCELED` maps onto `JobStatus` one-for-one |
| `result()` | `GET /result/{id}` → `QPUTaskResult.aggregated_data` |
| `cancel()` | `POST /job/{id}/cancel` |

Its `QPUTechnology` enum covers `vacancy_centres` (SaxonQ), `trapped_ions`,
`neutral_atoms`, `photons`, `solid_state_spins` and
`statevector_simulator` — so **one adapter covers the whole DLR fleet**, in the
same gateway shape as `arvak-adapter-braket`. Build
`arvak-adapter-qciconnect`, naming targets `qci:<qpu_alias>`; do not build
`arvak-adapter-saxonq`.

Four things to get right when we do:

1. **Metadata fetch must happen at `connect()`.** `gate_set`, `base_coupling_list`
   and the shot limits sit inside `restricted_fields` and are all `Optional`.
   `capabilities()` is sync and infallible, so follow the
   `IbmBackend::connect()` pattern and fail loudly at construction rather than
   shipping a placeholder topology — the deprecated `Capabilities::ibm()`
   factory is the cautionary tale.
2. **Result formats are plural.** `QPUResultDataFormat` is
   `samples`/`sample_distribution`/`populations_no_error`/`populations_with_error`/`quantum_state`.
   Only the first two map to `ExecutionResult.counts`; the rest should return
   `HalError::Unsupported` rather than being coerced.
3. **Verify bit order against hardware.** Same trap the Quandela adapter
   flagged at `adapters/arvak-adapter-quandela/src/backend.rs:494`.
4. **Access is application-based.** Gate 7 needs a DLR QCI Connect project
   before we can run the live round-trip test we hold other adapters to.

### Others worth tracking

**Buildable today, ordinary REST adapters.** OQC (`https://cloud.oqc.app/`,
docs at docs.oqc.app, `oqc-qcaas-client` 3.23.0); Alice & Bob Felis Cloud
(API-key auth, `qiskit-alice-bob-provider` 1.2.0, also on Google Cloud
Marketplace); QpiAI (`qpiai-quantum` 0.7.1, open-sourced SDK, docs at
docs.qcloud.qpiai.tech). OQC and several others are *already* reachable through
the existing Braket adapter, so a direct adapter only buys lower latency,
vendor-native features and independence from AWS.

**Partial fits — the mismatch is the HAL, not the vendor.** Pasqal has a clean
REST API (`/api/v1`, `/api/v2`, Auth0, `pasqal-cloud` 0.24.0) but its native
mode is analog pulse programming via Pulser; the digital gate surface is the
narrow part. Xanadu (`strawberryfields`, `xanadu-cloud-client`) is
continuous-variable. Rigetti is well-documented but ingests Quil over QCS/gRPC,
so it needs an IR emitter, not just a client.

**Real hardware, no public programmatic access yet.** planqc (256-qubit
neutral-atom system in Garching; cloud access stated as a plan, nothing on
PyPI), eleQtron (no public SDK; reachable through German national programmes),
Quantum Brilliance (on-prem NV, same shape as SaxonQ).

## The structural conclusion

Two observations matter more than any single vendor:

1. **Gateways beat one-off adapters right now.** The European vendors worth
   having — SaxonQ, planqc, eleQtron — are not building public clouds; they are
   attaching to national platforms. That makes the two gateway paths already in
   this tree the highest-leverage work: the Phase 12 QDMI driver-discovery item
   (the `arvak-qdmi` consumer layer is done and CI-verified against an MQT DDSIM
   device — only the registry surface is missing), and a new QCI Connect
   adapter. One adapter per N vendors, versus one adapter per pre-API vendor.
2. **Our HAL is gate-based and shot-based, and three of the interesting new
   players are not.** QuiX, Xanadu and analog-mode Pasqal all fail gate 3 for
   the same reason. Whether to model non-gate backends is a HAL-level decision,
   not an adapter detail. Worth noting that QCI Connect's own API already makes
   that split explicitly — separate analog device and analog job endpoints, and
   a `Primitive` enum of `sampling` vs `quantum_state`. If we decide to support
   analog backends, that API is a reasonable prior art to copy.

## Method

Vendor sites were not directly reachable from the assessment environment, so
the hard evidence here is drawn from package registries and published API
contracts rather than marketing pages:

- PyPI JSON API probed for each vendor's SDK (presence, version, dependencies,
  declared documentation URLs).
- `qciconnect-client` 1.4.0 sdist downloaded and read: endpoint paths, auth
  settings, and the `CircuitFormat`, `JobStatus`, `QPUInfo`,
  `QPURestrictedFields`, `QPUTechnology`, `QPUResultDataFormat` and `Primitive`
  models quoted above come from that package, not from prose.
- Product positioning, deployment status and roadmap dates from vendor
  announcements and trade press.

To re-run: probe PyPI for the vendor SDK, and if one exists pull its sdist and
read the generated client for the wire contract. That is the fastest way to
tell a real API from a press release.
