# Changelog

All notable changes to Arvak will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.5.2] - 2026-02-12

### Changed

#### Code Structure Improvements (Phase 4)
- **QASM3 parser modularization**: Split `parser.rs` (1285 LOC) into focused modules:
  - `parser/mod.rs` - Public API and core utilities (262 LOC)
  - `parser/expression.rs` - Expression parsing with precedence climbing (162 LOC)
  - `parser/statement.rs` - Statement parsing for all QASM3 constructs (349 LOC)
  - `parser/lowering.rs` - AST-to-Circuit transformation (539 LOC)
- **Optimization pass modularization**: Split `optimization.rs` (914 LOC) into focused modules:
  - `optimization/mod.rs` - Module exports and shared constants (13 LOC)
  - `optimization/optimize_1q.rs` - Single-qubit gate optimization (380 LOC)
  - `optimization/cancel.rs` - CX and commutative cancellation (388 LOC)
  - `optimization/tests.rs` - All optimization tests (168 LOC)
- **gRPC service modularization**: Split `service.rs` into focused modules:
  - `service/mod.rs` - Service orchestration
  - `service/backend_service.rs` - Backend management
  - `service/job_service.rs` - Job execution
  - `service/job_execution.rs` - Job processing
  - `service/circuit_utils.rs` - Circuit utilities
- **Hot-path clone reduction**: Optimized compilation performance by removing unnecessary clones:
  - `optimize_1q.rs`: Removed 2 clones using `zip()` with `into_iter()`
  - `translation.rs`: Reduced from 18 to 10 clones using `extend_from_slice()` (remaining 10 are cheap Arc-based `ParameterExpression` clones)

#### Python Bindings
- **PyO3 upgrade**: Updated from 0.23 to 0.28 for improved compatibility:
  - Better arm64 macOS support
  - Python 3.13 compatibility improvements
  - Updated API: `allow_threads` → `detach` for GIL management
  - Fixed deprecation warnings with `from_py_object` on all `#[pyclass]` types

### Fixed
- Python bindings compatibility with PyO3 0.28 API changes
- CI/nightly build for Python bindings on arm64 macOS

---

## [1.5.1] - 2026-02-11

### Added
- **Computational chemistry notebook**: LiH and H₂O VQE with potential energy surface curves
- **`Circuit.size()` Python binding**

### Changed

#### Compiler Performance Optimizations
- **DAG traversal**: Algorithmic improvements in `CircuitDag` for faster topological iteration and node lookup
- **Routing passes**: Optimized swap insertion and neutral-atom routing with reduced overhead
- **Optimization pass**: Improved gate cancellation and commutation analysis
- **PropertySet**: Faster typed property lookups with reduced allocation
- **Verification pass**: Streamlined measurement barrier verification
- **Build tooling**: Added `.cargo/config.toml` (codegen/linker settings) and `Makefile` for common workflows
- **CLI**: mimalloc global allocator for improved memory performance

#### CI/CD
- Nightly pipeline streamlined from 12 → 10 jobs (merged 3 dependency jobs, trimmed macOS/Python matrices, folded adapter-compat)
- Docker build validation job with buildx and GHA layer cache
- VPS smoke test: full-stack end-to-end test on arvak.io (Docker build, all framework integrations, cargo tests, dashboard health, audit)
- Configurable smoke test ports via `ARVAK_DASHBOARD_PORT` / `ARVAK_GRPC_HTTP_PORT` env vars
- **Makefile**: Added `docker-validate` target

### Fixed
- Python notebook bugs and missing pip dependencies
- Qrisp `from_arvak` converter: known-skip for Qrisp 0.6.x / Qiskit 2.x `_bits` incompatibility

---

## [1.5.0] - 2026-02-10

### Added

#### Compilation Speed Demos
- **demo-speed-vqe**: VQE compilation throughput benchmark — 5,000 circuits (500 optimizer iterations x 10 Hamiltonian terms) compiled at O0 and O2, reports per-circuit time and speedup vs 100ms baseline
- **demo-speed-qml**: QML training loop benchmark — 20,000+ circuits (parameter-shift gradient, 1000 steps) with parameterized quantum classifier circuits
- **demo-speed-qaoa**: QAOA sensor network benchmark — 6,000+ circuits across three tactical scenarios (drone patrol, radar deconfliction, surveillance grid) with QAOA depth sweep and angle grid search
- **QML circuit generator** (`demos/src/circuits/qml.rs`): Parameterized quantum classifier with data encoding (Rx) and variational (Ry+CZ) layers
- **Sensor assignment problems** (`demos/src/problems/sensor_assignment.rs`): Predefined weighted graphs for QAOA sensor network scenarios

#### Noise as Infrastructure
- **NoiseModel**: First-class noise model in `arvak-ir` with per-gate and per-qubit noise channels
- **NoiseChannel**: Depolarizing, amplitude damping, phase damping, bit-flip, phase-flip, and custom Kraus channels
- Noise model propagated across the stack: QASM3 emitter, scheduler matcher, dashboard, adapters

#### QI-Nutshell Demo
- **demo-qi-nutshell**: Quantum communication protocol emulation from "Quantum Internet in a Nutshell" (Hilder et al.)
- BB84, BBM92, PCCM protocols with QBER analysis and QEC error correction
- Compile-time metrics showing Arvak's overhead for protocol circuits

#### QDMI v1.2.1 Device Interface
- **Complete QDMI rewrite** to match QDMI v1.2.1 device interface specification
- Prefix-aware dlsym for multi-device shared libraries
- Native C FFI session lifecycle (alloc, device query, job submit/wait)
- Mock device integration tests with thread-safe atomic refcounting

#### Python & Integration Improvements
- **Real simulator backends**: All 4 Python framework backends (Qiskit, Qrisp, Cirq, PennyLane) now use Arvak's Rust statevector simulator via PyO3 instead of mock data
- **`arvak.run_sim()`**: New PyO3 binding exposing `SimulatorBackend::run_simulation()` with GIL release
- **PennyLane v0.44 support**: Updated converter and device for latest PennyLane QASM3 interface
- **End-to-end smoke test**: `scripts/smoke-test.sh` validates the entire stack

#### Dashboard
- **Compile-time metrics**: Dashboard compilation tab now shows per-pass timing breakdown

### Changed
- **QDMI adapter**: Rewritten from scratch for v1.2.1 spec compliance (1,937 lines changed)
- **Evaluator**: Removed QDMI contract checker module (superseded by native device interface validation)
- **Notebook naming**: All notebooks renamed from `hiq` to `arvak` prefix

### Fixed
- PennyLane integration compatibility with v0.44 (QASM3 circuit conversion)
- Thread safety in QDMI mock device (atomic refcount on macOS)
- DDSIM shim compatibility with mqt-core v3.4.1

## [1.4.0] - 2026-02-10

### Added
- **Real simulator backends**: All 4 Python framework backends (Qiskit, Qrisp, Cirq, PennyLane) now use Arvak's built-in Rust statevector simulator via PyO3 instead of returning mock data
- **`arvak.run_sim()` function**: New PyO3 binding exposing `SimulatorBackend::run_simulation()` directly to Python, with GIL release for concurrent execution
- **PennyLane VQE demo notebook**: Notebook 05 rewritten as a full VQE workflow demonstrating Arvak's compilation speed advantage for molecular simulations
- **PennyLane integration tests**: Complete test suite for PennyLane converter, ArvakDevice, and round-trip conversion
- **Simulator validation tests**: Added quantum-correctness tests (Bell/GHZ state outcomes) across all 4 framework backend test suites
- **`test_run_sim.py`**: Dedicated test suite for the core `run_sim` PyO3 binding (Bell state, GHZ, single-qubit, QASM circuits)
- **End-to-end smoke test**: `scripts/smoke-test.sh` validates the entire stack (Python SDK, simulator, converters, gRPC, dashboard)

### Changed
- **Notebook naming**: All notebooks renamed from `hiq` to `arvak` prefix, all code/markdown cells updated
- **Notebook template**: Fixed remaining `hiq`/`HIQ` references in framework template
- **`generate_notebook.py`**: Fixed stale `python/hiq/` path reference

### Removed
- **Mock backends**: Removed all `_mock_results()` methods and `RuntimeWarning` mock notices from Python backends
- **gRPC auth/rate-limit stubs**: Removed placeholder `AuthInterceptor` and `RateLimiter` (deploy behind nginx/Envoy with mTLS for production)

## [1.3.0] - 2026-02-07

### Added

#### Phase A: Compiler Correctness
- **MeasurementBarrierVerification pass**: Safety-net analysis pass that detects when optimization passes incorrectly move gates across measurement boundaries
- **DAG integrity checker**: `dag.verify_integrity()` method validates in/out nodes, acyclicity, reachability, and wire consistency
- **10 measurement safety integration tests**: Comprehensive test suite covering mid-circuit measurement, reset sequences, and multi-qubit measurement edge cases

#### Phase B: Benchmarks & Pass Organization
- **Quantum Volume (QV)**: Random SU(4) circuit generation with heavy output probability calculation
- **CLOPS benchmark**: End-to-end compilation throughput measurement (circuits/second)
- **Randomized Benchmarking**: Single- and two-qubit Clifford RB with exponential decay fitting for gate fidelity estimation
- **Pass categorization**: Reorganized passes into `agnostic/` (hardware-independent) and `target/` (hardware-specific) directories
- **Two-level IR markers**: `CircuitLevel::Logical` and `CircuitLevel::Physical` for tracking compilation stage

#### Phase C: Ecosystem Extension
- **NVIDIA CUDA-Q adapter** (`arvak-adapter-cudaq`): REST API client with QASM3 interchange, supporting `nvidia-mqpu`, `custatevec`, `tensornet`, and `dm` targets
- **Neutral-atom target**: `TopologyKind::NeutralAtom` with zoned topology, `InstructionKind::Shuttle` for qubit shuttling, and `NeutralAtomRouting` zone-aware compilation pass
- **Dynamic backend plugin system**: `BackendPlugin` trait with `libloading` for runtime `.so/.dylib` loading, `BackendRegistry` for unified backend discovery (feature-gated: `--features dynamic-backends`)
- **Message broker**: `MessageBroker` trait with NATS-style wildcard subject matching, `InMemoryBroker` implementation for testing and single-node deployments
- **Job router**: `JobRouter` with automatic routing by qubit count (local/cloud/HPC), configurable thresholds, and preferred backend support
- **QDMI system-integration**: Implemented all FFI stubs for the `system-qdmi` feature (session alloc, device query, job lifecycle via C FFI)

### Changed
- **Workspace version**: 1.2.0 → 1.3.0
- **Pass manager**: Verification pass automatically runs after all optimizations at opt level >= 1
- **InstructionKind**: Added `Shuttle { from_zone, to_zone }` variant, propagated across all exhaustive matches (emitter, simulator, dashboard, auto-uncompute)
- **BasisGates**: Added `neutral_atom()` preset (rz, rx, ry, cz, measure, barrier, shuttle)
- **CouplingMap**: Added `zoned()` constructor for neutral-atom zone topology

## [1.2.0] - 2026-02-07

### Added
- **Docker deployment**: Production-ready Dockerfile with multi-stage build for dashboard, CLI, gRPC server, and demo binaries
- **Docker Compose**: Orchestration for dashboard and gRPC services with configurable environment variables
- **gRPC reflection**: Added tonic-reflection support for tools like grpcurl
- **Live demo**: Deployed at [arvak.io](https://arvak.io) with SSL/HTTPS via Let's Encrypt

### Fixed
- **Dashboard ARVAK_BIND**: Dashboard now respects the `ARVAK_BIND` environment variable for configurable bind address
- **Dashboard asset paths**: Changed absolute paths to relative for correct loading behind reverse proxy
- **Jobs endpoint**: Return empty list instead of 500 error when no job store is configured
- **Jobs API client**: Added proper error handling for non-OK responses in the dashboard frontend

### Changed
- **Docker user**: Renamed container user from `hiq` to `arvak`
- **Environment variables**: Unified to `ARVAK_*` namespace across all services

## [arvak_grpc 1.6.0] - 2025-02-06

### Added - Phase 3: Data Export and Advanced Analysis

#### Week 1: Apache Arrow & Parquet Support
- **ResultExporter**: Export quantum measurement results to multiple formats
  - Apache Arrow columnar format for efficient in-memory processing
  - Parquet compressed storage (snappy, gzip, lz4, zstd codecs)
  - CSV export with customizable columns
  - JSON export with metadata preservation
  - `to_parquet()`, `from_parquet()`, `to_arrow_table()` methods
- **BatchExporter**: Incremental batch result export
  - Add results one-by-one or in batches
  - Export accumulated results to any format
  - Efficient memory usage for large datasets
- **Parquet metadata inspection**: `get_parquet_metadata()` function
- **Example**: `export_example.py` with 5 comprehensive demonstrations

#### Week 2: Pandas & Polars Integration
- **DataFrameConverter**: Convert JobResult to DataFrame formats
  - `to_pandas()`: Convert to pandas DataFrame with probabilities
  - `to_polars()`: Convert to polars DataFrame for faster operations
  - `batch_to_pandas()`, `batch_to_polars()`: Batch conversions
  - Optional metadata columns (job_id, shots, execution_time_ms)
- **StatisticalAnalyzer**: Comprehensive quantum statistics
  - Shannon entropy calculation (H = -Σ p log₂ p)
  - Purity computation (P = Σ p²)
  - Fidelity estimation (Bhattacharyya coefficient)
  - Total variation distance between distributions
  - Summary statistics with automatic analysis
- **Visualizer**: Matplotlib-based visualization tools
  - `plot_distribution()`: Bar charts of measurement results
  - `plot_comparison()`: Compare multiple distributions side-by-side
  - `plot_statistics_table()`: Display summary statistics
  - Automatic figure sizing and formatting
- **Example**: `dataframe_example.py` with 6 examples
- **Tests**: 23 tests (10 passing, 13 skipped for optional dependencies)

#### Week 3: Result Caching & Storage
- **MemoryCache**: In-memory LRU cache with TTL
  - Configurable max size and time-to-live
  - LRU eviction policy for memory efficiency
  - Access statistics (hits, misses, hit rate)
  - Automatic TTL-based expiration
- **DiskCache**: Persistent cache with multiple formats
  - JSON format for human readability
  - Parquet format for compression and speed
  - TTL support with automatic eviction
  - Hash-based subdirectory distribution
  - Metadata tracking for all cached results
- **TwoLevelCache**: L1 (memory) + L2 (disk) hierarchy
  - Automatic promotion from L2 to L1 on access
  - Configurable TTL per level
  - Unified API for both cache levels
  - Combined statistics reporting
- **CachedClient**: Transparent caching wrapper
  - Drop-in replacement for ArvakClient
  - Automatic result caching on retrieval
  - Configurable auto-cache behavior
  - Cache statistics API
- **Example**: `caching_example.py` with 6 examples
- **Tests**: 24 tests (23 passing, 1 skipped)

#### Week 4: Result Aggregation & Analysis
- **ResultAggregator**: Combine and filter results
  - `combine()`: Sum counts from multiple runs
  - `average()`: Compute averaged probability distributions
  - `filter_by_threshold()`: Remove low-probability states
  - `top_k_states()`: Keep only most common states
- **ResultComparator**: Advanced distribution comparison
  - Total Variation Distance (TVD)
  - Kullback-Leibler Divergence (KL)
  - Jensen-Shannon Divergence (JS)
  - Hellinger Distance
  - Probability Overlap (Bhattacharyya coefficient)
  - Pearson Correlation of counts
  - `compare()`: Comprehensive comparison with all metrics
- **ConvergenceAnalyzer**: Shot convergence analysis
  - `analyze_convergence()`: Track metrics vs shot count
  - Entropy, purity, fidelity tracking
  - Convergence detection with configurable threshold
  - `estimate_required_shots()`: Statistical shot estimation
- **ResultTransformer**: Result manipulation tools
  - `normalize()`: Fix rounding errors in counts
  - `downsample()`: Reduce shot count proportionally
  - `apply_noise()`: Simulate bit-flip errors
  - Reproducible noise with seed support
- **Batch operations**: Multi-result analysis
  - `batch_compare()`: Pairwise comparison matrix
  - `group_by_similarity()`: Cluster similar results
- **Example**: `analysis_example.py` with 7 advanced examples
- **Tests**: 23 tests (all passing)

### Changed
- **Package version**: 1.3.0 → 1.6.0
- **Dependencies**: Added optional extras for `export`, `polars`, `viz`
- **__init__.py**: Exported 40+ new classes and functions

### Documentation
- **README**: Expanded gRPC section from 44 to 276 lines
- **Examples**: 4 comprehensive example files (950+ lines)
- **Tests**: 70 total tests (56 passing, 14 skipped for optional deps)

### Performance
- **Caching**: Up to 100x speedup for repeated result access
- **Parquet**: Efficient compression (typically 5-10x smaller than JSON)
- **Arrow**: Zero-copy DataFrame conversions where possible

## [arvak_grpc 1.2.0] - 2025-02-06

### Added - Phase 2: Advanced Client Features

#### Async/Await Support
- **AsyncArvakClient**: Full async/await API with asyncio
  - Connection pooling for efficient resource usage
  - Concurrent job submission with `asyncio.gather()`
  - Non-blocking wait operations
  - Graceful connection management
  - Example: Submit 100 jobs concurrently

#### JobFuture Interface
- **JobFuture**: Promise-like interface for jobs
  - `result()`: Blocking result retrieval
  - `wait()`: Non-blocking wait with timeout
  - `cancel()`: Cancel running jobs
  - `add_done_callback()`: Register completion callbacks
  - `as_concurrent_future()`: Integration with concurrent.futures
- **Coordination functions**: `as_completed()`, `wait()` for multiple futures
- Background polling thread for non-blocking operation

#### Retry & Resilience
- **RetryPolicy**: Configurable retry with backoff strategies
  - Exponential backoff (default)
  - Linear backoff
  - Constant delay
  - Configurable max attempts and backoff multiplier
  - Retry on transient failures
- **CircuitBreaker**: Prevent cascading failures
  - Three states: CLOSED, OPEN, HALF_OPEN
  - Automatic recovery after timeout
  - Failure threshold configuration
  - State transition tracking
- **ResilientClient**: Combined retry + circuit breaker
  - Automatic transient error handling
  - Graceful degradation
  - Decorator functions: `@with_retry`, `@with_circuit_breaker`

#### Batch Operations
- **BatchJobManager**: Concurrent batch execution
  - ThreadPoolExecutor for parallel submission
  - Configurable worker pool size
  - Progress tracking with callbacks
  - Fail-fast or continue-on-error modes
  - Comprehensive statistics (success/failure rates, timing)
  - `execute_batch()`: High-level batch execution
  - 8.1x performance improvement over sequential (16.1 vs 2.0 jobs/s)

### Changed
- **Package version**: 1.0.0 → 1.2.0
- **Dependencies**: Added `grpcio.aio` for async support

### Documentation
- **Migration guide**: Backward-compatible upgrade path
- **Examples**: 5 new example files demonstrating async, futures, retry, batching
- **Tests**: 45 new tests (all passing)

## [arvak_grpc 1.0.0] - 2025-02-06

### Added - Phase 1: Core gRPC Service

#### Rust gRPC Server
- **arvak-grpc**: Complete gRPC service implementation
  - 10 RPCs: SubmitJob, SubmitBatch, GetJobStatus, GetJobResult, CancelJob, ListBackends, GetBackendInfo, WatchJob, StreamResults, SubmitBatchStream
  - Protobuf schema (arvak.proto) with 25 messages
  - Thread-safe in-memory job storage with `Arc<RwLock<FxHashMap>>`
  - Non-blocking job execution with tokio::spawn
  - Backend registry with feature-gated backends
  - Automatic timestamp management
  - Circuit format support: OpenQASM 3.0 (Arvak IR JSON format defined in proto but not yet implemented)

#### Python Client Library
- **ArvakClient**: Synchronous blocking client
  - `submit_qasm()`, `submit_circuit_json()`: Job submission
  - `submit_batch()`: Batch job submission
  - `get_job_status()`, `get_job_result()`: Result retrieval
  - `wait_for_job()`: Polling helper with configurable interval
  - `list_backends()`, `get_backend_info()`: Backend discovery
  - `cancel_job()`: Job cancellation
  - Connection management with context manager support

#### Type System
- **Job**: Job metadata with state tracking
- **JobResult**: Measurement results with counts
- **JobState**: Enum for job lifecycle (QUEUED, RUNNING, COMPLETED, FAILED, CANCELED)
- **BackendInfo**: Backend capabilities and metadata

#### Error Handling
- Custom exceptions: `ArvakError`, `ArvakJobNotFoundError`, `ArvakBackendNotFoundError`, `ArvakInvalidCircuitError`, `ArvakJobNotCompletedError`
- Proper gRPC status code mapping

### Documentation
- **README**: Complete gRPC section with examples
- **Examples**: Basic usage examples in Rust and Python
- **Tests**: 27 tests (5 unit + 9 integration + 13 Python)

### Performance
- Server: Handles 100+ jobs/sec submission rate
- Non-blocking: RPCs return immediately, jobs execute in background

## [1.1.1] - 2025-02-06

### Fixed
- **Python module initialization**: Fixed PyInit_arvak symbol export in Rust bindings
  - Changed #[pymodule] function name from `hiq` to `arvak` to match module name
  - Resolves warning: "Couldn't find the symbol `PyInit_arvak` in the native library"
  - Ensures proper module loading when importing arvak in Python

### Changed
- **Repository location**: Migrated from `hiq-lab/HIQ` to `hiq-lab/arvak`
  - Updated all repository URLs throughout documentation and configuration files
  - Maintained git history and tags during migration

## [1.1.0] - 2025-02-06

### Added

#### Multi-Framework Integration System
- **Extensible plugin architecture** with auto-discovery and registration
  - `FrameworkIntegration` abstract base class for consistent integration pattern
  - `IntegrationRegistry` with automatic module scanning
  - Zero-dependency core with optional framework extras
  - Public API: `list_integrations()`, `integration_status()`, `get_integration()`

#### Framework Integrations (4 Complete)
- **Qiskit Integration** (IBM Quantum ecosystem)
  - `QiskitIntegration` class with BackendV2 provider implementation
  - `ArvakProvider` and `ArvakBackend` for using Arvak as Qiskit backend
  - Circuit conversion via OpenQASM 3.0 interchange format
  - ~15 comprehensive tests with graceful dependency skipping
  - Full documentation and interactive notebook

- **Qrisp Integration** (High-level quantum programming)
  - `QrispIntegration` supporting QuantumVariable and QuantumSession
  - Support for Qrisp's automatic uncomputation features
  - `ArvakBackendClient` implementing Qrisp's backend interface
  - 22 comprehensive tests covering all conversion scenarios
  - Examples demonstrating high-level quantum types

- **Cirq Integration** (Google Quantum AI)
  - `CirqIntegration` with LineQubit and GridQubit support
  - `ArvakSampler` and `ArvakEngine` implementing Cirq's execution interfaces
  - Support for Cirq's Moments and parametrized circuits
  - 25+ comprehensive tests for all gate types and topologies
  - NISQ algorithm examples and hardware-native circuits

- **PennyLane Integration** (Quantum machine learning)
  - `PennyLaneIntegration` for QNode and quantum tape conversion
  - `ArvakDevice` implementing PennyLane's Device interface
  - Support for automatic differentiation workflows
  - Ready for quantum machine learning applications
  - QML examples with gradient computation

#### Developer Tools
- **Template system** for adding new frameworks
  - `framework_template.ipynb` with standard structure
  - `generate_notebook.py` script for automated notebook creation
  - Consistent patterns across all integrations (~30 min to add framework)

- **Comprehensive testing suite** (60+ tests)
  - Registry tests: 14 tests (100% passing)
  - Framework-specific integration tests
  - Graceful skipping when optional dependencies not installed
  - `verify_integration_system.py` for full system validation

- **Documentation and examples**
  - `INTEGRATION_GUIDE.md`: Complete contributor guide (18KB)
  - `QUICKSTART_INTEGRATIONS.md`: 5-minute user quickstart
  - 5 interactive Jupyter notebooks with examples
  - `FINAL_STATUS.md`: Achievement summary (133% of target)
  - Framework-specific implementation documentation

#### PyPI Package
- Published as `arvak` on PyPI
- Optional dependencies for framework integrations:
  - `pip install arvak[qiskit]` - IBM Quantum
  - `pip install arvak[qrisp]` - High-level programming
  - `pip install arvak[cirq]` - Google Quantum AI
  - `pip install arvak[pennylane]` - Quantum ML
  - `pip install arvak[all]` - All frameworks

### Changed
- **Python bindings** now include framework integration infrastructure
- **README** updated with comprehensive framework integration examples
- **Documentation** expanded to cover all four framework integrations

### Technical Details
- 38 files created/modified
- 8,468+ lines of code added
- Zero-dependency core maintains backward compatibility
- All integrations use OpenQASM 3.0 as universal interchange format
- Auto-discovery system requires no manual configuration

## [1.0.0] - 2025-02-05

### Added

#### Core Infrastructure
- **arvak-ir**: Complete circuit intermediate representation with DAG-based architecture
  - Qubit and classical bit management
  - 30+ standard gates (H, X, Y, Z, S, T, CX, CZ, CCX, etc.)
  - Parameterized gates with symbolic expressions
  - High-level Circuit builder API

- **arvak-qasm3**: Full OpenQASM 3.0 parser and emitter
  - Parse QASM files into Arvak circuits
  - Emit circuits back to valid QASM
  - Round-trip support for circuit serialization

- **arvak-compile**: Modular compilation framework
  - Pass manager for orchestrating compilation
  - PropertySet for inter-pass communication
  - Layout passes (Trivial, Dense)
  - Routing passes (Basic, SABRE-style)
  - Basis translation for IQM (PRX+CZ) and IBM (SX+RZ+CX)
  - Advanced optimization passes:
    - `Optimize1qGates`: Merge consecutive 1-qubit gates via ZYZ decomposition
    - `CancelCX`: Cancel adjacent CX·CX pairs
    - `CommutativeCancellation`: Merge same-type rotation gates

- **arvak-hal**: Hardware abstraction layer
  - Unified Backend trait for all quantum systems
  - Capabilities API for hardware description
  - Job lifecycle management
  - OIDC authentication for HPC sites (LUMI, LRZ)

- **arvak-cli**: Command-line interface
  - `arvak compile`: Compile circuits for target hardware
  - `arvak run`: Execute circuits on backends
  - `arvak backends`: List available backends

#### Quantum Types (Qrisp-inspired)
- **arvak-types**: High-level quantum data types
  - `QuantumInt<N>`: Fixed-width quantum integers
  - `QuantumFloat<M, E>`: Quantum floating-point numbers
  - `QuantumArray<N, W>`: Arrays of quantum values
  - `QubitRegister`: Qubit allocation management

- **arvak-auto**: Automatic uncomputation framework
  - Gate inversion utilities
  - `UncomputeContext` for marking computation sections
  - Circuit analysis for determining uncomputable qubits
  - Computational cone detection

#### Backend Adapters
- **arvak-adapter-sim**: Local statevector simulator
  - Exact simulation up to ~25 qubits
  - All standard gates supported
  - Measurement sampling

- **arvak-adapter-iqm**: IQM Quantum backend
  - Resonance cloud API integration
  - LUMI (Helmi) and LRZ support
  - OIDC authentication

- **arvak-adapter-ibm**: IBM Quantum backend
  - Qiskit Runtime API integration
  - All IBM Quantum systems supported

#### HPC Integration
- **arvak-sched**: HPC job scheduler
  - SLURM adapter (sbatch, squeue, sacct, scancel)
  - PBS/Torque adapter (qsub, qstat, qdel, qhold)
  - Workflow orchestration with DAG dependencies
  - Priority-based job queuing
  - Persistent state storage (JSON, SQLite)

#### Demo Applications
- **demos**: Example quantum algorithms
  - Grover's search algorithm
  - Variational Quantum Eigensolver (VQE)
  - Quantum Approximate Optimization (QAOA)
  - Molecular Hamiltonians (H2, LiH, BeH2, H2O)
  - Error mitigation (ZNE, Pauli twirling)
  - Multi-algorithm benchmarking

#### Python Bindings
- **arvak-python**: PyO3-based Python interface
  - Circuit building from Python
  - Compilation and optimization
  - QASM import/export

### Performance
- Compilation optimized for large circuits
- DAG operations use efficient graph algorithms
- Simulator uses vectorized operations

### Documentation
- Comprehensive rustdoc for all public APIs
- README with quick start guide
- Examples for common use cases

## [0.1.0] - Initial Development

- Initial project structure
- Core circuit representation
- Basic gate set
- Proof-of-concept simulator

---

## Migration Guide

### From Pre-1.0 Development Versions

If upgrading from development versions:

1. **Circuit API**: Use the builder pattern
   ```rust
   // Old
   let mut dag = CircuitDag::new();
   dag.add_qubits(2);

   // New (1.0)
   let mut circuit = Circuit::new("my_circuit");
   circuit.add_qubits(2);
   circuit.h(QubitId(0))?;
   ```

2. **Compilation**: Use PassManagerBuilder
   ```rust
   // New (1.0)
   let (pm, mut props) = PassManagerBuilder::new()
       .with_optimization_level(2)
       .with_target(CouplingMap::star(5), BasisGates::iqm())
       .build();
   ```

3. **Backend execution**: Use async/await
   ```rust
   // New (1.0)
   let backend = SimulatorBackend::new();
   let job_id = backend.submit(&circuit, 1000).await?;
   let result = backend.wait(&job_id).await?;
   ```

---

[1.5.1]: https://github.com/hiq-lab/arvak/releases/tag/v1.5.1
[1.5.0]: https://github.com/hiq-lab/arvak/releases/tag/v1.5.0
[1.4.0]: https://github.com/hiq-lab/arvak/releases/tag/v1.4.0
[1.3.0]: https://github.com/hiq-lab/arvak/releases/tag/v1.3.0
[1.2.0]: https://github.com/hiq-lab/arvak/releases/tag/v1.2.0
[1.1.1]: https://github.com/hiq-lab/arvak/releases/tag/v1.1.1
[1.1.0]: https://github.com/hiq-lab/arvak/releases/tag/v1.1.0
[1.0.0]: https://github.com/hiq-lab/arvak/releases/tag/v1.0.0
[0.1.0]: https://github.com/hiq-lab/arvak/releases/tag/v0.1.0
