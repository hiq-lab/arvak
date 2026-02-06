# Arvak Compilation Framework

## Overview

The Arvak compilation framework provides a pass-based transpilation system for transforming quantum circuits to target hardware. It follows the design patterns established by Qiskit while providing a Rust-native implementation.

## Compilation Pipeline

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Circuit   │────▶│ CircuitDag  │────▶│ PassManager │
│  (builder)  │     │    (IR)     │     │             │
└─────────────┘     └─────────────┘     └──────┬──────┘
                                               │
                    ┌──────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────┐
│                    Compilation Stages                    │
│                                                         │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐    │
│  │  Init   │─▶│ Layout  │─▶│ Routing │─▶│ Basis   │    │
│  └─────────┘  └─────────┘  └─────────┘  └─────────┘    │
│                                               │         │
│                                               ▼         │
│                                         ┌─────────┐    │
│                                         │Optimize │    │
│                                         └─────────┘    │
└─────────────────────────────────────────────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │ Compiled Circuit│
                    └─────────────────┘
```

## Pass Types

### Analysis Pass

Reads the circuit and writes analysis results to PropertySet. Does not modify the DAG.

```rust
pub trait AnalysisPass: Pass {
    fn analyze(&self, dag: &CircuitDag, properties: &mut PropertySet) -> CompileResult<()>;
}
```

### Transformation Pass

Modifies the CircuitDag to achieve a specific transformation goal.

```rust
pub trait TransformationPass: Pass {
    fn transform(&self, dag: &mut CircuitDag, properties: &PropertySet) -> CompileResult<()>;
}
```

### Pass Trait

Common interface for all passes.

```rust
pub trait Pass: Send + Sync {
    fn name(&self) -> &str;
    fn kind(&self) -> PassKind;
    fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()>;
    fn should_run(&self, dag: &CircuitDag, properties: &PropertySet) -> bool { true }
}

pub enum PassKind {
    Analysis,
    Transformation,
}
```

## PropertySet

Shared state for inter-pass communication.

```rust
pub struct PropertySet {
    pub layout: Option<Layout>,
    pub coupling_map: Option<CouplingMap>,
    pub basis_gates: Option<BasisGates>,
    // Custom properties via TypeId
}
```

### Layout

Mapping from logical qubits to physical qubits.

```rust
pub struct Layout {
    logical_to_physical: HashMap<QubitId, u32>,
    physical_to_logical: HashMap<u32, QubitId>,
}

impl Layout {
    fn add(&mut self, logical: QubitId, physical: u32);
    fn get_physical(&self, logical: QubitId) -> Option<u32>;
    fn get_logical(&self, physical: u32) -> Option<QubitId>;
    fn swap(&mut self, p1: u32, p2: u32);
}
```

### CouplingMap

Target device qubit connectivity.

```rust
pub struct CouplingMap {
    edges: Vec<(u32, u32)>,
    num_qubits: u32,
}

impl CouplingMap {
    fn linear(n: u32) -> Self;      // 0-1-2-3-...
    fn full(n: u32) -> Self;        // All-to-all
    fn star(n: u32) -> Self;        // 0 connected to all (IQM)
    fn is_connected(&self, q1: u32, q2: u32) -> bool;
}
```

### BasisGates

Target device native gate set.

```rust
pub struct BasisGates {
    gates: Vec<String>,
}

impl BasisGates {
    fn iqm() -> Self;   // ["prx", "cz", "measure", "barrier"]
    fn ibm() -> Self;   // ["id", "rz", "sx", "x", "cx", "measure", "barrier"]
    fn contains(&self, gate: &str) -> bool;
}
```

## PassManager

Orchestrates pass execution.

```rust
pub struct PassManager {
    passes: Vec<Box<dyn Pass>>,
}

impl PassManager {
    fn new() -> Self;
    fn add_pass(&mut self, pass: impl Pass + 'static);
    fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()>;
}
```

### PassManagerBuilder

Convenient construction with presets.

```rust
let properties = PropertySet::new()
    .with_target(
        CouplingMap::star(5),
        BasisGates::iqm(),
    );

let (pm, props) = PassManagerBuilder::new()
    .with_optimization_level(2)
    .with_properties(properties)
    .build();

let mut dag = circuit.into_dag();
pm.run(&mut dag, &mut props)?;
```

### Optimization Levels

| Level | Description |
|-------|-------------|
| 0 | No optimization, only required transformations |
| 1 | Light optimization (default) |
| 2 | Moderate optimization |
| 3 | Heavy optimization (potentially expensive) |

## Built-in Passes

### Init Stage

#### RemoveBarriers

Removes barrier instructions that would prevent optimization.

```rust
pub struct RemoveBarriers;
```

### Layout Stage

#### TrivialLayout

Maps logical qubit i to physical qubit i.

```rust
pub struct TrivialLayout;

// Result: logical qubit 0 → physical qubit 0
//         logical qubit 1 → physical qubit 1
//         ...
```

#### DenseLayout (Planned)

Selects physical qubits to minimize circuit depth based on error rates and connectivity.

### Routing Stage

#### BasicRouting

Simple routing that checks connectivity constraints. Errors if qubits aren't connected (placeholder for full implementation).

```rust
pub struct BasicRouting;
```

#### SabreRouting (Planned)

SWAP-based routing using the SABRE algorithm.

- Heuristic lookahead for SWAP selection
- Bidirectional pass for improved results
- Parameterized by lookahead depth

### Translation Stage

#### BasisTranslation

Decomposes gates to target basis gate set.

```rust
pub struct BasisTranslation;
```

**Decomposition Rules (IQM basis):**

| Source Gate | Decomposition |
|-------------|---------------|
| H | PRX(π/2, -π/2) · PRX(π, 0) |
| X | PRX(π, 0) |
| Y | PRX(π, π/2) |
| Rx(θ) | PRX(θ, 0) |
| Ry(θ) | PRX(θ, π/2) |
| CX | H · CZ · H (on target) |

### Optimization Stage

#### Optimize1qGates

Merges consecutive single-qubit gates on the same qubit.

```rust
pub struct Optimize1qGates;

// Before: Rz(π/4) · Rz(π/4) · Rx(π/2)
// After:  Rz(π/2) · Rx(π/2)
```

#### CancelCx

Cancels adjacent CX gates on the same qubits.

```rust
pub struct CancelCx;

// Before: CX(a,b) · CX(a,b)
// After:  (identity, removed)
```

#### CommutativeAnalysis (Planned)

Finds commutation relations to enable gate reordering.

#### ConsolidateBlocks (Planned)

Consolidates sequences of gates into optimal decompositions.

## Custom Passes

### Implementing a Custom Pass

```rust
use hiq_compile::{Pass, PassKind, PropertySet, CompileResult};
use hiq_ir::dag::CircuitDag;

pub struct MyCustomPass {
    threshold: f64,
}

impl Pass for MyCustomPass {
    fn name(&self) -> &str {
        "MyCustomPass"
    }

    fn kind(&self) -> PassKind {
        PassKind::Transformation
    }

    fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()> {
        // Access target properties
        let basis = properties.basis_gates.as_ref();

        // Iterate over operations
        for (node, instr) in dag.topological_ops() {
            // Transform as needed
        }

        Ok(())
    }

    fn should_run(&self, dag: &CircuitDag, properties: &PropertySet) -> bool {
        // Conditionally skip this pass
        dag.num_ops() > 10
    }
}
```

### Adding Custom Pass to Manager

```rust
let mut pm = PassManager::new();
pm.add_pass(RemoveBarriers);
pm.add_pass(TrivialLayout);
pm.add_pass(MyCustomPass { threshold: 0.5 });
pm.add_pass(BasisTranslation);
```

## Target-Specific Compilation

### IQM Compilation

```rust
let iqm_properties = PropertySet::new()
    .with_target(
        CouplingMap::star(5),  // 5-qubit star topology
        BasisGates::iqm(),     // PRX, CZ
    );

let (pm, mut props) = PassManagerBuilder::new()
    .with_optimization_level(2)
    .with_properties(iqm_properties)
    .build();
```

### IBM Compilation

```rust
let ibm_properties = PropertySet::new()
    .with_target(
        CouplingMap::linear(5),  // Example linear topology
        BasisGates::ibm(),       // id, rz, sx, x, cx
    );

let (pm, mut props) = PassManagerBuilder::new()
    .with_optimization_level(2)
    .with_properties(ibm_properties)
    .build();
```

## Error Handling

```rust
pub enum CompileError {
    Ir(IrError),
    MissingCouplingMap,
    MissingLayout,
    RoutingFailed { qubit1: u32, qubit2: u32 },
    GateNotInBasis(String),
    PassFailed(String, String),
}
```

### Handling Compilation Errors

```rust
match pm.run(&mut dag, &mut props) {
    Ok(()) => println!("Compilation successful"),
    Err(CompileError::RoutingFailed { qubit1, qubit2 }) => {
        eprintln!("Cannot route: qubits {} and {} not connected", qubit1, qubit2);
    }
    Err(CompileError::GateNotInBasis(gate)) => {
        eprintln!("Gate '{}' not supported by target", gate);
    }
    Err(e) => eprintln!("Compilation failed: {}", e),
}
```

## Performance Considerations

1. **Pass Ordering** — Order passes to minimize repeated work
2. **Conditional Execution** — Use `should_run()` to skip unnecessary passes
3. **DAG Operations** — Batch node modifications when possible
4. **Parallelization** — Some passes can run in parallel (future work)

## Future Passes

| Pass | Stage | Description |
|------|-------|-------------|
| SabreLayout | Layout | Error-aware qubit selection |
| SabreRouting | Routing | SABRE SWAP insertion |
| GateDirection | Translation | Respect native gate directions |
| RemoveResetInZeroState | Optimization | Remove resets on |0⟩ |
| OptimizeSwapBeforeMeasure | Optimization | Eliminate SWAPs before measurement |
| ContractIdleWires | Optimization | Remove unused qubits |
| PulseSchedule | Scheduling | Pulse-level timing |
