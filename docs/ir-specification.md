# Arvak IR Specification

## Overview

The Arvak Intermediate Representation (IR) is a DAG-based circuit representation optimized for compilation and transformation passes. It draws inspiration from Qiskit's DAGCircuit while providing a Rust-native implementation.

## Core Types

### QubitId and ClbitId

Unique identifiers for quantum and classical bits within a circuit.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QubitId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClbitId(pub u32);
```

### Qubit and Clbit

Quantum and classical bits with optional register membership.

```rust
pub struct Qubit {
    pub id: QubitId,
    pub register: Option<String>,
    pub index: Option<u32>,
}

pub struct Clbit {
    pub id: ClbitId,
    pub register: Option<String>,
    pub index: Option<u32>,
}
```

## Gate Representation

### StandardGate

Enumeration of standard gates with known semantics.

```rust
pub enum StandardGate {
    // Single-qubit Pauli gates
    I, X, Y, Z,

    // Single-qubit Clifford gates
    H, S, Sdg, T, Tdg, SX, SXdg,

    // Single-qubit rotation gates
    Rx(ParameterExpression),
    Ry(ParameterExpression),
    Rz(ParameterExpression),
    P(ParameterExpression),
    U(ParameterExpression, ParameterExpression, ParameterExpression),

    // Two-qubit gates
    CX, CY, CZ, CH, Swap, ISwap,
    CRx(ParameterExpression),
    CRy(ParameterExpression),
    CRz(ParameterExpression),
    CP(ParameterExpression),
    RXX(ParameterExpression),
    RYY(ParameterExpression),
    RZZ(ParameterExpression),

    // Three-qubit gates
    CCX, CSwap,

    // IQM native gates
    PRX(ParameterExpression, ParameterExpression),
}
```

### Gate Qubit Counts

| Gate | Qubits | Notes |
|------|--------|-------|
| I, X, Y, Z, H, S, T, etc. | 1 | Pauli and Clifford |
| Rx, Ry, Rz, P, U, PRX | 1 | Parameterized single-qubit |
| CX, CY, CZ, Swap, etc. | 2 | Two-qubit gates |
| CCX, CSwap | 3 | Three-qubit gates |

### CustomGate

User-defined or decomposed gates.

```rust
pub struct CustomGate {
    pub name: String,
    pub num_qubits: u32,
    pub params: Vec<ParameterExpression>,
    pub matrix: Option<Vec<Complex64>>,
}
```

### GateKind

Either standard or custom.

```rust
pub enum GateKind {
    Standard(StandardGate),
    Custom(CustomGate),
}
```

### Gate

A gate with associated metadata.

```rust
pub struct Gate {
    pub kind: GateKind,
    pub label: Option<String>,
    pub condition: Option<ClassicalCondition>,
}

pub struct ClassicalCondition {
    pub register: String,
    pub value: u64,
}
```

## Parameter Expressions

Symbolic or concrete parameter values.

```rust
pub enum ParameterExpression {
    Constant(f64),
    Symbol(String),
    Pi,
    Neg(Box<ParameterExpression>),
    Add(Box<ParameterExpression>, Box<ParameterExpression>),
    Sub(Box<ParameterExpression>, Box<ParameterExpression>),
    Mul(Box<ParameterExpression>, Box<ParameterExpression>),
    Div(Box<ParameterExpression>, Box<ParameterExpression>),
}
```

### Parameter Operations

| Method | Description |
|--------|-------------|
| `is_symbolic()` | Check if contains symbolic parameters |
| `as_f64()` | Get concrete value if not symbolic |
| `symbols()` | Collect all symbolic parameter names |
| `bind(name, value)` | Substitute a parameter value |

### Examples

```rust
use arvak_ir::parameter::ParameterExpression;
use std::f64::consts::PI;

// Constant
let angle = ParameterExpression::constant(PI / 2.0);

// Symbolic
let theta = ParameterExpression::symbol("theta");

// Expression: theta * pi / 2
let expr = theta.clone() * ParameterExpression::pi() / ParameterExpression::constant(2.0);

// Bind value
let bound = expr.bind("theta", 0.5);
assert!(!bound.is_symbolic());
```

## Instructions

### InstructionKind

The kind of instruction in a circuit.

```rust
pub enum InstructionKind {
    Gate(Gate),
    Measure,
    Reset,
    Barrier,
    Delay { duration: u64 },
}
```

### Instruction

A complete instruction with operands.

```rust
pub struct Instruction {
    pub kind: InstructionKind,
    pub qubits: Vec<QubitId>,
    pub clbits: Vec<ClbitId>,
}
```

### Instruction Constructors

```rust
// Gate instruction
Instruction::gate(Gate::standard(StandardGate::H), [q0])

// Measurement
Instruction::measure(q0, c0)

// Multi-qubit measurement
Instruction::measure_all([q0, q1], [c0, c1])

// Reset
Instruction::reset(q0)

// Barrier
Instruction::barrier([q0, q1, q2])

// Delay
Instruction::delay(q0, 100)
```

## Circuit DAG

### DagNode

A node in the circuit DAG.

```rust
pub enum DagNode {
    In(WireId),   // Input node for a qubit or clbit
    Out(WireId),  // Output node for a qubit or clbit
    Op(Instruction), // An instruction
}

pub enum WireId {
    Qubit(QubitId),
    Clbit(ClbitId),
}
```

### CircuitDag

DAG-based circuit representation.

```rust
pub struct CircuitDag {
    graph: DiGraph<DagNode, DagEdge>,
    qubit_inputs: HashMap<QubitId, NodeIndex>,
    qubit_outputs: HashMap<QubitId, NodeIndex>,
    clbit_inputs: HashMap<ClbitId, NodeIndex>,
    clbit_outputs: HashMap<ClbitId, NodeIndex>,
    global_phase: f64,
}
```

### DAG Structure

```
     In(q0)          In(q1)          In(c0)          In(c1)
       │               │               │               │
       │               │               │               │
       ▼               │               │               │
    ┌─────┐            │               │               │
    │  H  │            │               │               │
    └──┬──┘            │               │               │
       │               │               │               │
       └───────┬───────┘               │               │
               │                       │               │
               ▼                       │               │
           ┌──────┐                    │               │
           │  CX  │                    │               │
           └──┬───┘                    │               │
              │                        │               │
       ┌──────┴──────┐                 │               │
       │             │                 │               │
       ▼             ▼                 │               │
   ┌───────┐     ┌───────┐            │               │
   │Measure│     │Measure│            │               │
   └───┬───┘     └───┬───┘            │               │
       │             │                 │               │
       │             │                 │               │
       ▼             ▼                 ▼               ▼
    Out(q0)       Out(q1)          Out(c0)         Out(c1)
```

### DAG Operations

| Method | Description |
|--------|-------------|
| `add_qubit(id)` | Add a qubit to the DAG |
| `add_clbit(id)` | Add a classical bit to the DAG |
| `apply(instruction)` | Apply an instruction to the DAG |
| `topological_ops()` | Get operations in topological order |
| `get_instruction(node)` | Get instruction at a node |
| `remove_op(node)` | Remove an operation node |
| `substitute_node(node, replacement)` | Replace a node with multiple instructions |
| `num_qubits()` | Number of qubits |
| `num_ops()` | Number of operations |
| `depth()` | Circuit depth |

## Circuit Builder

High-level circuit construction API.

```rust
pub struct Circuit {
    name: String,
    qubits: Vec<Qubit>,
    clbits: Vec<Clbit>,
    dag: CircuitDag,
}
```

### Construction

```rust
// Empty circuit
let circuit = Circuit::new("my_circuit");

// With size
let circuit = Circuit::with_size("my_circuit", 5, 5);

// Add registers
let mut circuit = Circuit::new("my_circuit");
let qreg = circuit.add_qreg("q", 3);  // Returns [QubitId(0), QubitId(1), QubitId(2)]
let creg = circuit.add_creg("c", 3);  // Returns [ClbitId(0), ClbitId(1), ClbitId(2)]
```

### Gate Application

```rust
let mut circuit = Circuit::with_size("test", 2, 2);
let q0 = QubitId(0);
let q1 = QubitId(1);
let c0 = ClbitId(0);
let c1 = ClbitId(1);

circuit
    .h(q0)?           // Hadamard
    .cx(q0, q1)?      // CNOT
    .rz(PI/4.0, q0)?  // Rz rotation
    .measure(q0, c0)? // Measurement
    .measure(q1, c1)?;
```

### Pre-built Circuits

```rust
// Bell state
let bell = Circuit::bell()?;

// GHZ state
let ghz = Circuit::ghz(5)?;

// Quantum Fourier Transform
let qft = Circuit::qft(4)?;
```

## Matrix Representation

### Matrix2x2

2x2 complex matrix for single-qubit gates.

```rust
pub struct Matrix2x2 {
    pub data: [[Complex64; 2]; 2],
}
```

### Methods

| Method | Description |
|--------|-------------|
| `new(a00, a01, a10, a11)` | Create from elements |
| `identity()` | Identity matrix |
| `multiply(&self, other)` | Matrix multiplication |
| `adjoint()` | Conjugate transpose |
| `is_unitary(tolerance)` | Check unitarity |

### Standard Gate Matrices

```rust
// Pauli X
let x = Matrix2x2::new(
    Complex64::new(0.0, 0.0), Complex64::new(1.0, 0.0),
    Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0),
);

// Hadamard
let sqrt2_inv = 1.0 / 2.0_f64.sqrt();
let h = Matrix2x2::new(
    Complex64::new(sqrt2_inv, 0.0), Complex64::new(sqrt2_inv, 0.0),
    Complex64::new(sqrt2_inv, 0.0), Complex64::new(-sqrt2_inv, 0.0),
);
```

## Serialization

All IR types implement `Serialize` and `Deserialize` for persistence.

```rust
use serde_json;

let circuit = Circuit::bell()?;
let json = serde_json::to_string(&circuit)?;
let loaded: Circuit = serde_json::from_str(&json)?;
```

## Error Handling

```rust
pub enum IrError {
    QubitNotFound(QubitId),
    ClbitNotFound(ClbitId),
    InvalidDag,
    InvalidNode,
    QubitCountMismatch { expected: u32, got: u32 },
    UnboundParameter(String),
    ParameterizedCircuit,
}

pub type IrResult<T> = Result<T, IrError>;
```

## IQM Native Gates

Arvak includes IQM's native gate set for efficient compilation.

### PRX Gate

Phased rotation around X-axis: PRX(θ, φ)

```
PRX(θ, φ) = Rz(φ) · Rx(θ) · Rz(-φ)
```

| Standard Gate | PRX Decomposition |
|---------------|-------------------|
| X | PRX(π, 0) |
| Y | PRX(π, π/2) |
| Rx(θ) | PRX(θ, 0) |
| Ry(θ) | PRX(θ, π/2) |
| H | PRX(π/2, -π/2) · PRX(π, 0) |

### CZ Gate

Native two-qubit gate for IQM devices. CNOT is decomposed as:

```
CX(control, target) = H(target) · CZ(control, target) · H(target)
```

## Performance Considerations

1. **SmallVec** — Used for qubit/clbit lists (typically ≤3 elements)
2. **FxHashMap** — Fast hashing for internal maps
3. **Arena Allocation** — Considered for DAG nodes (future)
4. **Lazy Evaluation** — Parameter expressions evaluated on demand
