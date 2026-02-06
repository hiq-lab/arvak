# Arvak Research Context

## Background Research

This document captures the research conducted on existing quantum computing frameworks that informed HIQ's design decisions.

## Qiskit Architecture Analysis

### Circuit Representation

**QuantumCircuit (User-Facing)**
- Core attribute `data`: sequence-like object exposing `CircuitInstruction` objects
- Immutable data attributes: `qubits`, `clbits`, `qregs`, `cregs`, `parameters`
- Design philosophy: Append-only operations

**DAGCircuit (Transpiler IR)**
- Directed acyclic graph with three node types: Input, Output, Operation
- Edges represent quantum wires (qubits) and classical wires
- Conversion pipeline: `QuantumCircuit` → `DAGCircuit` → transpiler passes → `QuantumCircuit`

**CircuitInstruction**
- Rust implementation for performance (handle to internal Rust data structures)
- Python interface provides access to Rust-backed circuit data

**Operation/Instruction/Gate Hierarchy**
- Operation (abstract): Very high-level view for abstract circuits
- Instruction (concrete): Actual quantum operations, can be unitary or non-unitary
- Gate (specialized): Subclass for unitary operations only

### Compilation Pipeline

**StagedPassManager** organizes compilation into 6 stages:
1. **init** — Initial setup and circuit validation
2. **layout** — Map logical qubits to physical qubits
3. **routing** — Insert SWAP gates for connectivity constraints
4. **translation** — Convert to basis gates
5. **optimization** — Optimize circuit depth/gate count
6. **scheduling** — Schedule gates and add timing delays

**Pass Types:**
- AnalysisPass: Reads circuit, writes to PropertySet, does NOT modify DAG
- TransformationPass: Modifies DAG, can read but NOT modify PropertySet

**Optimization Levels:**
- Level 0: Disables optimizations; only hardware-required transformations
- Level 1-2: Moderate optimization
- Level 3: Full optimization suite

### Backend Abstraction

**Three-Level Architecture:**
1. Provider (Backend Repository): Manages groups of backends
2. Backend Interface (Versioned): BackendV1 (legacy), BackendV2 (current)
3. Job (Execution Result): Returns from `backend.run()`

**BackendV2 Extensions:**
- `get_translation_stage_plugin()`: Custom basis gate translation
- `get_scheduling_stage_plugin()`: Custom scheduling implementation

### Extension Points

- TranspilerStagePlugin via Python entry points
- Backend stage plugins
- Primitive system (BaseSamplerV2/BaseEstimatorV2)
- Provider system via BaseProvider interface

### Key Takeaways for HIQ

1. DAG-based IR is proven and effective
2. Staged pass manager provides clean separation
3. PropertySet pattern enables inter-pass communication
4. Plugin/entry point system enables extensibility
5. Rust already used for performance-critical paths

## Qrisp Architecture Analysis

### High-Level Abstractions

**QuantumVariable System**
- Central data structure hiding qubit management
- Enables human-readable I/O and strong typing via class inheritance
- Each QuantumVariable registered to exactly one QuantumSession

**Advanced Type System:**
- QuantumFloat — arbitrary precision quantum numbers
- QuantumBool — boolean values
- QuantumChar — characters
- QuantumString — strings
- QuantumArray — NumPy-like slicing and reshaping
- QuantumModulus — modular arithmetic

### Internal IR

**Permeability DAG**
- Nodes: Quantum operations (gates, measurements, allocations/deallocations)
- Edges: Dependencies between operations
- Anti-dependency edges: Connect non-permeable gates

**Key Concepts:**
- Permeability: Gate commutes with Z operator on that qubit
- Qfree property: Gate neither introduces nor destroys superposition states
- Enables optimal gate reordering and parallelization

### Automatic Uncomputation

**Features:**
- `@auto_uncompute` decorator uncomputes all local QuantumVariables
- Replaces multi-controlled X-gates with phase-tolerant variants
- Examines combined gates for qfree property
- Recomputation option trades qubit usage vs. circuit depth

**Unqomp Algorithm (Improved):**
- Based on Paradis et al.'s Unqomp
- Enhanced for high-level programming framework
- Automatically determines qfree-ness and permeability
- Synthesizes uncomputation circuits

### Backend Interface

**REST-based Design:**
- BackendServer: Creates `run_func` accepting QuantumCircuit, shots, token
- BackendClient: Connects to remote BackendServers
- VirtualBackend: External circuit dispatching code locally

**Specialized Backends:**
- QiskitBackend — Qiskit simulators and IBM Quantum
- IQMBackend — IQM quantum computers (up to 20,000 shots)
- AQTBackend — AQT Cloud systems (up to 2,000 shots)

### Key Differentiators from Qiskit

| Aspect | Qrisp | Qiskit |
|--------|-------|--------|
| Programming Model | Variables and functions | Direct gate application |
| Abstraction Level | High-level, variables-based | Mid-level, circuit-based |
| Qubit Management | Automatic, hidden | Manual, explicit |
| Uncomputation | Automatic | Manual |
| Circuit Efficiency | Leverages code structure | Gate-level only |

### Key Takeaways for HIQ

1. High-level types valuable but large scope
2. Permeability DAG enables advanced optimizations
3. Automatic uncomputation is research-heavy
4. REST-based backend interface is clean and portable
5. Qrisp already supports IQM natively

## Quantum IR Standards Landscape

### OpenQASM 3

**Status:**
- Version 3.0 finalized, 3.1 live specification
- Industry standard for quantum assembly language

**Features:**
- Extends OpenQASM with classical logic, control flow, data types
- Gate timing, external pulse-level grammar
- Real-time quantum-classical interactions

**Limitations:**
- Incomplete adoption across vendors
- Requires traditional compiler optimizations on top
- Reinvents concepts from classical compilers

### QIR (Quantum Intermediate Representation)

**Design:**
- Based on LLVM IR, not custom quantum IR
- Hardware and language agnostic
- Qubits as pointers to opaque structure type `%Qubit*`
- Supports complex control flow, loops, conditionals

**Alliance Members:**
- Microsoft, NVIDIA, Oak Ridge National Laboratory
- Quantinuum, Quantum Circuits Inc., Rigetti Computing

**Challenges:**
- Low adoption outside industrial players
- Integration into existing compilers challenging
- Dynamic qubit addressing mechanisms complex

### Other IRs

**Quil (Rigetti):**
- Open standard, treats quantum computers as coprocessors
- Quil-T adds pulse-level analog control
- Natively supported on Rigetti QPUs

**XACC:**
- Extensible compilation framework for hybrid quantum-classical
- Eclipse Science project
- Supports IBM, Rigetti, IonQ, D-Wave QPUs

### Vendor-Specific Approaches

**IBM (Qiskit):**
- DAGCircuit IR (graph-based)
- PassManager-based transpilation
- 83x faster transpilation than Tket 2.6.0 (2025)
- AI-powered transpiler service

**IQM:**
- MLIR-based framework
- Just-In-Time LLVM-based compiler
- Munich Quantum Software Stack (MQSS)
- Pulse-level access via IQM Resonance

**IonQ:**
- Modular, containerized architecture
- Software-configurable quantum computer
- All-to-all gate operations (no routing needed)

### Identified Gaps

1. **Distributed Quantum Computing** — NetQIR not production-ready
2. **HPC Integration** — Ad-hoc middleware, no standards
3. **Multi-Backend Compilation** — No unified heterogeneous QPU standard
4. **Resource Management** — Quantum resources not standardized in HPC
5. **Hybrid Workflow Semantics** — Incomplete quantum-classical integration spec

## Strategic Implications for HIQ

### Why Not Just Use Qiskit?

1. Python-first limits HPC deployment (GC, startup time)
2. IBM-centric despite multi-backend support
3. Large dependency footprint for HPC nodes
4. Algorithm focus vs. orchestration focus

### Why Not Just Use Qrisp?

1. Python-only implementation
2. Research framework, not production-focused
3. Limited HPC scheduler integration
4. Primarily academic community

### Arvak's Niche

1. **Rust-native** — Performance, single binary, no GC
2. **HPC-first** — Slurm/PBS integration as primary use case
3. **Compilation-focused** — Not algorithms or chemistry
4. **IQM + European HPC** — Underserved market segment
5. **Complementary** — Works with Qiskit/Qrisp, not replacing

### Design Decisions Informed by Research

| Decision | Rationale |
|----------|-----------|
| DAG-based IR | Proven effective (Qiskit), enables optimization |
| Staged pass manager | Clean separation, extensible |
| PropertySet pattern | Inter-pass communication (Qiskit) |
| Rust core + Python bindings | Performance + ecosystem access |
| OpenQASM 3 as exchange format | Industry standard, portable |
| Plugin-based backends | Isolation, independent versioning |
| REST-based backend interface | Clean, portable (Qrisp) |
| IQM as first QPU target | European HPC alignment |

## References

### Qiskit
- [Qiskit Repository](https://github.com/Qiskit/qiskit)
- [IBM Quantum Documentation](https://docs.quantum.ibm.com/)
- [Qiskit RFC Repository](https://github.com/Qiskit/RFCs)

### Qrisp
- [Qrisp Repository](https://github.com/eclipse-qrisp/Qrisp)
- [Qrisp Documentation](https://qrisp.eu/)
- [Qrisp arXiv Paper](https://arxiv.org/abs/2406.14792)

### Standards
- [OpenQASM Specification](https://openqasm.com/)
- [QIR Alliance](https://www.qir-alliance.org/)
- [XACC Framework](https://github.com/eclipse-xacc/xacc)

### HPC Integration
- [Interfacing Quantum Computing with HPC](https://arxiv.org/html/2509.06205v1)
- [Integrating Quantum Computers into HPC Survey](https://arxiv.org/pdf/2507.03540)
