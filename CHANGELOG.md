# Changelog

All notable changes to HIQ will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2025-02-05

### Added

#### Core Infrastructure
- **hiq-ir**: Complete circuit intermediate representation with DAG-based architecture
  - Qubit and classical bit management
  - 30+ standard gates (H, X, Y, Z, S, T, CX, CZ, CCX, etc.)
  - Parameterized gates with symbolic expressions
  - High-level Circuit builder API

- **hiq-qasm3**: Full OpenQASM 3.0 parser and emitter
  - Parse QASM files into HIQ circuits
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

[1.0.0]: https://github.com/hiq-project/hiq/releases/tag/v1.0.0
[0.1.0]: https://github.com/hiq-project/hiq/releases/tag/v0.1.0
