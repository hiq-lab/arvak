# arvak-adapter-qdmi

Arvak backend adapter for [QDMI](https://github.com/Munich-Quantum-Software-Stack/QDMI) (Quantum Device Management Interface), the standardized C API for quantum device access from the [Munich Quantum Software Stack (MQSS)](https://www.lrz.de/services/compute/quantum/).

Enables Arvak to submit circuits to any QDMI-compliant quantum device at European HPC centers (LRZ, JSC, etc.).

## Architecture

```
Arvak Compilation Stack
┌──────────┐  ┌──────────────┐  ┌───────────────────────┐
│ arvak-ir │→ │arvak-compile │→ │      arvak-hal        │
│(Circuit) │  │  (Optimize)  │  │  (Backend Trait)      │
└──────────┘  └──────────────┘  └───────────┬───────────┘
                                            │
              Backend Adapters              │
     ┌─────────┐ ┌─────────┐ ┌─────────────┴───────────┐
     │   IQM   │ │   IBM   │ │  arvak-adapter-qdmi     │
     └─────────┘ └─────────┘ └─────────────┬───────────┘
                                           │
                              ┌────────────▼────────────┐
                              │   QDMI C API (libqdmi)  │
                              │   Session → Device →    │
                              │   Job → Results         │
                              └────────────┬────────────┘
                                           │
              ┌────────────────────────────┼────────────────────────┐
              ▼                            ▼                        ▼
       ┌────────────┐             ┌────────────┐           ┌────────────┐
       │ IQM Garnet │             │  Neutral   │           │   Other    │
       │ (20 Qubit) │             │   Atoms    │           │  Backends  │
       └────────────┘             └────────────┘           └────────────┘
```

## Quick Start

```rust
use arvak_adapter_qdmi::QdmiBackend;
use arvak_hal::Backend;
use arvak_ir::{Circuit, QubitId};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let backend = QdmiBackend::new()
        .with_token("your-api-token")
        .with_base_url("https://qdmi.lrz.de");

    // Query device
    let caps = backend.capabilities().await?;
    println!("{}: {} qubits", caps.name, caps.num_qubits);

    // Submit Bell state
    let mut circuit = Circuit::with_size("bell", 2, 2);
    circuit.h(QubitId(0))?;
    circuit.cx(QubitId(0), QubitId(1))?;
    circuit.measure_all();

    let job_id = backend.submit(&circuit, 1000).await?;
    let result = backend.wait(&job_id).await?;

    for (bitstring, count) in result.counts.sorted() {
        println!("  {} : {}", bitstring, count);
    }
    Ok(())
}
```

## Feature Flags

| Feature | Description |
|---------|-------------|
| *(default)* | Mock mode -- no external dependencies, for development and testing |
| `system-qdmi` | Links against system `libqdmi.so` for real hardware access via FFI |

```toml
# Development / CI (mock mode)
arvak-adapter-qdmi = { path = "adapters/arvak-adapter-qdmi" }

# Production (real hardware)
arvak-adapter-qdmi = { path = "adapters/arvak-adapter-qdmi", features = ["system-qdmi"] }
```

## System QDMI Integration

When `system-qdmi` is enabled, the adapter calls QDMI C functions via FFI:

| Step | FFI Call | Description |
|------|----------|-------------|
| 1 | `QDMI_session_alloc` | Allocate session handle |
| 2 | `QDMI_session_set_parameter` | Set token, base URL |
| 3 | `QDMI_session_init` | Connect to backend |
| 4 | `QDMI_session_get_devices` | Discover devices |
| 5 | `QDMI_device_create_job` | Create job on device |
| 6 | `QDMI_job_set_parameter` | Set QASM3 program + shots |
| 7 | `QDMI_job_submit` | Submit to queue |
| 8 | `QDMI_job_check` | Poll job status |
| 9 | `QDMI_job_get_results` | Retrieve histogram |
| 10 | `QDMI_job_free` / `QDMI_session_free` | Cleanup (automatic via `Drop`) |

Resource cleanup is automatic: `QdmiBackend` implements `Drop` to free all FFI handles.

### Prerequisites

```bash
# QDMI library must be installed system-wide
export LD_LIBRARY_PATH=/path/to/qdmi/lib:$LD_LIBRARY_PATH
export PKG_CONFIG_PATH=/path/to/qdmi/lib/pkgconfig:$PKG_CONFIG_PATH
```

## API

### Construction

```rust
// Builder pattern
let backend = QdmiBackend::new()
    .with_token("token")
    .with_base_url("https://qdmi.lrz.de");

// Factory pattern
use arvak_hal::backend::{BackendConfig, BackendFactory};
let backend = QdmiBackend::from_config(
    BackendConfig::new("qdmi")
        .with_token("token")
        .with_endpoint("https://qdmi.lrz.de")
)?;
```

### Backend Trait

| Method | Description |
|--------|-------------|
| `capabilities()` | Device properties (cached). Returns qubits, gates, topology. |
| `is_available()` | `true` if device status is `Idle` or `Busy` |
| `submit(circuit, shots)` | Auto-converts to QASM3, returns `JobId` |
| `status(job_id)` | Maps QDMI status to `JobStatus` |
| `result(job_id)` | Returns `ExecutionResult` with histogram counts |
| `cancel(job_id)` | Cancel queued or running job |
| `wait(job_id)` | Poll until complete (inherited from `Backend`) |

### Error Handling

QDMI errors map to `HalError` via `From<QdmiError>`:

| QdmiError | HalError |
|-----------|----------|
| `NoDevice` | `BackendUnavailable` |
| `JobNotFound` | `JobNotFound` |
| `JobFailed` | `JobFailed` |
| `Timeout` | `Timeout` |
| Others | `Backend(msg)` |

## FFI Types

Re-exported for advanced usage:

- **Session**: `QdmiSessionParameter` (BaseUrl, Token, AuthFile, AuthUrl, Username, Password)
- **Device**: `QdmiDeviceProperty` (Name, QubitsNum, CouplingMap, Status, Sites, Operations)
- **Site**: `QdmiSiteProperty` (T1, T2, Coordinates, IsZone, ModuleIndex)
- **Operation**: `QdmiOperationProperty` (Fidelity, Duration, InteractionRadius)
- **Job**: `QdmiJobParameter`, `QdmiJobStatus`, `QdmiJobResult`
- **Program**: `QdmiProgramFormat` (Qasm2, Qasm3, QIR, QPY, IqmJson)

## QDMI Compatibility

| Feature | Status |
|---------|--------|
| OpenQASM 3.0 | Supported (default) |
| OpenQASM 2.0 | Supported |
| QIR Base Profile | Planned |
| Token / OIDC Auth | Supported |
| Device property queries | Supported |
| Site properties (T1/T2, coordinates) | Supported |
| Operation properties (fidelity) | Supported |
| Neutral-atom extensions (zones, shuttling) | Supported via FFI types |

## Testing

```bash
# Mock mode (no QDMI library required)
cargo test -p arvak-adapter-qdmi

# With system QDMI (requires libqdmi.so)
cargo test -p arvak-adapter-qdmi --features system-qdmi
```

## Links

- [QDMI Specification](https://github.com/Munich-Quantum-Software-Stack/QDMI)
- [Munich Quantum Software Stack](https://www.lrz.de/services/compute/quantum/)
- [Arvak Project](https://github.com/hiq-lab/arvak)
