# Arvak: Rust-Native Quantum Compilation Stack

[![Version](https://img.shields.io/badge/version-1.8.0-blue.svg)](https://github.com/hiq-lab/arvak/releases/tag/v1.8.0)
[![PyPI](https://img.shields.io/pypi/v/arvak.svg)](https://pypi.org/project/arvak/)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-Apache--2.0-green.svg)](LICENSE)

**HPC-Integrated Quantum Orchestration Platform with Deep Framework Integration**

Arvak is a Rust-native quantum compilation and orchestration stack designed for HPC environments. It provides blazing-fast compilation, first-class HPC scheduler integration, and **seamless interoperability** with the entire quantum ecosystem through deep framework integrations.

> **v1.8.0 Released!** Multi-backend hardware support: Scaleway/IQM (Garnet, Emerald, Sirius) and AWS Braket adapters, IBM Cloud API adapter, Nathan code anonymization for PII protection, 100+ security and correctness fixes from architectural audits. See [CHANGELOG.md](CHANGELOG.md).

## Quick Install

```bash
# Install from PyPI
pip install arvak

# With framework integrations
pip install arvak[qiskit]      # IBM Quantum ecosystem
pip install arvak[qrisp]       # High-level quantum programming
pip install arvak[cirq]        # Google Quantum AI
pip install arvak[pennylane]   # Quantum machine learning
pip install arvak[all]         # All frameworks + Jupyter notebooks
```

## Why Arvak?

Arvak is **not** a Qiskit/Cirq/Qrisp replacement. It's a **complementary platform** that:

1. **Integrates deeply** with existing quantum frameworks through auto-discovery plugin architecture
2. **Provides** Rust-native compilation for performance-critical HPC workflows
3. **Prioritizes** European HPC quantum installations (LUMI, LRZ) as first-class citizens
4. **Enables** seamless interoperability: use Qiskit/Cirq/Qrisp circuits with Arvak backends
5. **Offers** unified access to IQM, IBM Quantum, Scaleway QaaS, AWS Braket, NVIDIA CUDA-Q, and any QDMI-compliant device
6. **Supports** neutral-atom architectures with zone-aware routing and shuttling
7. **Includes** Nathan, an AI research optimizer grounded in 1,700+ quantum computing papers

## Architecture: Deep Modular Integration

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                    Quantum Framework Ecosystem                                   │
│         Qiskit  │  Qrisp  │  Cirq  │  PennyLane  │  Your Framework               │
└─────────────────────────────┬───────────────────────────────────────────────────┘
                              │
                              │ Plugin Architecture (Auto-Discovery)
                              │
┌─────────────────────────────▼───────────────────────────────────────────────────┐
│                        Arvak Integration Layer (Python)                          │
│  ┌─────────────────────────────────────────────────────────────────────────┐    │
│  │  FrameworkIntegration Registry  │  Bidirectional Converters             │    │
│  │  • Auto-discovery & registration │  • Framework → Arvak (via QASM3)     │    │
│  │  • Dependency checking           │  • Arvak → Framework (via QASM3)     │    │
│  │  • Status reporting              │  • Backend adapters (Qiskit, Cirq)   │    │
│  └─────────────────────────────────────────────────────────────────────────┘    │
│                                                                                   │
│  Interactive Notebooks: 5 Jupyter notebooks with live examples                   │
└─────────────────────────────┬───────────────────────────────────────────────────┘
                              │ PyO3 bindings (Rust ↔ Python)
┌─────────────────────────────▼───────────────────────────────────────────────────┐
│                           Arvak Core (Rust)                                      │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌───────────┐  ┌──────────┐   │
│  │  arvak-ir    │  │ arvak-compile│  │  arvak-hal   │  │ arvak-sched │  │ arvak-types│   │
│  │            │  │            │  │            │  │           │  │          │   │
│  │ Circuit IR │  │ Pass mgr   │  │ Backend    │  │ SLURM/PBS │  │ Quantum  │   │
│  │ QASM3      │  │ Optimizer  │  │ abstraction│  │ Workflows │  │ Types    │   │
│  └────────────┘  └────────────┘  └────────────┘  └───────────┘  └──────────┘   │
│                                                                                   │
│  ┌──────────────────────────────────────────────────────────────────────────┐   │
│  │                          arvak-dashboard (Web UI)                           │   │
│  │    Circuit Viz │ Compilation │ Job Monitoring │ Results │ D3.js Plots    │   │
│  └──────────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────┬───────────────────────────────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────────────────────────────┐
│                         Backend Adapters + Plugin System                         │
│  Simulator │ IQM │ IBM │ Scaleway │ Braket │ CUDA-Q │ QDMI │ Plugins            │
└──────────────────────────────────────────────────────────────────────────────────┘
```

**Key Architecture Principles:**
- **OpenQASM 3.0** as universal interchange format between all frameworks
- **Auto-discovery**: Frameworks register automatically when installed (no manual config)
- **Zero-dependency core**: Framework integrations are optional extras
- **Bidirectional**: Convert to/from any supported framework seamlessly
- **Extensible**: Add new frameworks in ~30 minutes with template system

## Nathan: AI Research Optimizer

Arvak includes **Nathan**, an AI-powered quantum research optimizer grounded in 1,700+ quantum computing papers. Nathan provides hardware-aware circuit analysis and optimization recommendations.

```python
import arvak

# Analyze a circuit for optimization opportunities
qasm = """
OPENQASM 3.0;
qubit[3] q;
h q[0];
cx q[0], q[1];
cx q[1], q[2];
"""

# Get hardware-aware analysis
result = arvak.nathan.analyze(qasm, backend="ibm")
print(result)  # Optimization suggestions grounded in literature

# Freeform Q&A about quantum computing
response = arvak.nathan.chat("What error mitigation techniques work best for GHZ states on noisy hardware?")
print(response)
```

**Features:**
- **1,700+ sources**: Grounded in peer-reviewed quantum computing literature
- **Hardware-aware**: Recommendations tailored to target backend (IBM, IQM, etc.)
- **Code anonymization**: Automatically strips PII before analysis
- **Jupyter integration**: Works seamlessly in notebook workflows
- **Available at**: [arvak.io/nathan](https://arvak.io/nathan)

## gRPC Service API

Arvak provides a **production-ready gRPC service** for remote quantum circuit execution with comprehensive client libraries, enabling language-agnostic access to quantum backends:

```bash
# Start the gRPC server
cargo run --release --bin arvak-grpc-server

# Server listens on 0.0.0.0:50051 by default
```

### Quick Start

```python
from arvak_grpc import ArvakClient

# Connect to server
client = ArvakClient("localhost:50051")

# Submit circuit
qasm = """
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
"""

job_id = client.submit_qasm(qasm, "simulator", shots=1000)

# Wait for results
result = client.wait_for_job(job_id)
print(f"Counts: {result.counts}")  # {'00': 502, '11': 498}

client.close()
```

### Core Features

**Server (Rust):**
- **10 gRPC RPCs**: SubmitJob, SubmitBatch, GetJobStatus, GetJobResult, CancelJob, ListBackends, GetBackendInfo, WatchJob, StreamResults, SubmitBatchStream
- **Non-blocking execution**: Jobs execute asynchronously, RPCs return immediately
- **Circuit format**: OpenQASM 3 (Arvak IR JSON planned)
- **Thread-safe**: Handles concurrent requests with `Arc<RwLock<>>`
- **Feature-gated backends**: Enable specific backends via Cargo features

**Python Client (v1.8.0):**
- **Synchronous & Async APIs**: Full sync and async/await support
- **Job Futures**: Promise-like interface with callbacks
- **Retry & Resilience**: Exponential backoff, circuit breaker
- **Batch Operations**: Concurrent batch execution with progress tracking
- **Data Export**: Arrow, Parquet, CSV, JSON with compression
- **Caching**: Multi-level caching (L1 memory + L2 disk)
- **Analysis Tools**: Statistical analysis, convergence detection, distribution comparison
- **DataFrame Integration**: Convert to pandas/polars with visualization

### Async/Await API

```python
from arvak_grpc import AsyncArvakClient
import asyncio

async def main():
    # Async client with connection pooling
    client = AsyncArvakClient("localhost:50051", pool_size=10)

    # Submit jobs concurrently
    job_ids = await asyncio.gather(*[
        client.submit_qasm(qasm, "simulator", shots=1000)
        for _ in range(10)
    ])

    # Wait for all results
    results = await asyncio.gather(*[
        client.wait_for_job(job_id)
        for job_id in job_ids
    ])

    await client.close()

asyncio.run(main())
```

### JobFuture Interface

```python
from arvak_grpc import ArvakClient, as_completed

client = ArvakClient("localhost:50051")

# Submit and get futures
futures = [
    client.submit_qasm_future(qasm, "simulator", shots=1000)
    for _ in range(5)
]

# Process as they complete
for future in as_completed(futures):
    result = future.result()
    print(f"Job {future.job_id}: {len(result.counts)} states")

# Or use callbacks
def on_complete(future):
    print(f"Job completed: {future.job_id}")

future = client.submit_qasm_future(qasm, "simulator", shots=1000)
future.add_done_callback(on_complete)
```

### Retry & Circuit Breaker

```python
from arvak_grpc import ResilientClient, RetryPolicy, CircuitBreakerConfig

# Configure resilience
retry_policy = RetryPolicy(
    max_attempts=3,
    initial_backoff=1.0,
    backoff_multiplier=2.0,
    strategy=RetryStrategy.EXPONENTIAL_BACKOFF
)

circuit_breaker = CircuitBreakerConfig(
    failure_threshold=5,
    reset_timeout=60.0
)

# Wrap client with resilience
client = ResilientClient(
    base_client,
    retry_policy=retry_policy,
    circuit_breaker=circuit_breaker
)

# Automatic retries on transient failures
result = client.submit_and_wait(qasm, "simulator", shots=1000)
```

### Batch Operations

```python
from arvak_grpc import BatchJobManager

client = ArvakClient("localhost:50051")
manager = BatchJobManager(client, max_workers=10)

# Submit batch with progress tracking
circuits = [(qasm, 1000) for _ in range(50)]

def progress_callback(progress):
    print(f"Progress: {progress.completed}/{progress.total} "
          f"({progress.success} success, {progress.failed} failed)")

result = manager.execute_batch(
    circuits,
    backend_id="simulator",
    progress_callback=progress_callback
)

print(f"Batch completed: {result.status}")
print(f"Total time: {result.total_time_seconds:.2f}s")
print(f"Throughput: {result.jobs_per_second:.1f} jobs/s")
```

### Data Export & Analysis

```python
from arvak_grpc import (
    ArvakClient,
    ResultExporter,
    to_pandas,
    StatisticalAnalyzer,
    ResultComparator,
    CachedClient,
    TwoLevelCache
)

client = ArvakClient("localhost:50051")

# Get results
job_id = client.submit_qasm(qasm, "simulator", shots=1000)
result = client.wait_for_job(job_id)

# Export to Parquet
ResultExporter.to_parquet(result, "result.parquet", compression='snappy')

# Convert to DataFrame
df = to_pandas(result)
print(df)

# Statistical analysis
entropy = StatisticalAnalyzer.entropy(result)
purity = StatisticalAnalyzer.purity(result)
fidelity = StatisticalAnalyzer.fidelity_estimate(result, ideal_state)

print(f"Entropy: {entropy:.4f} bits")
print(f"Purity: {purity:.6f}")
print(f"Fidelity: {fidelity:.6f}")

# Compare distributions
comparison = ResultComparator.compare(result1, result2)
print(f"TVD: {comparison.tvd:.6f}")
print(f"Overlap: {comparison.overlap:.6f}")

# Caching for performance
cached_client = CachedClient(
    client,
    cache=TwoLevelCache(memory_size=100, cache_dir=".cache")
)

# First call: from server (slow)
result = cached_client.get_job_result(job_id)

# Second call: from cache (fast!)
result = cached_client.get_job_result(job_id)

print(f"Cache hit rate: {cached_client.cache_stats()['l1']['hit_rate']:.2%}")
```

### Visualization

```python
from arvak_grpc import Visualizer, ConvergenceAnalyzer

# Plot measurement distribution
fig, axes = Visualizer.plot_distribution(result, max_states=20)
fig.savefig('distribution.png')

# Compare multiple runs
fig, ax = Visualizer.plot_comparison(
    results,
    labels=["Run 1", "Run 2", "Run 3"]
)
fig.savefig('comparison.png')

# Analyze convergence
analysis = ConvergenceAnalyzer.analyze_convergence(
    results_with_increasing_shots,
    target_state=ideal_state
)

print(f"Converged: {analysis.converged}")
print(f"Final entropy: {analysis.entropies[-1]:.4f}")
```

### Installation

```bash
# Basic client
pip install arvak-grpc

# With export support (Arrow/Parquet)
pip install arvak-grpc[export]

# With DataFrame support
pip install arvak-grpc[polars]

# With visualization
pip install arvak-grpc[viz]

# Everything
pip install arvak-grpc[all]
```

### Python API Summary

**Core Clients:**
- `ArvakClient` - Synchronous blocking client
- `AsyncArvakClient` - Async/await client with connection pooling
- `ResilientClient` - Client with retry and circuit breaker
- `CachedClient` - Client with transparent caching

**Job Management:**
- `JobFuture` - Promise-like interface for jobs
- `BatchJobManager` - Concurrent batch execution
- `as_completed()`, `wait()` - Future coordination

**Data Export:**
- `ResultExporter` - Export to Arrow, Parquet, CSV, JSON
- `BatchExporter` - Incremental batch export
- `get_parquet_metadata()` - Inspect Parquet files

**DataFrame Integration:**
- `to_pandas()`, `to_polars()` - Convert to DataFrames
- `DataFrameConverter` - Advanced conversion options
- `StatisticalAnalyzer` - Entropy, purity, fidelity, TVD
- `Visualizer` - Distribution plots, comparisons, statistics tables

**Caching:**
- `MemoryCache` - LRU cache with TTL
- `DiskCache` - Persistent cache (JSON/Parquet)
- `TwoLevelCache` - L1 memory + L2 disk

**Analysis:**
- `ResultAggregator` - Combine, average, filter results
- `ResultComparator` - Compare distributions (TVD, KL, JS, Hellinger)
- `ConvergenceAnalyzer` - Analyze convergence, estimate required shots
- `ResultTransformer` - Normalize, downsample, add noise

See [`crates/arvak-grpc/README.md`](crates/arvak-grpc/README.md) for complete documentation.

## Framework Integrations

Arvak provides **deep, bidirectional integration** with major quantum frameworks:

| Framework | Status | Use Arvak As... | Convert Circuits | Python Package |
|-----------|--------|-----------------|------------------|----------------|
| **Qiskit** | ✅ | Backend (BackendV2) | ✅ To/From | `arvak[qiskit]` |
| **Qrisp** | ✅ | Backend Client | ✅ To/From | `arvak[qrisp]` |
| **Cirq** | ✅ | Sampler/Engine | ✅ To/From | `arvak[cirq]` |
| **PennyLane** | ✅ | Device | ✅ To/From | `arvak[pennylane]` |

### Quick Integration Examples

**Check Available Integrations:**
```python
import arvak

# List all integrations and availability
print(arvak.list_integrations())
# {'qiskit': True, 'qrisp': True, 'cirq': False, 'pennylane': True}

# Get detailed status
status = arvak.integration_status()
print(status['qiskit'])
# {'name': 'qiskit', 'available': True, 'packages': ['qiskit>=1.0.0', ...]}
```

**Use Arvak as Qiskit Backend:**
```python
from qiskit import QuantumCircuit
from arvak.integrations.qiskit import ArvakProvider

# Create circuit in Qiskit
qc = QuantumCircuit(2, 2)
qc.h(0)
qc.cx(0, 1)
qc.measure_all()

# Run on Arvak backends (sim, iqm, ibm)
provider = ArvakProvider()
backend = provider.get_backend('sim')
job = backend.run(qc, shots=1000)
result = job.result()
print(result.get_counts())
```

**Convert Between Frameworks:**
```python
import arvak

# Qiskit → Arvak → Cirq
qiskit_int = arvak.get_integration('qiskit')
cirq_int = arvak.get_integration('cirq')

arvak_circuit = qiskit_int.to_arvak(qiskit_circuit)
cirq_circuit = cirq_int.from_arvak(arvak_circuit)
```

**Use Arvak as PennyLane Device:**
```python
import pennylane as qml
from arvak.integrations.pennylane import ArvakDevice

dev = ArvakDevice(wires=2, backend='sim', shots=1000)

@qml.qnode(dev)
def circuit(x):
    qml.RX(x, wires=0)
    qml.CNOT(wires=[0, 1])
    return qml.expval(qml.PauliZ(0))

result = circuit(0.5)  # Runs on Arvak backend
```

### Interactive Jupyter Notebooks

Explore integrations hands-on with **5 interactive notebooks**:

```bash
# Install with notebook support
pip install arvak[all]  # Includes jupyter + matplotlib

# Launch notebooks
jupyter notebook crates/arvak-python/notebooks/
```

**Available Notebooks:**
1. `01_core_arvak.ipynb` - Core Arvak functionality and basics
2. `02_qiskit_integration.ipynb` - Qiskit backend usage, circuit conversion
3. `03_qrisp_integration.ipynb` - High-level quantum types with Arvak backend
4. `04_cirq_integration.ipynb` - Cirq sampler, NISQ algorithms
5. `05_pennylane_integration.ipynb` - QML workflows with automatic differentiation

### Adding Your Framework

Arvak's plugin architecture makes adding frameworks straightforward:

1. Create integration module in `python/arvak/integrations/<framework>/`
2. Implement `FrameworkIntegration` base class (3 methods)
3. Add to `pyproject.toml` optional dependencies
4. Framework auto-registers when package installed

See [docs/INTEGRATION_GUIDE.md](docs/INTEGRATION_GUIDE.md) for the complete guide. Most integrations take ~30 minutes.

## Project Structure

```
arvak/
├── crates/
│   ├── arvak-ir/          # Circuit intermediate representation
│   ├── arvak-qasm3/       # OpenQASM 3.0 parser and emitter
│   ├── arvak-compile/     # Compilation pass manager
│   ├── arvak-hal/         # Hardware abstraction layer
│   ├── arvak-cli/         # Command-line interface
│   ├── arvak-grpc/        # gRPC service (Rust server)
│   ├── arvak-python/      # Python bindings (PyO3) + integrations
│   │   ├── python/arvak/              # Python package
│   │   ├── python/arvak/integrations/ # Framework integrations
│   │   ├── notebooks/                 # 5 Jupyter notebooks
│   │   └── docs/                      # Integration guides
│   ├── arvak-eval/        # Evaluator: compilation observability, QDMI contracts, emitter compliance
│   ├── arvak-sched/       # HPC job scheduler (SLURM, PBS, workflows, routing)
│   ├── arvak-dashboard/   # Web dashboard for visualization & monitoring
│   ├── arvak-bench/       # Benchmark suite (QV, CLOPS, Randomized Benchmarking)
│   ├── arvak-types/       # Qrisp-like quantum types (QuantumInt, QuantumFloat)
│   └── arvak-auto/        # Automatic uncomputation
├── grpc-client/
│   └── arvak_grpc/        # gRPC Python client (v1.8.0)
│       ├── client.py               # Sync client
│       ├── async_client.py         # Async client with connection pooling
│       ├── job_future.py           # Promise-like job interface
│       ├── retry_policy.py         # Retry & circuit breaker
│       ├── batch_manager.py        # Concurrent batch operations
│       ├── result_export.py        # Arrow/Parquet/CSV/JSON export
│       ├── result_cache.py         # Multi-level caching
│       ├── result_analysis.py      # Advanced analysis tools
│       ├── dataframe_integration.py # Pandas/Polars integration
│       ├── examples/               # 4 comprehensive examples
│       └── tests/                  # 70 tests (56 passing)
├── adapters/
│   ├── arvak-adapter-sim/      # Local statevector simulator
│   ├── arvak-adapter-iqm/      # IQM Resonance API adapter
│   ├── arvak-adapter-ibm/      # IBM Quantum API adapter
│   ├── arvak-adapter-scaleway/ # Scaleway QaaS adapter (IQM Garnet, Emerald, Sirius)
│   ├── arvak-adapter-braket/   # AWS Braket adapter
│   ├── arvak-adapter-cudaq/    # NVIDIA CUDA-Q adapter (GPU-accelerated)
│   └── arvak-adapter-qdmi/     # QDMI (Munich Quantum Software Stack) adapter
├── demos/               # Demo applications
│   ├── bin/             # Grover, VQE, QAOA, QI-Nutshell, speed benchmarks
│   ├── src/             # Shared circuits, problems, runners
│   └── lumi-hybrid/     # LUMI quantum-HPC hybrid VQE demo
└── examples/            # Example QASM circuits
```

## Python API

### Basic Usage

```python
import arvak

# Build a Bell state circuit
circuit = arvak.Circuit(2)
circuit.h(0)
circuit.cx(0, 1)
circuit.measure_all()

print(f"Qubits: {circuit.num_qubits}, Depth: {circuit.depth()}")

# Export to QASM3
qasm = arvak.to_qasm(circuit)
print(qasm)

# Compile for target hardware
compiled = arvak.compile_circuit(
    circuit,
    target="iqm",
    optimization_level=2
)
```

### Rust API

```rust
use arvak_ir::Circuit;
use arvak_qasm3::{parse, emit};
use arvak_adapter_sim::SimulatorBackend;
use arvak_hal::Backend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse QASM3
    let source = r#"
        OPENQASM 3.0;
        qubit[2] q;
        bit[2] c;
        h q[0];
        cx q[0], q[1];
        c = measure q;
    "#;

    let circuit = parse(source)?;
    println!("Parsed: {} qubits, depth {}", circuit.num_qubits(), circuit.depth());

    // Run on simulator
    let backend = SimulatorBackend::new();
    let job_id = backend.submit(&circuit, 1000).await?;
    let result = backend.wait(&job_id).await?;

    println!("Results: {:?}", result.counts);

    // Emit back to QASM3
    let qasm_out = emit(&circuit)?;
    println!("{}", qasm_out);

    Ok(())
}
```

### CLI Usage

```bash
# Install Rust CLI
cargo install --path crates/arvak-cli

# List available backends
arvak backends

# Run a circuit on the simulator
arvak run --input examples/bell.qasm --backend sim --shots 1000

# Compile a circuit for IQM hardware
arvak compile --input examples/bell.qasm --target iqm --output bell_compiled.qasm

# Run on IQM hardware (requires IQM_TOKEN)
export IQM_TOKEN="your-api-token"
arvak run --input examples/bell.qasm --backend iqm --shots 1000

# Run on IBM Quantum (requires IBM_QUANTUM_TOKEN)
export IBM_QUANTUM_TOKEN="your-api-token"
arvak run --input examples/bell.qasm --backend ibm --shots 1000

# Run on Scaleway QaaS / IQM (requires Scaleway credentials)
export SCALEWAY_SECRET_KEY="your-secret-key"
export SCALEWAY_PROJECT_ID="your-project-id"
arvak run --input examples/bell.qasm --backend scaleway --shots 1000

# Evaluate a circuit (compilation observability + QDMI contract check)
arvak eval --input examples/bell.qasm --target iqm

# Evaluate with orchestration analysis and emitter compliance
arvak eval --input examples/bell.qasm --target iqm --orchestration --emit iqm --scheduler-site lrz

# Evaluate with a benchmark workload
arvak eval --input examples/bell.qasm --target simulator --benchmark ghz --benchmark-qubits 8
```

## Building from Source

### Prerequisites

- **Rust 1.85+** (install via [rustup](https://rustup.rs/))
- **Protocol Buffers compiler** (required for gRPC):
  ```bash
  # Linux (Debian/Ubuntu)
  sudo apt-get install protobuf-compiler

  # macOS
  brew install protobuf

  # Verify installation
  protoc --version  # should show libprotoc 3.x or later
  ```

### Build Steps

```bash
# Clone repository
git clone https://github.com/hiq-lab/arvak.git
cd arvak

# Build all crates
cargo build --release

# Install CLI
cargo install --path crates/arvak-cli

# Build Python package
cd crates/arvak-python
pip install maturin
maturin develop --release

# Run tests
cargo test
```

## Backend Support

| Backend | Status | Auth | Notes |
|---------|--------|------|-------|
| Simulator | ✅ | None | Local statevector, up to ~20 qubits |
| IQM Resonance | ✅ | `IQM_TOKEN` | Cloud API |
| IBM Quantum | ✅ | `IBM_QUANTUM_TOKEN` | Cloud API (Qiskit Runtime) |
| Scaleway QaaS | ✅ | `SCALEWAY_SECRET_KEY` | IQM Garnet (20q), Emerald (54q), Sirius (16q) |
| AWS Braket | ✅ | AWS credentials | IonQ, Rigetti, IQM, simulators |
| NVIDIA CUDA-Q | ⚠️ Library-only | `CUDAQ_API_TOKEN` | GPU-accelerated simulation (mqpu, custatevec, tensornet) |
| IQM LUMI | ✅ | OIDC | On-premise (CSC Finland) |
| IQM LRZ | ✅ | OIDC | On-premise (Germany) |
| QDMI (MQSS) | ⚠️ Library-only | Token/OIDC | Any QDMI-compliant device |
| Dynamic Plugins | ⚠️ Library-only | Varies | Load custom backends via `$ARVAK_PLUGIN_DIR` |

## Compilation Targets

| Target | Basis Gates | Topology |
|--------|-------------|----------|
| `iqm`, `iqm5` | PRX, CZ | Star (5 qubits) |
| `iqm20` | PRX, CZ | Star (20 qubits) |
| `ibm`, `ibm5` | RZ, SX, X, CX | Linear (5 qubits) |
| `ibm27` | RZ, SX, X, CX | Linear (27 qubits) |
| `ibm133` | RZ, SX, X, CX | Heavy-hex (133 qubits, Heron) |
| `scaleway-garnet` | PRX, CZ | IQM Garnet (20 qubits) |
| `scaleway-emerald` | PRX, CZ | IQM Emerald (54 qubits) |
| `scaleway-sirius` | PRX, CZ | IQM Sirius (16 qubits) |
| `simulator` | Universal | Full connectivity |

## HPC Deployment

Arvak provides first-class support for HPC environments with both SLURM and PBS schedulers.

### LUMI (CSC, Finland)

```yaml
# ~/.arvak/config.yaml
site: lumi
scheduler:
  type: slurm
  partition: q_fiqci
  account: project_462000xxx

backend:
  type: iqm
  endpoint: https://qpu.lumi.csc.fi
  auth_method: oidc
```

```bash
# Submit job to LUMI (authenticate via OIDC config)
arvak run --input circuit.qasm --backend iqm --shots 1000
```

### Scheduler Support

| Scheduler | Commands | Features |
|-----------|----------|----------|
| SLURM | sbatch, squeue, sacct, scancel | QOS mapping, array jobs |
| PBS/Torque | qsub, qstat, qdel, qhold, qrls | Array jobs, job holds |

## Demo Applications

### Quantum Algorithms

```bash
# Run all demos
cargo run --bin demo-all

# Run specific algorithms
cargo run --bin demo-grover        # Grover's search algorithm
cargo run --bin demo-vqe           # Variational Quantum Eigensolver
cargo run --bin demo-qaoa          # Quantum Approximate Optimization
cargo run --bin demo-qi-nutshell   # QKD protocol emulation (BB84, BBM92, PCCM)
```

### Compilation Speed Benchmarks

Demonstrate Arvak's microsecond-level compilation throughput in realistic algorithm loops:

```bash
# VQE: 5,000 circuits (500 iterations x 10 Hamiltonian terms)
cargo run --bin demo-speed-vqe

# QML: 20,000+ circuits (parameter-shift gradient, 1000 training steps)
cargo run --bin demo-speed-qml

# QAOA: sensor network optimization with depth sweep + grid search
cargo run --bin demo-speed-qaoa
```

Each demo reports per-circuit compile times, gates/s throughput, and speedup vs. a 100ms/circuit baseline.

### LUMI Hybrid VQE Demo

Complete quantum-HPC hybrid workflow using VQE for H₂ molecule ground state energy:

```bash
# Local simulation
cargo run -p lumi-hybrid -- --shots 1000 --iterations 20

# Bond distance scan
cargo run -p lumi-hybrid -- --mode bond-scan --start 0.5 --end 2.0 --points 10

# On LUMI (via SLURM)
cd demos/lumi-hybrid
sbatch slurm/vqe_workflow.sh
```

**Features:**
- UCCSD ansatz for H₂ molecule
- Jordan-Wigner transformed Hamiltonian
- Nelder-Mead optimizer (converges to chemical accuracy)
- SLURM job scripts for LUMI-G (GPU) and LUMI-Q (quantum)
- Python visualization for results

See [demos/lumi-hybrid/README.md](demos/lumi-hybrid/README.md) for detailed setup.

## Web Dashboard

Arvak includes a web-based dashboard for circuit visualization, compilation, and job monitoring:

```bash
# Run the dashboard with simulator backend
cargo run -p arvak-dashboard --features with-simulator

# Dashboard available at http://localhost:3000
```

**Features:**
- **Circuit Visualization**: Interactive circuit diagrams with D3.js
- **Compilation**: Compile circuits for different targets with before/after comparison
- **Backend Status**: View registered backends and capabilities
- **Job Monitoring**: Track job status, view QASM, inspect results
- **Result Histograms**: Interactive D3.js histograms

**API Endpoints:**

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Health check and version info |
| `/api/circuits/visualize` | POST | Parse QASM3 and return visualization data |
| `/api/circuits/compile` | POST | Compile circuit for target with before/after |
| `/api/backends` | GET | List all registered backends |
| `/api/jobs` | GET | List jobs (with filtering) |
| `/api/jobs` | POST | Create a new job |
| `/api/jobs/:id/result` | GET | Get job execution results |

## Quantum Types (Qrisp-inspired)

```rust
use arvak_types::{QuantumInt, QuantumFloat, QuantumArray};
use arvak_ir::Circuit;

fn main() -> anyhow::Result<()> {
    let mut circuit = Circuit::new("arithmetic");

    // Create quantum integers
    let a = QuantumInt::<4>::new(&mut circuit);  // 4-bit integer [0, 15]
    let b = QuantumInt::<4>::new(&mut circuit);

    // Initialize values
    a.initialize(5, &mut circuit)?;  // a = |5⟩
    b.initialize(3, &mut circuit)?;  // b = |3⟩

    // Create quantum floats (sign + mantissa + exponent)
    let x = QuantumFloat::<4, 3>::new(&mut circuit);  // 4-bit mantissa, 3-bit exponent

    // Create quantum arrays
    let arr = QuantumArray::<4, 8>::new(&mut circuit);  // 4 elements, 8 qubits each

    Ok(())
}
```

## Automatic Uncomputation

```rust
use arvak_auto::{UncomputeContext, uncompute};
use arvak_ir::Circuit;

fn main() -> anyhow::Result<()> {
    let mut circuit = Circuit::new("with_uncompute");

    // Mark the start of computation
    let ctx = UncomputeContext::begin(&circuit)
        .with_label("ancilla_block");

    // Perform operations on ancilla qubits
    circuit.h(QubitId(0))?;
    circuit.cx(QubitId(0), QubitId(1))?;

    // Automatically uncompute - appends inverse operations
    uncompute(&mut circuit, ctx)?;

    // Circuit now has: H, CX, CX†, H† (ancillas back to |0⟩)
    Ok(())
}
```

## Supported Gates

### Single-Qubit Gates
| Gate | Description | Syntax |
|------|-------------|--------|
| Hadamard | Superposition | `h q[0];` |
| Pauli-X/Y/Z | Bit/phase flip | `x q[0];` |
| S/T | Phase gates | `s q[0];` |
| RX/RY/RZ | Rotations | `rx(θ) q[0];` |
| U | Universal | `u(θ,φ,λ) q[0];` |
| PRX | Phased RX (IQM) | `prx(θ,φ) q[0];` |

### Two-Qubit Gates
| Gate | Description | Syntax |
|------|-------------|--------|
| CNOT | Controlled-X | `cx q[0], q[1];` |
| CZ | Controlled-Z | `cz q[0], q[1];` |
| SWAP | Qubit swap | `swap q[0], q[1];` |
| iSWAP | Imaginary swap | `iswap q[0], q[1];` |

### Three-Qubit Gates
| Gate | Description | Syntax |
|------|-------------|--------|
| Toffoli | CCX | `ccx q[0], q[1], q[2];` |

## Current Status

| Component | Status | Notes |
|-----------|--------|-------|
| Circuit IR (`arvak-ir`) | ✅ Complete | DAG-based representation, shuttle instructions, integrity checks, noise model |
| QASM3 Parser (`arvak-qasm3`) | ✅ Complete | Parse & emit, neutral-atom pragmas |
| Compilation (`arvak-compile`) | ✅ Complete | Pass manager, layout, routing, optimization, measurement verification |
| HAL (`arvak-hal`) | ✅ Complete | Backend trait, plugin system, registry, neutral-atom topology |
| CLI (`arvak-cli`) | ✅ Complete | compile, run, eval, backends commands |
| **Evaluator** (`arvak-eval`) | ✅ Complete | **QDMI contracts, emitter compliance, orchestration, benchmarks** |
| **Benchmarks** (`arvak-bench`) | ✅ Complete | **Quantum Volume, CLOPS, Randomized Benchmarking** |
| **gRPC Service** (`arvak-grpc`) | ✅ Complete | **10 RPCs, async execution, thread-safe** |
| **gRPC Python Client** (`arvak_grpc`) | ✅ Complete | **v1.7.1: Async, futures, caching, analysis** |
| Quantum Types (`arvak-types`) | ✅ Complete | QuantumInt, QuantumFloat, QuantumArray |
| Auto-Uncompute (`arvak-auto`) | ✅ Complete | Automatic ancilla uncomputation |
| Simulator (`arvak-adapter-sim`) | ✅ Complete | Statevector simulation |
| IQM Adapter (`arvak-adapter-iqm`) | ✅ Complete | Resonance API integration |
| IBM Adapter (`arvak-adapter-ibm`) | ✅ Complete | Qiskit Runtime API + IBM Cloud |
| **Scaleway Adapter** (`arvak-adapter-scaleway`) | ✅ Complete | **Scaleway QaaS: IQM Garnet, Emerald, Sirius** |
| **Braket Adapter** (`arvak-adapter-braket`) | ✅ Complete | **AWS Braket: IonQ, Rigetti, IQM, simulators** |
| **CUDA-Q Adapter** (`arvak-adapter-cudaq`) | ✅ Complete | **NVIDIA GPU-accelerated simulation** |
| QDMI Adapter (`arvak-adapter-qdmi`) | ✅ Complete | QDMI v1.2.1 device interface, prefix-aware dlsym |
| HPC Scheduler (`arvak-sched`) | ✅ Complete | SLURM & PBS, workflows, message broker, job routing |
| Dashboard (`arvak-dashboard`) | ✅ Complete | Web UI for circuit visualization, compilation, job monitoring |
| Python Bindings (`arvak-python`) | ✅ Complete | PyO3 bindings + 4 framework integrations + real simulator |
| **Framework Integrations** | ✅ Complete | **Qiskit, Qrisp, Cirq, PennyLane + 5 notebooks** |
| Demos | ✅ Complete | Grover, VQE, QAOA, QI-Nutshell, speed benchmarks (VQE/QML/QAOA) |

## Testing

```bash
# Run all tests
cargo test

# Run integration tests (60+ tests)
cd crates/arvak-python
pytest tests/

# Run specific framework tests
pytest tests/test_qiskit_integration.py
pytest tests/test_registry.py  # 14 tests, 100% passing

# Verify entire integration system
python tests/verify_integration_system.py
```

## Roadmap

### Phase 1-4: Foundation & Production ✅ COMPLETE
- [x] Circuit IR, QASM3 parser, CLI
- [x] Compilation passes, layout, routing
- [x] IQM, IBM, QDMI adapters
- [x] SLURM, PBS integration
- [x] Quantum types, automatic uncomputation
- [x] **v1.0.0 release**

### Phase 5: Ecosystem Integration ✅ COMPLETE
- [x] Extensible plugin architecture with auto-discovery
- [x] Qiskit integration (Backend, converter, 15+ tests)
- [x] Qrisp integration (Backend client, 22+ tests)
- [x] Cirq integration (Sampler/Engine, 25+ tests)
- [x] PennyLane integration (Device, QML examples)
- [x] Template system for adding frameworks (~30 min)
- [x] 5 interactive Jupyter notebooks
- [x] Complete integration guide (INTEGRATION_GUIDE.md)
- [x] PyPI publication as `arvak`
- [x] **v1.1.0 → v1.1.1 → v1.2.0 release**

### Phase 6: MQSS Alignment & Advanced Features ✅ COMPLETE
- [x] Measurement safety verification pass
- [x] DAG integrity checker
- [x] Benchmark suite (Quantum Volume, CLOPS, Randomized Benchmarking)
- [x] Pass categorization (agnostic vs target-specific)
- [x] Two-level IR markers (Logical/Physical circuits)
- [x] QDMI system-integration via FFI
- [x] NVIDIA CUDA-Q adapter (GPU-accelerated simulation)
- [x] Neutral-atom target (zoned topology, shuttle routing)
- [x] Dynamic backend plugin system (`libloading`)
- [x] Message broker with NATS-style subject matching
- [x] Job router with automatic cloud/HPC/local routing
- [x] **v1.3.0 release**

### Phase 7: Compiler & Orchestration Observability ✅ COMPLETE
- [x] Evaluator crate (`arvak-eval`) with 9-step pipeline
- [x] Input analysis (QASM3 parsing, structural metrics, content hashing)
- [x] Compilation observer (pass-wise before/after snapshots)
- [x] QDMI contract checker (Safe/Conditional/Violating classification)
- [x] Orchestration insights (hybrid DAG, critical path, batchability)
- [x] HPC scheduler fitness (LRZ/LUMI constraints, walltime estimation)
- [x] Emitter compliance (IQM/IBM/CUDA-Q coverage, decomposition costs, loss documentation)
- [x] Optional benchmark loader (GHZ, QFT, Grover, Random circuits)
- [x] Unified metrics aggregator (compilation + orchestration + emitter deltas)
- [x] JSON export with schema versioning (v0.3.0) and reproducibility tracking
- [x] CLI: `arvak eval` with `--orchestration`, `--emit`, `--benchmark` flags
- [x] 62 unit tests
- [x] **v1.4.0 release**

### Phase 8: Speed, Noise & Protocol Demos ✅ COMPLETE
- [x] Compilation speed demos: VQE (5K circuits), QML (20K+), QAOA (6K+ with depth sweep)
- [x] Noise-as-infrastructure model (`NoiseModel`, `NoiseChannel`) in `arvak-ir`
- [x] QI-Nutshell demo: BB84, BBM92, PCCM quantum communication protocols
- [x] QDMI v1.2.1 rewrite with native device interface and prefix-aware dlsym
- [x] Real simulator backends for all Python frameworks (Qiskit, Qrisp, Cirq, PennyLane)
- [x] End-to-end smoke test (`scripts/smoke-test.sh`)
- [x] Compile-time metrics in dashboard
- [x] **v1.5.0 release**

### Phase 9: Multi-Backend & AI Research ✅ COMPLETE
- [x] Scaleway QaaS adapter (IQM Garnet 20q, Emerald 54q, Sirius 16q)
- [x] AWS Braket adapter (IonQ, Rigetti, IQM, simulators)
- [x] IBM Cloud API adapter with Heron compilation support
- [x] Nathan AI research optimizer (1,700+ papers, hardware-aware analysis)
- [x] Code anonymization for Nathan (PII protection before LLM analysis)
- [x] Multi-backend CLI (`arvak run --backend scaleway/braket/ibm`)
- [x] Python compile bindings
- [x] IBM Quantum benchmarks on ibm_torino (133-qubit Heron)
- [x] Comprehensive architectural audit (100+ security/correctness fixes)
- [x] **v1.8.0 release**

### Phase 10: Community & Ecosystem
- [ ] Error mitigation (ZNE, readout correction, Pauli twirling)
- [ ] Pulse-level control for IQM/IBM
- [ ] Advanced routing algorithms (SABRE improvements)
- [ ] Circuit equivalence checking
- [ ] Plugin marketplace for community integrations
- [ ] Performance benchmarks vs Qiskit transpiler
- [ ] Cloud deployment guides (AWS Braket, Azure Quantum)

## Evaluator (`arvak-eval`)

Arvak includes a comprehensive evaluator for compiler and orchestration observability, producing structured JSON reports:

```bash
# Basic evaluation: input analysis + compilation + QDMI contract check
arvak eval --input circuit.qasm --target iqm --export report.json

# Full evaluation: orchestration + emitter compliance + benchmark
arvak eval --input circuit.qasm --target iqm \
  --orchestration --scheduler-site lrz \
  --emit iqm \
  --benchmark ghz --benchmark-qubits 8
```

**Pipeline (9 steps):**

| Step | Module | Description |
|------|--------|-------------|
| 1 | Input Analysis | QASM3 parsing, structural metrics, content hashing |
| 2 | Compilation Observer | Pass-wise before/after snapshots with deltas |
| 3 | QDMI Contract Checker | Gate safety classification (Safe/Conditional/Violating) |
| 4 | Orchestration (opt) | Hybrid quantum-classical DAG, critical path, batchability |
| 5 | Emitter Compliance (opt) | Native gate coverage, decomposition costs, loss docs |
| 6 | Benchmark Loader (opt) | GHZ, QFT, Grover, Random circuit workloads |
| 7 | Metrics Aggregator | Unified compilation + orchestration + emitter deltas |
| 8 | Reproducibility | CLI args, versions, schema tracking |
| 9 | JSON Export | Structured report with schema v0.3.0 |

**Emitter Compliance Targets:**

| Target | Native Gates | Use Case |
|--------|-------------|----------|
| `iqm` | PRX, CZ | IQM hardware (LUMI, LRZ) |
| `ibm` | SX, RZ, CX | IBM Quantum |
| `cuda-q` | Universal | NVIDIA simulation |

**HPC Scheduler Fitness:**

| Site | Partition | Max Qubits | Max Walltime |
|------|-----------|------------|--------------|
| LRZ | qc_iqm | 20 | 1 hour |
| LUMI | q_fiqci | 5 | 15 minutes |

## Benchmarks

Arvak includes a standard quantum benchmark suite (`arvak-bench`) for evaluating hardware and compilation quality:

| Benchmark | Metric | Description |
|-----------|--------|-------------|
| **Quantum Volume** | QV = 2^n | Random SU(4) circuits, heavy output probability |
| **CLOPS** | circuits/sec | End-to-end compilation + execution throughput |
| **Randomized Benchmarking** | Gate fidelity | Single- and two-qubit Clifford RB with exponential decay fit |

```bash
# Run benchmarks via CLI
cargo run -p arvak-bench
```

## QDMI Integration (Munich Quantum Software Stack)

Arvak provides native integration with [QDMI](https://github.com/Munich-Quantum-Software-Stack/QDMI), the Quantum Device Management Interface from the Munich Quantum Software Stack (MQSS).

```rust
use arvak_adapter_qdmi::QdmiBackend;
use arvak_hal::Backend;

let backend = QdmiBackend::new()
    .with_token("your-api-token")
    .with_base_url("https://qdmi.lrz.de");

// Access any QDMI-compliant device
let caps = backend.capabilities().await?;
let job_id = backend.submit(&circuit, 1000).await?;
let result = backend.wait(&job_id).await?;
```

This integration allows Arvak to access quantum devices at European HPC centers through the standardized QDMI interface, complementing Arvak's existing IQM and IBM adapters.

## NVIDIA CUDA-Q Integration

Arvak integrates with [NVIDIA CUDA-Q](https://developer.nvidia.com/cuda-q) for GPU-accelerated quantum simulation:

```rust
use arvak_adapter_cudaq::CudaqBackend;
use arvak_hal::Backend;

let backend = CudaqBackend::new()
    .with_target("nvidia-mqpu")
    .with_credentials("your-api-token");

let job_id = backend.submit(&circuit, 1000).await?;
let result = backend.wait(&job_id).await?;
```

**Supported targets:** `nvidia-mqpu` (multi-GPU), `custatevec` (single-GPU statevector), `tensornet` (tensor network), `dm` (density matrix).

## Neutral-Atom Support

Arvak provides first-class support for neutral-atom quantum architectures with zoned topologies and qubit shuttling:

```rust
use arvak_hal::capability::{Topology, Capabilities};
use arvak_compile::passes::NeutralAtomRouting;

// Configure neutral-atom topology with 3 interaction zones
let topology = Topology::neutral_atom(20, 3);
let caps = Capabilities::neutral_atom("my-device", 20, 3);

// Zone-aware routing automatically inserts shuttle instructions
let routing_pass = NeutralAtomRouting::new(20, 3);
```

The compiler automatically inserts shuttle instructions for cross-zone two-qubit gates, enabling efficient compilation for platforms like planqc or Pasqal.

## Acknowledgments

Arvak builds on ideas from and integrates with:

- [Qiskit](https://qiskit.org/) — Circuit representation, transpiler architecture, and IBM Quantum ecosystem
- [Qrisp](https://qrisp.eu/) — High-level abstractions and automatic uncomputation
- [Cirq](https://quantumai.google/cirq) — Google Quantum AI framework and NISQ algorithms
- [PennyLane](https://pennylane.ai/) — Quantum machine learning and automatic differentiation
- [XACC](https://github.com/eclipse-xacc/xacc) — HPC integration patterns
- [QDMI](https://github.com/Munich-Quantum-Software-Stack/QDMI) — Munich Quantum Software Stack device interface

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

**Adding Framework Integrations:**
See [docs/INTEGRATION_GUIDE.md](docs/INTEGRATION_GUIDE.md) for the complete guide on adding new framework integrations (~30 minutes with our template system).

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

## Contact & Support

### The HAL Contract

Arvak is developed and maintained by **[The HAL Contract](https://www.hal-contract.org)**, an initiative dedicated to advancing quantum computing infrastructure for European HPC centers.

- **Website**: [www.hal-contract.org](https://www.hal-contract.org)
- **Email**: [daniel@hal-contract.org](mailto:daniel@hal-contract.org)

### Project Resources

- **GitHub Repository**: [github.com/hiq-lab/arvak](https://github.com/hiq-lab/arvak)
- **GitHub Issues**: [github.com/hiq-lab/arvak/issues](https://github.com/hiq-lab/arvak/issues)
- **Documentation**: [docs/](docs/)
- **PyPI Package**: [pypi.org/project/arvak](https://pypi.org/project/arvak/)

### Collaboration & Partnership

For collaboration opportunities, enterprise support, or partnership inquiries, please contact us through [The HAL Contract](https://www.hal-contract.org).
