# HIQ HAL Contract Specification

## Overview

The HAL (Hardware Abstraction Layer) Contract defines the formal interface between HIQ and quantum backends. It provides a stable, versioned API that third parties can implement to make their quantum hardware or simulators accessible through HIQ.

## Design Goals

1. **Stability** — Contract versions are immutable once released
2. **Minimalism** — Smallest viable interface for interoperability
3. **Extensibility** — Optional capabilities without breaking core contract
4. **Language-agnostic** — Definable as Rust trait, OpenAPI, or protobuf

## Contract Layers

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Contract Layers                                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  Layer 3: Extensions (Optional)                                   │  │
│  │  - Pulse-level control                                            │  │
│  │  - Error mitigation                                               │  │
│  │  - Calibration access                                             │  │
│  │  - Session management                                             │  │
│  │  - Batch execution                                                │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  Layer 2: Capabilities (Required)                                 │  │
│  │  - Device topology                                                │  │
│  │  - Gate set                                                       │  │
│  │  - Constraints                                                    │  │
│  │  - Availability status                                            │  │
│  │  - Circuit validation                                             │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  Layer 1: Core (Required)                                         │  │
│  │  - Submit circuit                                                 │  │
│  │  - Query status                                                   │  │
│  │  - Retrieve results                                               │  │
│  │  - Cancel job                                                     │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Layer 1: Core Contract

The core contract defines the minimum interface that all backends MUST implement.

### Rust Trait Definition

```rust
//! HIQ HAL Core Contract v1
//!
//! This is the minimum interface a backend MUST implement.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Contract version identifier.
pub const HAL_CONTRACT_VERSION: &str = "1.0.0";

/// Unique job identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub String);

/// Job execution status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum JobStatus {
    /// Job is waiting in queue.
    Queued { position: Option<u32> },
    /// Job is currently executing.
    Running { progress: Option<f32> },
    /// Job completed successfully.
    Completed,
    /// Job failed with error.
    Failed { error: String, code: Option<String> },
    /// Job was cancelled.
    Cancelled,
}

/// Circuit in a portable format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum CircuitPayload {
    /// OpenQASM 3 text.
    OpenQasm3 { source: String },
    /// QIR bitcode (base64 encoded).
    Qir { bitcode: String },
    /// Backend-native format (opaque).
    Native { data: serde_json::Value },
}

/// Job submission request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitRequest {
    /// Circuit to execute.
    pub circuit: CircuitPayload,
    /// Number of shots.
    pub shots: u32,
    /// Optional: seed for reproducibility (simulators).
    pub seed: Option<u64>,
    /// Optional: client-provided job name.
    pub name: Option<String>,
    /// Optional: client-provided metadata.
    pub metadata: Option<serde_json::Value>,
}

/// Job submission response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitResponse {
    /// Assigned job ID.
    pub job_id: JobId,
    /// Estimated queue time in seconds (if available).
    pub estimated_queue_time: Option<u64>,
}

/// Measurement counts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Counts(pub std::collections::HashMap<String, u64>);

/// Job result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    /// Job ID.
    pub job_id: JobId,
    /// Measurement counts.
    pub counts: Counts,
    /// Actual shots executed.
    pub shots: u32,
    /// Execution time in milliseconds.
    pub execution_time_ms: Option<u64>,
    /// Backend-specific metadata.
    pub metadata: Option<serde_json::Value>,
}

/// Error type for HAL operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HalError {
    /// Error code (for programmatic handling).
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Additional details.
    pub details: Option<serde_json::Value>,
}

/// Result type for HAL operations.
pub type HalResult<T> = Result<T, HalError>;

/// Core HAL contract — MUST be implemented by all backends.
#[async_trait]
pub trait HalCore: Send + Sync {
    /// Return the contract version this backend implements.
    fn contract_version(&self) -> &str {
        HAL_CONTRACT_VERSION
    }

    /// Return the backend identifier.
    fn backend_id(&self) -> &str;

    /// Submit a circuit for execution.
    async fn submit(&self, request: SubmitRequest) -> HalResult<SubmitResponse>;

    /// Get the status of a job.
    async fn status(&self, job_id: &JobId) -> HalResult<JobStatus>;

    /// Get the result of a completed job.
    ///
    /// Returns error if job is not completed.
    async fn result(&self, job_id: &JobId) -> HalResult<JobResult>;

    /// Cancel a job.
    ///
    /// Returns Ok if cancellation was accepted (job may still complete).
    async fn cancel(&self, job_id: &JobId) -> HalResult<()>;
}
```

### Standard Error Codes

| Code | Description |
|------|-------------|
| `AUTHENTICATION_FAILED` | Invalid or missing credentials |
| `AUTHORIZATION_FAILED` | Insufficient permissions |
| `INVALID_CIRCUIT` | Circuit syntax or semantic error |
| `CIRCUIT_TOO_LARGE` | Circuit exceeds backend limits |
| `UNSUPPORTED_FEATURE` | Circuit uses unsupported feature |
| `JOB_NOT_FOUND` | Job ID does not exist |
| `JOB_FAILED` | Job execution failed |
| `BACKEND_UNAVAILABLE` | Backend is offline or in maintenance |
| `RATE_LIMITED` | Too many requests |
| `INTERNAL_ERROR` | Unexpected backend error |

```rust
/// Standard error codes.
pub mod error_codes {
    pub const AUTHENTICATION_FAILED: &str = "AUTHENTICATION_FAILED";
    pub const AUTHORIZATION_FAILED: &str = "AUTHORIZATION_FAILED";
    pub const INVALID_CIRCUIT: &str = "INVALID_CIRCUIT";
    pub const CIRCUIT_TOO_LARGE: &str = "CIRCUIT_TOO_LARGE";
    pub const UNSUPPORTED_FEATURE: &str = "UNSUPPORTED_FEATURE";
    pub const JOB_NOT_FOUND: &str = "JOB_NOT_FOUND";
    pub const JOB_FAILED: &str = "JOB_FAILED";
    pub const BACKEND_UNAVAILABLE: &str = "BACKEND_UNAVAILABLE";
    pub const RATE_LIMITED: &str = "RATE_LIMITED";
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
}
```

---

## Layer 2: Capabilities Contract

The capabilities contract exposes device information for compilation and validation.

### Rust Trait Definition

```rust
//! HIQ HAL Capabilities Contract v1
//!
//! Backends MUST implement this to expose device capabilities.

use serde::{Deserialize, Serialize};

/// Gate definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateSpec {
    /// Gate name (e.g., "cx", "prx").
    pub name: String,
    /// Number of qubits.
    pub num_qubits: u8,
    /// Number of parameters.
    pub num_params: u8,
    /// Is this a native gate?
    pub native: bool,
    /// Gate description.
    pub description: Option<String>,
}

/// Qubit connectivity edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouplingEdge {
    /// Source qubit index.
    pub source: u32,
    /// Target qubit index.
    pub target: u32,
    /// Is this edge bidirectional?
    pub bidirectional: bool,
    /// Edge fidelity (0.0-1.0) if available.
    pub fidelity: Option<f64>,
}

/// Device topology.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topology {
    /// Number of qubits.
    pub num_qubits: u32,
    /// Connectivity edges.
    pub edges: Vec<CouplingEdge>,
    /// Topology type hint.
    pub topology_type: Option<TopologyType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopologyType {
    Linear,
    Star,
    Grid,
    HeavyHex,
    FullyConnected,
    Custom,
}

/// Backend constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraints {
    /// Maximum number of shots per job.
    pub max_shots: u32,
    /// Maximum circuit depth.
    pub max_depth: Option<u32>,
    /// Maximum number of gates.
    pub max_gates: Option<u32>,
    /// Maximum number of qubits usable.
    pub max_qubits: u32,
    /// Supported circuit formats.
    pub supported_formats: Vec<String>,
}

/// Backend availability status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailabilityStatus {
    /// Is the backend currently available?
    pub available: bool,
    /// Current queue depth.
    pub queue_depth: Option<u32>,
    /// Estimated wait time in seconds.
    pub estimated_wait: Option<u64>,
    /// Maintenance message if unavailable.
    pub message: Option<String>,
    /// Next available time (ISO 8601) if in maintenance.
    pub available_at: Option<String>,
}

/// Full backend capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    /// Backend identifier.
    pub backend_id: String,
    /// Backend vendor.
    pub vendor: String,
    /// Backend description.
    pub description: Option<String>,
    /// Is this a simulator?
    pub simulator: bool,
    /// Device topology.
    pub topology: Topology,
    /// Supported gates.
    pub gates: Vec<GateSpec>,
    /// Backend constraints.
    pub constraints: Constraints,
    /// Supported optional extensions.
    pub extensions: Vec<String>,
    /// Last calibration time (ISO 8601).
    pub last_calibrated: Option<String>,
}

/// Circuit validation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Is the circuit valid?
    pub valid: bool,
    /// Validation errors (if any).
    pub errors: Vec<ValidationError>,
    /// Validation warnings (if any).
    pub warnings: Vec<ValidationWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: String,
    pub message: String,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub code: String,
    pub message: String,
    pub location: Option<String>,
}

/// Capabilities contract — MUST be implemented by all backends.
#[async_trait::async_trait]
pub trait HalCapabilities: HalCore {
    /// Get full backend capabilities.
    async fn capabilities(&self) -> HalResult<Capabilities>;

    /// Get current availability status.
    async fn availability(&self) -> HalResult<AvailabilityStatus>;

    /// Check if a specific circuit is valid for this backend.
    async fn validate(&self, circuit: &CircuitPayload) -> HalResult<ValidationResult>;
}
```

---

## Layer 3: Extensions (Optional)

Extensions are optional capabilities that backends MAY implement. Backends advertise supported extensions in the `Capabilities.extensions` field.

### Extension Identifiers

```rust
/// Extension identifiers.
pub mod extensions {
    /// Pulse-level control.
    pub const PULSE: &str = "hal.ext.pulse.v1";
    /// Calibration data access.
    pub const CALIBRATION: &str = "hal.ext.calibration.v1";
    /// Session management.
    pub const SESSIONS: &str = "hal.ext.sessions.v1";
    /// Error mitigation.
    pub const ERROR_MITIGATION: &str = "hal.ext.error_mitigation.v1";
    /// Batch execution.
    pub const BATCH: &str = "hal.ext.batch.v1";
    /// Real-time feedback.
    pub const REALTIME: &str = "hal.ext.realtime.v1";
}

/// Extension discovery trait.
pub trait HalExtensions: HalCapabilities {
    /// Get list of supported extensions.
    fn supported_extensions(&self) -> &[String];

    /// Check if a specific extension is supported.
    fn supports(&self, extension: &str) -> bool {
        self.supported_extensions().contains(&extension.to_string())
    }
}
```

### Batch Extension

For submitting multiple circuits in a single request.

```rust
/// Batch execution extension.
#[async_trait]
pub trait HalBatch: HalCore {
    /// Submit multiple circuits in a single request.
    async fn submit_batch(&self, requests: Vec<SubmitRequest>) -> HalResult<BatchResponse>;

    /// Get results for a batch job.
    async fn batch_results(&self, batch_id: &str) -> HalResult<Vec<JobResult>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResponse {
    pub batch_id: String,
    pub job_ids: Vec<JobId>,
}
```

### Session Extension

For iterative algorithms with lower latency between iterations.

```rust
/// Session management for iterative algorithms.
#[async_trait]
pub trait HalSessions: HalCore {
    /// Create a new session.
    async fn create_session(&self, config: SessionConfig) -> HalResult<SessionId>;

    /// Submit within a session (lower latency).
    async fn session_submit(
        &self,
        session_id: &SessionId,
        request: SubmitRequest,
    ) -> HalResult<SubmitResponse>;

    /// Close a session.
    async fn close_session(&self, session_id: &SessionId) -> HalResult<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Maximum session duration in seconds.
    pub max_duration: u64,
    /// Maximum jobs in session.
    pub max_jobs: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);
```

### Calibration Extension

For accessing device calibration data.

```rust
/// Calibration data access.
#[async_trait]
pub trait HalCalibration: HalCapabilities {
    /// Get current calibration data.
    async fn calibration(&self) -> HalResult<CalibrationData>;

    /// List available calibration sets.
    async fn calibration_sets(&self) -> HalResult<Vec<CalibrationSetInfo>>;

    /// Pin to a specific calibration set.
    async fn set_calibration(&self, set_id: &str) -> HalResult<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationData {
    pub set_id: String,
    pub timestamp: String,
    pub qubit_properties: Vec<QubitCalibration>,
    pub gate_properties: Vec<GateCalibration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QubitCalibration {
    pub qubit: u32,
    pub t1: Option<f64>,
    pub t2: Option<f64>,
    pub readout_fidelity: Option<f64>,
    pub frequency: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCalibration {
    pub gate: String,
    pub qubits: Vec<u32>,
    pub fidelity: Option<f64>,
    pub duration: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationSetInfo {
    pub set_id: String,
    pub timestamp: String,
    pub description: Option<String>,
}
```

### Error Mitigation Extension

For backends supporting built-in error mitigation.

```rust
/// Error mitigation extension.
#[async_trait]
pub trait HalErrorMitigation: HalCore {
    /// Get available error mitigation strategies.
    async fn mitigation_strategies(&self) -> HalResult<Vec<MitigationStrategy>>;

    /// Submit with error mitigation.
    async fn submit_mitigated(
        &self,
        request: SubmitRequest,
        strategy: &str,
    ) -> HalResult<SubmitResponse>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MitigationStrategy {
    pub name: String,
    pub description: String,
    /// Overhead factor (e.g., 2.0 means 2x more shots needed).
    pub overhead: f64,
}
```

---

## REST API Specification

For backends that prefer HTTP/REST over native Rust integration.

### OpenAPI 3.1 Specification

```yaml
openapi: 3.1.0
info:
  title: HIQ HAL Contract
  version: "1.0.0"
  description: |
    Hardware Abstraction Layer contract for quantum backends.

    All compliant backends MUST implement Layer 1 (Core) and Layer 2 (Capabilities).
    Layer 3 (Extensions) is optional.

servers:
  - url: https://api.example.com/hal/v1
    description: Example backend

paths:
  # Layer 1: Core
  /info:
    get:
      operationId: getBackendInfo
      summary: Get backend information
      tags: [Core]
      responses:
        "200":
          description: Backend information
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/BackendInfo"

  /jobs:
    post:
      operationId: submitJob
      summary: Submit a quantum job
      tags: [Core]
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: "#/components/schemas/SubmitRequest"
      responses:
        "201":
          description: Job submitted
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/SubmitResponse"
        "400":
          $ref: "#/components/responses/BadRequest"
        "401":
          $ref: "#/components/responses/Unauthorized"

  /jobs/{jobId}:
    get:
      operationId: getJobStatus
      summary: Get job status
      tags: [Core]
      parameters:
        - $ref: "#/components/parameters/JobId"
      responses:
        "200":
          description: Job status
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/JobStatusResponse"
        "404":
          $ref: "#/components/responses/NotFound"

    delete:
      operationId: cancelJob
      summary: Cancel a job
      tags: [Core]
      parameters:
        - $ref: "#/components/parameters/JobId"
      responses:
        "202":
          description: Cancellation accepted
        "404":
          $ref: "#/components/responses/NotFound"

  /jobs/{jobId}/result:
    get:
      operationId: getJobResult
      summary: Get job result
      tags: [Core]
      parameters:
        - $ref: "#/components/parameters/JobId"
      responses:
        "200":
          description: Job result
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/JobResult"
        "404":
          $ref: "#/components/responses/NotFound"
        "409":
          description: Job not completed
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/HalError"

  # Layer 2: Capabilities
  /capabilities:
    get:
      operationId: getCapabilities
      summary: Get backend capabilities
      tags: [Capabilities]
      responses:
        "200":
          description: Backend capabilities
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/Capabilities"

  /availability:
    get:
      operationId: getAvailability
      summary: Get backend availability status
      tags: [Capabilities]
      responses:
        "200":
          description: Availability status
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/AvailabilityStatus"

  /validate:
    post:
      operationId: validateCircuit
      summary: Validate a circuit without executing
      tags: [Capabilities]
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: "#/components/schemas/CircuitPayload"
      responses:
        "200":
          description: Validation result
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/ValidationResult"

  # Layer 3: Extensions (Optional)
  /batch:
    post:
      operationId: submitBatch
      summary: Submit multiple circuits (Extension)
      tags: [Extensions]
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: array
              items:
                $ref: "#/components/schemas/SubmitRequest"
      responses:
        "201":
          description: Batch submitted
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/BatchResponse"

  /sessions:
    post:
      operationId: createSession
      summary: Create a session (Extension)
      tags: [Extensions]
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: "#/components/schemas/SessionConfig"
      responses:
        "201":
          description: Session created
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/SessionResponse"

  /calibration:
    get:
      operationId: getCalibration
      summary: Get calibration data (Extension)
      tags: [Extensions]
      responses:
        "200":
          description: Calibration data
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/CalibrationData"

components:
  parameters:
    JobId:
      name: jobId
      in: path
      required: true
      schema:
        type: string

  responses:
    BadRequest:
      description: Invalid request
      content:
        application/json:
          schema:
            $ref: "#/components/schemas/HalError"
    Unauthorized:
      description: Authentication failed
      content:
        application/json:
          schema:
            $ref: "#/components/schemas/HalError"
    NotFound:
      description: Resource not found
      content:
        application/json:
          schema:
            $ref: "#/components/schemas/HalError"

  schemas:
    BackendInfo:
      type: object
      required: [backend_id, contract_version]
      properties:
        backend_id:
          type: string
          example: "iqm-garnet"
        contract_version:
          type: string
          example: "1.0.0"
        vendor:
          type: string
          example: "IQM"
        description:
          type: string

    CircuitPayload:
      oneOf:
        - type: object
          required: [format, source]
          properties:
            format:
              const: "openqasm3"
            source:
              type: string
        - type: object
          required: [format, bitcode]
          properties:
            format:
              const: "qir"
            bitcode:
              type: string
              format: byte
        - type: object
          required: [format, data]
          properties:
            format:
              const: "native"
            data:
              type: object

    SubmitRequest:
      type: object
      required: [circuit, shots]
      properties:
        circuit:
          $ref: "#/components/schemas/CircuitPayload"
        shots:
          type: integer
          minimum: 1
          maximum: 100000
        seed:
          type: integer
        name:
          type: string
          maxLength: 256
        metadata:
          type: object

    SubmitResponse:
      type: object
      required: [job_id]
      properties:
        job_id:
          type: string
        estimated_queue_time:
          type: integer
          description: Estimated queue time in seconds

    JobStatusResponse:
      type: object
      required: [job_id, status]
      properties:
        job_id:
          type: string
        status:
          $ref: "#/components/schemas/JobStatus"

    JobStatus:
      oneOf:
        - type: object
          properties:
            status: { const: "queued" }
            position: { type: integer }
        - type: object
          properties:
            status: { const: "running" }
            progress: { type: number, minimum: 0, maximum: 1 }
        - type: object
          properties:
            status: { const: "completed" }
        - type: object
          required: [status, error]
          properties:
            status: { const: "failed" }
            error: { type: string }
            code: { type: string }
        - type: object
          properties:
            status: { const: "cancelled" }

    JobResult:
      type: object
      required: [job_id, counts, shots]
      properties:
        job_id:
          type: string
        counts:
          type: object
          additionalProperties:
            type: integer
          example: { "00": 512, "11": 488 }
        shots:
          type: integer
        execution_time_ms:
          type: integer
        metadata:
          type: object

    Capabilities:
      type: object
      required: [backend_id, vendor, simulator, topology, gates, constraints]
      properties:
        backend_id:
          type: string
        vendor:
          type: string
        description:
          type: string
        simulator:
          type: boolean
        topology:
          $ref: "#/components/schemas/Topology"
        gates:
          type: array
          items:
            $ref: "#/components/schemas/GateSpec"
        constraints:
          $ref: "#/components/schemas/Constraints"
        extensions:
          type: array
          items:
            type: string
        last_calibrated:
          type: string
          format: date-time

    Topology:
      type: object
      required: [num_qubits, edges]
      properties:
        num_qubits:
          type: integer
        edges:
          type: array
          items:
            $ref: "#/components/schemas/CouplingEdge"
        topology_type:
          type: string
          enum: [linear, star, grid, heavy_hex, fully_connected, custom]

    CouplingEdge:
      type: object
      required: [source, target]
      properties:
        source:
          type: integer
        target:
          type: integer
        bidirectional:
          type: boolean
          default: true
        fidelity:
          type: number
          minimum: 0
          maximum: 1

    GateSpec:
      type: object
      required: [name, num_qubits, num_params, native]
      properties:
        name:
          type: string
        num_qubits:
          type: integer
        num_params:
          type: integer
        native:
          type: boolean
        description:
          type: string

    Constraints:
      type: object
      required: [max_shots, max_qubits, supported_formats]
      properties:
        max_shots:
          type: integer
        max_depth:
          type: integer
        max_gates:
          type: integer
        max_qubits:
          type: integer
        supported_formats:
          type: array
          items:
            type: string
            enum: [openqasm3, qir, native]

    AvailabilityStatus:
      type: object
      required: [available]
      properties:
        available:
          type: boolean
        queue_depth:
          type: integer
        estimated_wait:
          type: integer
        message:
          type: string
        available_at:
          type: string
          format: date-time

    ValidationResult:
      type: object
      required: [valid]
      properties:
        valid:
          type: boolean
        errors:
          type: array
          items:
            $ref: "#/components/schemas/ValidationError"
        warnings:
          type: array
          items:
            $ref: "#/components/schemas/ValidationWarning"

    ValidationError:
      type: object
      required: [code, message]
      properties:
        code:
          type: string
        message:
          type: string
        location:
          type: string

    ValidationWarning:
      type: object
      required: [code, message]
      properties:
        code:
          type: string
        message:
          type: string
        location:
          type: string

    BatchResponse:
      type: object
      required: [batch_id, job_ids]
      properties:
        batch_id:
          type: string
        job_ids:
          type: array
          items:
            type: string

    SessionConfig:
      type: object
      required: [max_duration]
      properties:
        max_duration:
          type: integer
          description: Maximum session duration in seconds
        max_jobs:
          type: integer

    SessionResponse:
      type: object
      required: [session_id]
      properties:
        session_id:
          type: string
        expires_at:
          type: string
          format: date-time

    CalibrationData:
      type: object
      required: [set_id, timestamp]
      properties:
        set_id:
          type: string
        timestamp:
          type: string
          format: date-time
        qubit_properties:
          type: array
          items:
            $ref: "#/components/schemas/QubitCalibration"
        gate_properties:
          type: array
          items:
            $ref: "#/components/schemas/GateCalibration"

    QubitCalibration:
      type: object
      required: [qubit]
      properties:
        qubit:
          type: integer
        t1:
          type: number
        t2:
          type: number
        readout_fidelity:
          type: number
        frequency:
          type: number

    GateCalibration:
      type: object
      required: [gate, qubits]
      properties:
        gate:
          type: string
        qubits:
          type: array
          items:
            type: integer
        fidelity:
          type: number
        duration:
          type: number

    HalError:
      type: object
      required: [code, message]
      properties:
        code:
          type: string
          enum:
            - AUTHENTICATION_FAILED
            - AUTHORIZATION_FAILED
            - INVALID_CIRCUIT
            - CIRCUIT_TOO_LARGE
            - UNSUPPORTED_FEATURE
            - JOB_NOT_FOUND
            - JOB_FAILED
            - BACKEND_UNAVAILABLE
            - RATE_LIMITED
            - INTERNAL_ERROR
        message:
          type: string
        details:
          type: object

  securitySchemes:
    bearerAuth:
      type: http
      scheme: bearer
    apiKey:
      type: apiKey
      in: header
      name: X-API-Key

security:
  - bearerAuth: []
  - apiKey: []
```

---

## Contract Versioning

### Semantic Versioning

The contract follows semantic versioning:

```
MAJOR.MINOR.PATCH

- MAJOR: Breaking changes (incompatible API changes)
- MINOR: New features (backwards compatible)
- PATCH: Bug fixes (backwards compatible)
```

### Version Negotiation

```rust
/// Version negotiation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Supported contract versions (newest first).
    pub supported: Vec<String>,
    /// Preferred version.
    pub preferred: String,
}

/// Backends report supported versions.
pub trait HalVersioned {
    /// Get supported contract versions.
    fn version_info(&self) -> VersionInfo;

    /// Check if a specific version is supported.
    fn supports_version(&self, version: &str) -> bool;
}
```

### Deprecation Policy

1. **Announce** deprecation in MINOR release (documentation + response headers)
2. **Support** deprecated features for at least 2 MINOR releases
3. **Remove** in next MAJOR release

### Response Headers

```
X-HAL-Contract-Version: 1.0.0
X-HAL-Deprecated: feature_name; sunset=2025-06-01
```

---

## Compliance Testing

### Test Suite

HIQ provides a compliance test suite that backend implementers can run to verify their implementation.

```rust
/// Compliance test for HAL implementations.
pub struct HalComplianceTest<B: HalCore + HalCapabilities> {
    backend: B,
}

impl<B: HalCore + HalCapabilities> HalComplianceTest<B> {
    /// Run all compliance tests.
    pub async fn run_all(&self) -> ComplianceReport {
        let mut report = ComplianceReport::new();

        // Core tests
        report.add(self.test_contract_version().await);
        report.add(self.test_backend_id().await);
        report.add(self.test_submit_simple_circuit().await);
        report.add(self.test_status_transitions().await);
        report.add(self.test_result_format().await);
        report.add(self.test_cancel().await);
        report.add(self.test_error_codes().await);

        // Capabilities tests
        report.add(self.test_capabilities_format().await);
        report.add(self.test_topology_consistency().await);
        report.add(self.test_gate_validation().await);
        report.add(self.test_constraints_enforcement().await);

        report
    }

    async fn test_contract_version(&self) -> TestResult {
        let version = self.backend.contract_version();
        TestResult {
            name: "contract_version".into(),
            passed: semver::Version::parse(version).is_ok(),
            message: format!("Version: {}", version),
        }
    }

    async fn test_submit_simple_circuit(&self) -> TestResult {
        let circuit = CircuitPayload::OpenQasm3 {
            source: r#"
                OPENQASM 3.0;
                qubit[2] q;
                bit[2] c;
                h q[0];
                cx q[0], q[1];
                c = measure q;
            "#.into(),
        };

        let request = SubmitRequest {
            circuit,
            shots: 100,
            seed: Some(42),
            name: Some("compliance_test".into()),
            metadata: None,
        };

        match self.backend.submit(request).await {
            Ok(response) => TestResult {
                name: "submit_simple_circuit".into(),
                passed: !response.job_id.0.is_empty(),
                message: format!("Job ID: {}", response.job_id.0),
            },
            Err(e) => TestResult {
                name: "submit_simple_circuit".into(),
                passed: false,
                message: format!("Error: {}", e.message),
            },
        }
    }

    async fn test_status_transitions(&self) -> TestResult {
        // Submit a job and verify status transitions
        // Queued -> Running -> Completed (or Failed)
        // ...
    }

    async fn test_capabilities_format(&self) -> TestResult {
        match self.backend.capabilities().await {
            Ok(caps) => {
                let valid = !caps.backend_id.is_empty()
                    && !caps.vendor.is_empty()
                    && caps.topology.num_qubits > 0
                    && !caps.gates.is_empty();
                TestResult {
                    name: "capabilities_format".into(),
                    passed: valid,
                    message: format!("{} qubits, {} gates",
                        caps.topology.num_qubits,
                        caps.gates.len()),
                }
            }
            Err(e) => TestResult {
                name: "capabilities_format".into(),
                passed: false,
                message: format!("Error: {}", e.message),
            },
        }
    }

    // ... more tests
}

#[derive(Debug)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

#[derive(Debug)]
pub struct ComplianceReport {
    pub results: Vec<TestResult>,
    pub passed: usize,
    pub failed: usize,
    pub timestamp: String,
    pub contract_version: String,
}

impl ComplianceReport {
    pub fn is_compliant(&self) -> bool {
        self.failed == 0
    }
}
```

### Running Compliance Tests

```bash
# Test a REST backend
hiq compliance test --endpoint https://api.backend.com/hal/v1

# Test with authentication
hiq compliance test --endpoint https://api.backend.com/hal/v1 --token $TOKEN

# Generate report
hiq compliance test --endpoint https://api.backend.com/hal/v1 --output report.json
```

---

## Implementation Guide

### Minimal Compliant Backend Checklist

- [ ] Implement `HalCore` trait (or REST endpoints)
- [ ] Implement `HalCapabilities` trait
- [ ] Return valid semantic version from `contract_version()`
- [ ] Support `openqasm3` circuit format
- [ ] Handle all standard error codes appropriately
- [ ] Return valid `Capabilities` with accurate topology and gates
- [ ] Pass compliance test suite

### Full-Featured Backend Checklist

- [ ] All minimal requirements
- [ ] Support `qir` circuit format
- [ ] Implement `HalBatch` extension
- [ ] Implement `HalSessions` extension
- [ ] Implement `HalCalibration` extension
- [ ] Provide accurate queue depth and wait time estimates
- [ ] Support circuit validation endpoint

### Example Implementation (Rust)

```rust
use hiq_hal_contract::*;

pub struct MyBackend {
    config: BackendConfig,
    client: reqwest::Client,
}

#[async_trait]
impl HalCore for MyBackend {
    fn backend_id(&self) -> &str {
        "my-backend-v1"
    }

    async fn submit(&self, request: SubmitRequest) -> HalResult<SubmitResponse> {
        // Validate circuit
        if let CircuitPayload::OpenQasm3 { source } = &request.circuit {
            // Parse and validate QASM3
        }

        // Submit to hardware/simulator
        let job_id = self.internal_submit(&request).await?;

        Ok(SubmitResponse {
            job_id: JobId(job_id),
            estimated_queue_time: Some(30),
        })
    }

    async fn status(&self, job_id: &JobId) -> HalResult<JobStatus> {
        let internal_status = self.internal_status(&job_id.0).await?;

        Ok(match internal_status {
            InternalStatus::Pending => JobStatus::Queued { position: Some(5) },
            InternalStatus::Running => JobStatus::Running { progress: Some(0.5) },
            InternalStatus::Done => JobStatus::Completed,
            InternalStatus::Error(msg) => JobStatus::Failed {
                error: msg,
                code: Some("EXECUTION_ERROR".into()),
            },
        })
    }

    async fn result(&self, job_id: &JobId) -> HalResult<JobResult> {
        let status = self.status(job_id).await?;

        if !matches!(status, JobStatus::Completed) {
            return Err(HalError {
                code: "JOB_NOT_COMPLETED".into(),
                message: "Job has not completed yet".into(),
                details: None,
            });
        }

        let internal_result = self.internal_result(&job_id.0).await?;

        Ok(JobResult {
            job_id: job_id.clone(),
            counts: Counts(internal_result.measurements),
            shots: internal_result.shots,
            execution_time_ms: Some(internal_result.duration_ms),
            metadata: None,
        })
    }

    async fn cancel(&self, job_id: &JobId) -> HalResult<()> {
        self.internal_cancel(&job_id.0).await
    }
}

#[async_trait]
impl HalCapabilities for MyBackend {
    async fn capabilities(&self) -> HalResult<Capabilities> {
        Ok(Capabilities {
            backend_id: self.backend_id().into(),
            vendor: "My Company".into(),
            description: Some("My quantum backend".into()),
            simulator: false,
            topology: Topology {
                num_qubits: 5,
                edges: vec![
                    CouplingEdge { source: 0, target: 1, bidirectional: true, fidelity: Some(0.99) },
                    CouplingEdge { source: 0, target: 2, bidirectional: true, fidelity: Some(0.98) },
                    // ...
                ],
                topology_type: Some(TopologyType::Star),
            },
            gates: vec![
                GateSpec { name: "prx".into(), num_qubits: 1, num_params: 2, native: true, description: None },
                GateSpec { name: "cz".into(), num_qubits: 2, num_params: 0, native: true, description: None },
            ],
            constraints: Constraints {
                max_shots: 10000,
                max_depth: Some(100),
                max_gates: Some(1000),
                max_qubits: 5,
                supported_formats: vec!["openqasm3".into()],
            },
            extensions: vec![],
            last_calibrated: Some("2025-01-15T10:30:00Z".into()),
        })
    }

    async fn availability(&self) -> HalResult<AvailabilityStatus> {
        Ok(AvailabilityStatus {
            available: true,
            queue_depth: Some(3),
            estimated_wait: Some(120),
            message: None,
            available_at: None,
        })
    }

    async fn validate(&self, circuit: &CircuitPayload) -> HalResult<ValidationResult> {
        // Validate circuit against capabilities
        // ...
        Ok(ValidationResult {
            valid: true,
            errors: vec![],
            warnings: vec![],
        })
    }
}
```

---

## Summary

| Layer | Purpose | Required | Endpoints/Methods |
|-------|---------|----------|-------------------|
| **Core** | Job lifecycle | Yes | submit, status, result, cancel |
| **Capabilities** | Device introspection | Yes | capabilities, availability, validate |
| **Extensions** | Advanced features | No | batch, sessions, calibration, etc. |

The HAL Contract enables:

1. **Interoperability** — Any backend implementing the contract works with HIQ
2. **Stability** — Versioned contracts with clear deprecation policy
3. **Flexibility** — Optional extensions for advanced features
4. **Testability** — Compliance test suite validates implementations
5. **Language Independence** — Rust traits or REST API
