# HAL Contract Compliance — Technical Debt Register

## Current Status: ~25% Compliant

| Contract Layer | Required | Implemented | Status |
|:--------------|:--------:|:-----------:|:------:|
| Layer 1: Core | 4 methods | 3 methods | 75% (missing cancel) |
| Layer 2: Capabilities | 3 methods | 0 methods | 0% |
| Layer 3: Extensions | Optional | 0 methods | N/A |
| Error Codes | 10 standard | Raw exceptions | 0% |

---

## Debt Items

### D1: Missing Cancel Functionality [CRITICAL]

**Layer:** 1 (Core Contract)

`HalCore::cancel()` is not implemented in `ArvakIBMBackend` or `ArvakScalewayBackend`. Long-running jobs cannot be interrupted.

**Trigger:** When users submit large QAOA circuits and want to abort.

---

### D2: No Capabilities Introspection [CRITICAL]

**Layer:** 2 (Capabilities Contract)

`_get_backend_info()` returns a raw dict, not the HAL `Capabilities` struct. Missing: `GateSpec[]`, `Topology` with topology_type, `Constraints` (max_shots/depth/gates), extensions list, calibration timestamp.

**Trigger:** When third-party tools expect HAL-compliant device discovery.

---

### D3: Missing Validation Endpoint [HIGH]

**Layer:** 2 (Capabilities Contract)

No `validate(circuit)` method. Circuits are submitted directly without pre-validation. Invalid circuits produce cryptic IBM/Scaleway errors.

**Trigger:** When submitting circuits with unsupported gates or exceeding qubit limits.

---

### D4: Raw Exceptions Instead of HAL Error Codes [CRITICAL]

**Layer:** 1 (Core Contract)

All error paths return `RuntimeError` with raw backend text instead of mapping to HAL standard error codes (AUTHENTICATION_FAILED, INVALID_CIRCUIT, JOB_FAILED, RATE_LIMITED, etc.).

**Trigger:** When clients need programmatic error handling or retry logic.

---

### D5: Python Bypasses Rust HAL Adapters [HIGH]

**Layer:** Architectural

Python `ArvakIBMBackend` makes direct HTTP calls instead of wrapping the Rust `IbmBackend` (which implements HalCore correctly). Duplicated logic, inconsistent error handling.

**Trigger:** When fixing bugs in one path but not the other, or adding new HAL features.

---

### D6: Missing Availability Status [MEDIUM]

**Layer:** 2 (Capabilities Contract)

`backend.status()` returns a human-readable string, not `AvailabilityStatus` struct with queue_depth, estimated_wait, maintenance window.

**Trigger:** When scheduling algorithms across multiple backends by queue depth.

---

### D7: Hardcoded EU/US Endpoint Routing [MEDIUM]

**Layer:** Configuration

EU backend set is hardcoded as `_IBM_EU_BACKENDS`. Adding new IBM backends requires code changes.

**Trigger:** When IBM adds new regional backends.

---

### D8: Implicit Heron/Eagle Detection [MEDIUM]

**Layer:** Compilation Strategy

Backend processor type detected by checking `"cz" in basis_gates`. Fragile and undocumented.

**Trigger:** When IBM changes basis gate naming or adds new processor types.

---

### D9: No Contract Version Advertising [MEDIUM]

**Layer:** 1 (Core Contract)

No `contract_version()` method. Clients cannot verify HAL compatibility.

**Trigger:** When deploying contract v2.0.0 alongside v1.0.0 backends.

---

### D10: String Job Status Instead of Typed Enum [MEDIUM]

**Layer:** 1 (Core Contract)

`ArvakIBMJob.status()` returns a string, not the HAL `JobStatus` enum with error details and queue position.

**Trigger:** When clients need structured status information.

---

### D11: Ad-hoc QASM3 Post-Processing [LOW]

**Layer:** Circuit Handling

Regex-based gate decomposition (RZZ/CZ/RX→ECR) is correct but fragile. Should be formalized as a compiler pass in Arvak's Rust compiler (ECR basis support).

**Trigger:** When Arvak's QASM3 emitter format changes.

---

### D12: No Circuit Format Negotiation [MEDIUM]

**Layer:** 1 + 2

Only QASM3 format supported. No QIR or native format submission. No format negotiation with `Capabilities.constraints.supported_formats`.

**Trigger:** When QIR-based tools want to use Arvak IBM backends.

---

## Remediation Priority

**Phase 1 (Critical — before production):**
- D1: Implement cancel()
- D4: Map errors to HAL codes
- D2: Implement capabilities()

**Phase 2 (High — within 3 months):**
- D3: Implement validate()
- D5: Wrap Python around Rust HAL adapters

**Phase 3 (Medium — within 6 months):**
- D6-D10: Polish and standardize

**Phase 4 (Low — nice to have):**
- D11: ECR compiler pass in Rust
- D12: QIR format support
