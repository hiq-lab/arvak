# arvak-adapter-qdmi

Arvak backend adapter for [QDMI](https://github.com/Munich-Quantum-Software-Stack/QDMI) (Quantum Device Management Interface), the standardized C-based interface for quantum device access developed as part of the [Munich Quantum Software Stack (MQSS)](https://www.lrz.de/services/compute/quantum/).

This adapter enables Arvak to submit circuits to any QDMI-compliant device, providing access to quantum hardware at European HPC centers such as LRZ.

## Architecture

```
                        Arvak
  ┌──────────┐  ┌──────────────┐  ┌─────────────────────┐
  │ arvak-ir │ → │arvak-compile │ → │      arvak-hal      │
  │(Circuit) │  │  (Optimize)  │  │     (Backend)       │
  └──────────┘  └──────────────┘  └──────────┬──────────┘
                                             │
                  Backend Adapters           │
         ┌─────────┐ ┌─────────┐ ┌──────────┴──────────┐
         │   IQM   │ │   IBM   │ │ arvak-adapter-qdmi  │
         └─────────┘ └─────────┘ └──────────┬──────────┘
                                            │
                                            ▼
                            ┌───────────────────────────┐
                            │     QDMI (libqdmi.so)     │
                            │ Munich Quantum Software   │
                            │         Stack             │
                            └─────────────┬─────────────┘
                                          │
                ┌─────────────────────────┼─────────────────────────┐
                ▼                         ▼                         ▼
         ┌──────────────┐         ┌──────────────┐         ┌──────────────┐
         │ IQM Quantum  │         │   Rigetti    │         │    Other     │
         │   (Garnet)   │         │   (Aspen)    │         │   Backends   │
         └──────────────┘         └──────────────┘         └──────────────┘
```

## Quick Start

```rust
use arvak_adapter_qdmi::QdmiBackend;
use arvak_hal::Backend;
use arvak_ir::Circuit;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create QDMI backend with authentication
    let backend = QdmiBackend::new()
        .with_token("your-api-token")
        .with_base_url("https://qdmi.lrz.de");

    // Check availability
    if !backend.is_available().await? {
        eprintln!("QDMI device not available");
        return Ok(());
    }

    // Get device capabilities
    let caps = backend.capabilities().await?;
    println!("Device: {} with {} qubits", caps.name, caps.num_qubits);

    // Create a Bell state circuit
    let mut circuit = Circuit::with_size("bell", 2, 2);
    circuit.h(arvak_ir::QubitId(0))?;
    circuit.cx(arvak_ir::QubitId(0), arvak_ir::QubitId(1))?;
    circuit.measure_all();

    // Submit and wait for results
    let job_id = backend.submit(&circuit, 1000).await?;
    let result = backend.wait(&job_id).await?;

    // Print results
    println!("Results ({} shots):", result.shots);
    for (bitstring, count) in result.counts.sorted() {
        println!("  {} : {}", bitstring, count);
    }

    Ok(())
}
```

## Feature Flags

| Feature | Description |
|---------|-------------|
| *(default)* | Mock mode - runs without QDMI library, for development and testing |
| `system-qdmi` | Links against the system QDMI library (`libqdmi.so`) for real hardware access |

```toml
# Mock mode (default) - no external dependencies
arvak-adapter-qdmi = { path = "adapters/arvak-adapter-qdmi" }

# Real hardware access
arvak-adapter-qdmi = { path = "adapters/arvak-adapter-qdmi", features = ["system-qdmi"] }
```

## API Reference

### `QdmiBackend`

The main entry point. Implements the `arvak_hal::Backend` trait.

#### Construction

```rust
let backend = QdmiBackend::new()
    .with_token("your-api-token")       // Token or OIDC authentication
    .with_base_url("https://qdmi.lrz.de"); // QDMI endpoint URL
```

#### Backend Trait Methods

| Method | Description |
|--------|-------------|
| `name()` | Returns `"qdmi"` |
| `capabilities()` | Queries device properties (qubits, gates, topology). Results are cached. |
| `is_available()` | Checks if the QDMI device is online (`Idle` or `Busy`) |
| `submit(circuit, shots)` | Submits an Arvak `Circuit` (auto-converted to OpenQASM 3.0) and returns a `JobId` |
| `status(job_id)` | Returns current `JobStatus` (Queued, Running, Completed, Failed, Cancelled) |
| `result(job_id)` | Returns `ExecutionResult` with measurement counts |
| `cancel(job_id)` | Cancels a queued or running job |

#### Factory

`QdmiBackend` also implements `BackendFactory` for config-driven creation:

```rust
use arvak_hal::backend::{BackendConfig, BackendFactory};

let config = BackendConfig::new("qdmi")
    .with_token("your-token")
    .with_endpoint("https://qdmi.lrz.de");

let backend = QdmiBackend::from_config(config)?;
```

### Error Handling

All operations return `HalResult<T>`. QDMI-specific errors are defined in `QdmiError`:

| Variant | Description |
|---------|-------------|
| `SessionNotInitialized` | QDMI session not yet initialized |
| `NoDevice` | No QDMI device available |
| `DeviceNotReady(msg)` | Device exists but is not ready |
| `JobNotFound(id)` | Job ID not found |
| `JobFailed(msg)` | Job execution failed |
| `Timeout(msg)` | Job timed out |
| `UnsupportedFormat(fmt)` | Program format not supported |
| `CircuitConversion(msg)` | Failed to convert circuit to QASM |
| `LibraryNotAvailable` | `system-qdmi` feature not enabled |

## QDMI Compatibility

Compatible with QDMI version 1.x.

| Feature | Status |
|---------|--------|
| OpenQASM 2.0 | Supported |
| OpenQASM 3.0 | Supported (preferred) |
| QIR Base Profile | Planned |
| Token Auth | Supported |
| OIDC Auth | Supported |
| Device Properties | Supported |
| Site Properties (T1/T2) | Supported |
| Operation Properties | Supported |

### Supported FFI Types

The adapter re-exports all QDMI FFI types for advanced usage:

- **Session**: `QdmiSessionParameter` (BaseUrl, Token, AuthFile, AuthUrl, Username, Password)
- **Device**: `QdmiDeviceProperty` (Name, Version, QubitsNum, CouplingMap, T1/T2, supported formats)
- **Jobs**: `QdmiJobParameter`, `QdmiJobStatus`, `QdmiJobResult`
- **Operations**: `QdmiOperationProperty` (fidelity, duration, qubit count)
- **Programs**: `QdmiProgramFormat` (OpenQASM 2.0/3.0, QIR, QPY, IQM JSON)

## Testing

The adapter includes built-in tests that run in mock mode (no QDMI library required):

```bash
cargo test -p arvak-adapter-qdmi
```

Tests cover:
- Backend creation and naming
- Device capabilities query
- Availability check
- Full submit-wait-result cycle (Bell state)
- Job cancellation

## Links

- [QDMI GitHub Repository](https://github.com/Munich-Quantum-Software-Stack/QDMI)
- [Munich Quantum Software Stack](https://www.lrz.de/services/compute/quantum/)
- [Arvak Project](https://github.com/hiq-lab/arvak)
