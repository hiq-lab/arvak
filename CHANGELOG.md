# Changelog

All notable changes to Arvak will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
  - `HIQProvider` and `HIQBackend` for using Arvak as Qiskit backend
  - Circuit conversion via OpenQASM 3.0 interchange format
  - ~15 comprehensive tests with graceful dependency skipping
  - Full documentation and interactive notebook

- **Qrisp Integration** (High-level quantum programming)
  - `QrispIntegration` supporting QuantumVariable and QuantumSession
  - Support for Qrisp's automatic uncomputation features
  - `HIQBackendClient` implementing Qrisp's backend interface
  - 22 comprehensive tests covering all conversion scenarios
  - Examples demonstrating high-level quantum types

- **Cirq Integration** (Google Quantum AI)
  - `CirqIntegration` with LineQubit and GridQubit support
  - `HIQSampler` and `HIQEngine` implementing Cirq's execution interfaces
  - Support for Cirq's Moments and parametrized circuits
  - 25+ comprehensive tests for all gate types and topologies
  - NISQ algorithm examples and hardware-native circuits

- **PennyLane Integration** (Quantum machine learning)
  - `PennyLaneIntegration` for QNode and quantum tape conversion
  - `HIQDevice` implementing PennyLane's Device interface
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
- Published as `hiq-quantum` on PyPI
- Optional dependencies for framework integrations:
  - `pip install hiq-quantum[qiskit]` - IBM Quantum
  - `pip install hiq-quantum[qrisp]` - High-level programming
  - `pip install hiq-quantum[cirq]` - Google Quantum AI
  - `pip install hiq-quantum[pennylane]` - Quantum ML
  - `pip install hiq-quantum[all]` - All frameworks

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
- **hiq-ir**: Complete circuit intermediate representation with DAG-based architecture
  - Qubit and classical bit management
  - 30+ standard gates (H, X, Y, Z, S, T, CX, CZ, CCX, etc.)
  - Parameterized gates with symbolic expressions
  - High-level Circuit builder API

- **hiq-qasm3**: Full OpenQASM 3.0 parser and emitter
  - Parse QASM files into Arvak circuits
  - Emit circuits back to valid QASM
  - Round-trip support for circuit serialization

- **hiq-compile**: Modular compilation framework
  - Pass manager for orchestrating compilation
  - PropertySet for inter-pass communication
  - Layout passes (Trivial, Dense)
  - Routing passes (Basic, SABRE-style)
  - Basis translation for IQM (PRX+CZ) and IBM (SX+RZ+CX)
  - Advanced optimization passes:
    - `Optimize1qGates`: Merge consecutive 1-qubit gates via ZYZ decomposition
    - `CancelCX`: Cancel adjacent CX·CX pairs
    - `CommutativeCancellation`: Merge same-type rotation gates

- **hiq-hal**: Hardware abstraction layer
  - Unified Backend trait for all quantum systems
  - Capabilities API for hardware description
  - Job lifecycle management
  - OIDC authentication for HPC sites (LUMI, LRZ)

- **hiq-cli**: Command-line interface
  - `hiq compile`: Compile circuits for target hardware
  - `hiq run`: Execute circuits on backends
  - `hiq backends`: List available backends

#### Quantum Types (Qrisp-inspired)
- **hiq-types**: High-level quantum data types
  - `QuantumInt<N>`: Fixed-width quantum integers
  - `QuantumFloat<M, E>`: Quantum floating-point numbers
  - `QuantumArray<N, W>`: Arrays of quantum values
  - `QubitRegister`: Qubit allocation management

- **hiq-auto**: Automatic uncomputation framework
  - Gate inversion utilities
  - `UncomputeContext` for marking computation sections
  - Circuit analysis for determining uncomputable qubits
  - Computational cone detection

#### Backend Adapters
- **hiq-adapter-sim**: Local statevector simulator
  - Exact simulation up to ~25 qubits
  - All standard gates supported
  - Measurement sampling

- **hiq-adapter-iqm**: IQM Quantum backend
  - Resonance cloud API integration
  - LUMI (Helmi) and LRZ support
  - OIDC authentication

- **hiq-adapter-ibm**: IBM Quantum backend
  - Qiskit Runtime API integration
  - All IBM Quantum systems supported

#### HPC Integration
- **hiq-sched**: HPC job scheduler
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
- **hiq-python**: PyO3-based Python interface
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

[1.1.0]: https://github.com/hiq-lab/HIQ/releases/tag/v1.1.0
[1.0.0]: https://github.com/hiq-lab/HIQ/releases/tag/v1.0.0
[0.1.0]: https://github.com/hiq-lab/HIQ/releases/tag/v0.1.0
