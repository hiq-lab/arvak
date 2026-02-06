# Arvak Roadmap

## Vision

Arvak aims to be the Rust-native quantum compilation and orchestration stack for HPC environments, providing fast compilation, first-class HPC scheduler integration, and unified access to quantum backends.

## Strategic Positioning

### What Arvak Is

- A Rust-native compilation core for performance-critical quantum workflows
- An HPC-first orchestration layer for quantum job management
- A unified interface for heterogeneous quantum backends
- A complement to existing quantum SDKs (Qiskit, Qrisp)

### What Arvak Is Not

- A replacement for Qiskit or Qrisp at the algorithm level
- A quantum algorithm library
- A quantum chemistry or optimization framework
- A visual circuit editor or notebook environment

## Phase 1: Foundation (Months 1-3)

### Goals
- Establish core IR and basic compilation
- Validate Rust tooling for quantum circuits
- Demonstrate end-to-end workflow

### Deliverables

| Milestone | Deliverable | Description |
|-----------|-------------|-------------|
| M1.1 | hiq-ir v0.1 | Circuit DAG, gates, instructions |
| M1.2 | hiq-qasm3 v0.1 | QASM3 parser (core subset) |
| M1.3 | hiq-hal v0.1 | Backend trait, job management |
| M1.4 | hiq-adapter-sim v0.1 | Local simulator |
| M1.5 | hiq-cli v0.1 | Basic commands (compile, run) |

### Success Criteria
- Parse QASM3 circuit
- Simulate locally
- Retrieve results via CLI

### Technical Milestones

```
Week 1-4:   Circuit IR design and implementation
Week 5-6:   QASM3 lexer and parser
Week 7-8:   Simulator backend
Week 9-10:  CLI skeleton
Week 11-12: Integration testing, documentation
```

## Phase 2: Compilation + First QPU (Months 4-6)

### Goals
- Implement core transpilation passes
- Connect to real quantum hardware (IQM)
- Validate HPC deployment model

### Deliverables

| Milestone | Deliverable | Description |
|-----------|-------------|-------------|
| M2.1 | hiq-compile v0.1 | Pass manager, layout, routing |
| M2.2 | BasisTranslation | IQM and IBM basis gate decomposition |
| M2.3 | hiq-adapter-iqm v0.1 | IQM Resonance cloud adapter |
| M2.4 | hiq-sched v0.1 | Slurm adapter |
| M2.5 | hiq-python v0.1 | PyO3 bindings (basic) |

### Success Criteria
- Compile circuit for IQM topology
- Submit to IQM Resonance
- Retrieve results
- Run via Slurm on test cluster

### Technical Milestones

```
Week 13-14: Pass manager infrastructure
Week 15-16: Layout and routing passes
Week 17-18: Basis translation (IQM)
Week 19-20: IQM adapter implementation
Week 21-22: Slurm integration
Week 23-24: Python bindings, testing
```

## Phase 3: HPC Integration (Months 7-9)

### Goals
- Production-ready HPC deployment
- Second backend (IBM)
- LUMI validation

### Deliverables

| Milestone | Deliverable | Description |
|-----------|-------------|-------------|
| M3.1 | hiq-adapter-ibm v0.1 | IBM Quantum adapter |
| M3.2 | IQM LUMI support | On-premise OIDC auth |
| M3.3 | Large circuit handling | 100+ qubit circuits |
| M3.4 | PBS adapter | PBS Pro support |
| M3.5 | Documentation | HPC deployment guides |

### Success Criteria
- Run on LUMI (IQM Helmi)
- IBM Quantum cloud access
- Handle 100+ qubit simulations
- Complete HPC deployment documentation

### Technical Milestones

```
Week 25-26: IBM adapter
Week 27-28: IQM OIDC authentication
Week 29-30: Large circuit optimizations
Week 31-32: PBS adapter
Week 33-34: LUMI testing
Week 35-36: Documentation, bug fixes
```

## Phase 4: Production + Community (Months 10-12)

### Goals
- 1.0 release
- Optimization passes
- Community building

### Deliverables

| Milestone | Deliverable | Description |
|-----------|-------------|-------------|
| M4.1 | Optimization passes | 1q gate merge, CX cancellation |
| M4.2 | hiq-types v0.1 | QuantumFloat, QuantumBool |
| M4.3 | hiq-auto v0.1 | Basic uncomputation |
| M4.4 | Benchmarks | Performance comparison |
| M4.5 | v1.0 release | Stable API |

### Success Criteria
- Measurable compilation speedup vs. Qiskit
- Stable API with semantic versioning
- Active community engagement
- Conference presentation

### Technical Milestones

```
Week 37-38: Optimization passes
Week 39-40: High-level types
Week 41-42: Uncomputation basics
Week 43-44: Benchmarking
Week 45-46: API stabilization
Week 47-48: Release, documentation
```

## Future Directions (Post 1.0)

### Near-term (Year 2)

| Feature | Description | Priority |
|---------|-------------|----------|
| SABRE routing | State-of-the-art routing algorithm | High |
| QIR support | LLVM-based IR integration | High |
| Noise-aware compilation | Error rate optimization | Medium |
| Circuit cutting | Large circuit partitioning | Medium |
| More backends | IonQ, Rigetti, etc. | Medium |

### Medium-term (Year 2-3)

| Feature | Description | Priority |
|---------|-------------|----------|
| Distributed quantum | Multi-QPU coordination | High |
| Pulse-level control | Quil-T style access | Medium |
| Error mitigation | ZNE, PEC passes | Medium |
| Full Qrisp parity | Complete uncomputation | Low |

### Long-term (Year 3+)

| Feature | Description |
|---------|-------------|
| Fault-tolerant compilation | Logical qubit compilation |
| Quantum networking | Distributed entanglement |
| AI-assisted optimization | ML-based pass selection |

## Resource Requirements

### Team

| Phase | Engineers | Focus |
|-------|-----------|-------|
| 1 | 1-2 | Core IR, parser |
| 2 | 2-3 | Compilation, backends |
| 3 | 2-3 | HPC integration |
| 4 | 2-3 | Optimization, release |

### Infrastructure

- CI/CD: GitHub Actions
- Testing: Local + HPC cluster access
- Backends: IQM Resonance, IBM Quantum credits
- Documentation: GitHub Pages

### Dependencies

| External | Risk | Mitigation |
|----------|------|------------|
| IQM API stability | Medium | Version pinning, adapter abstraction |
| IBM API changes | Medium | Adapter abstraction |
| LUMI access | Low | Partnership with CSC |
| Rust ecosystem | Low | Stable dependencies |

## Risks and Mitigations

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Backend API changes | High | Medium | Thin adapter layer, version pinning |
| HPC environment constraints | Medium | Medium | Static binaries, minimal dependencies |
| Team capacity | High | Medium | Strict scope management |
| Community adoption | Medium | Medium | Good docs, Rust community engagement |
| Qiskit competition | Low | High | Focus on HPC niche, complementary positioning |

## Success Metrics

### Technical

- Compilation time vs. Qiskit (target: 5-10x faster)
- Memory usage for large circuits
- Test coverage > 80%
- Documentation coverage

### Community

- GitHub stars
- Contributors
- PyPI downloads
- Conference presentations

### Adoption

- HPC center deployments
- Research papers using HIQ
- Industry partnerships

## Review Schedule

- Weekly: Team sync
- Monthly: Milestone review
- Quarterly: Roadmap adjustment
- Annually: Strategic review
