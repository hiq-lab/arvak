# HIQ Code Specification

## Workspace Configuration

### Cargo.toml (Root)

```toml
[workspace]
resolver = "2"
members = [
    "crates/*",
    "adapters/*",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.83"
license = "MIT OR Apache-2.0"
repository = "https://github.com/hiq-project/hiq"
keywords = ["quantum", "hpc", "compiler", "qasm", "orchestration"]
categories = ["science", "compilers", "simulation"]

[workspace.dependencies]
# Internal crates
hiq-ir = { path = "crates/hiq-ir" }
hiq-qasm3 = { path = "crates/hiq-qasm3" }
hiq-compile = { path = "crates/hiq-compile" }
hiq-auto = { path = "crates/hiq-auto" }
hiq-types = { path = "crates/hiq-types" }
hiq-hal = { path = "crates/hiq-hal" }
hiq-sched = { path = "crates/hiq-sched" }
hiq-core = { path = "crates/hiq-core" }

# Async runtime
tokio = { version = "1.43", features = ["full"] }
async-trait = "0.1"
futures = "0.3"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"

# Graph algorithms
petgraph = "0.7"

# Numeric
num-complex = "0.4"
ndarray = "0.16"
nalgebra = "0.33"

# Parsing
logos = "0.14"
chumsky = "0.9"

# Error handling
thiserror = "2.0"
anyhow = "1.0"
miette = { version = "7.0", features = ["fancy"] }

# CLI
clap = { version = "4.5", features = ["derive", "env"] }
indicatif = "0.17"
console = "0.15"

# HTTP
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }

# Python bindings
pyo3 = { version = "0.23", features = ["extension-module"] }

# Testing
proptest = "1.5"
criterion = "0.5"
rstest = "0.23"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[profile.release]
lto = "thin"
codegen-units = 1

[profile.bench]
lto = "thin"
```

## Core Type Definitions

### hiq-ir: Qubit Types

```rust
// crates/hiq-ir/src/qubit.rs

use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for a qubit within a circuit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QubitId(pub u32);

/// Unique identifier for a classical bit within a circuit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClbitId(pub u32);

/// A quantum bit with optional register membership.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Qubit {
    pub id: QubitId,
    pub register: Option<String>,
    pub index: Option<u32>,
}

/// A classical bit with optional register membership.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Clbit {
    pub id: ClbitId,
    pub register: Option<String>,
    pub index: Option<u32>,
}
```

### hiq-ir: Gate Types

```rust
// crates/hiq-ir/src/gate.rs

use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::parameter::ParameterExpression;

/// Standard gates with known semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// A quantum gate, either standard or custom.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GateKind {
    Standard(StandardGate),
    Custom(CustomGate),
}

/// A user-defined or decomposed gate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomGate {
    pub name: String,
    pub num_qubits: u32,
    pub params: Vec<ParameterExpression>,
    pub matrix: Option<Vec<Complex64>>,
}

/// A gate with associated metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gate {
    pub kind: GateKind,
    pub label: Option<String>,
    pub condition: Option<ClassicalCondition>,
}

/// Classical condition for conditional gates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassicalCondition {
    pub register: String,
    pub value: u64,
}
```

### hiq-ir: Parameter Expressions

```rust
// crates/hiq-ir/src/parameter.rs

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A symbolic or concrete parameter expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

impl ParameterExpression {
    pub fn constant(value: f64) -> Self;
    pub fn symbol(name: impl Into<String>) -> Self;
    pub fn pi() -> Self;
    pub fn is_symbolic(&self) -> bool;
    pub fn as_f64(&self) -> Option<f64>;
    pub fn symbols(&self) -> HashSet<String>;
    pub fn bind(&self, name: &str, value: f64) -> Self;
}
```

### hiq-ir: Instructions

```rust
// crates/hiq-ir/src/instruction.rs

use serde::{Deserialize, Serialize};

use crate::gate::Gate;
use crate::qubit::{QubitId, ClbitId};

/// The kind of instruction in a circuit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InstructionKind {
    Gate(Gate),
    Measure,
    Reset,
    Barrier,
    Delay { duration: u64 },
}

/// A complete instruction with operands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instruction {
    pub kind: InstructionKind,
    pub qubits: Vec<QubitId>,
    pub clbits: Vec<ClbitId>,
}

impl Instruction {
    pub fn gate(gate: Gate, qubits: impl IntoIterator<Item = QubitId>) -> Self;
    pub fn measure(qubit: QubitId, clbit: ClbitId) -> Self;
    pub fn measure_all(qubits: impl IntoIterator<Item = QubitId>, clbits: impl IntoIterator<Item = ClbitId>) -> Self;
    pub fn reset(qubit: QubitId) -> Self;
    pub fn barrier(qubits: impl IntoIterator<Item = QubitId>) -> Self;
    pub fn delay(qubit: QubitId, duration: u64) -> Self;
}
```

### hiq-ir: Circuit DAG

```rust
// crates/hiq-ir/src/dag.rs

use petgraph::graph::{DiGraph, NodeIndex as PetNodeIndex};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::instruction::Instruction;
use crate::qubit::{QubitId, ClbitId};
use crate::error::IrResult;

pub type NodeIndex = PetNodeIndex<u32>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DagNode {
    In(WireId),
    Out(WireId),
    Op(Instruction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WireId {
    Qubit(QubitId),
    Clbit(ClbitId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DagEdge {
    pub wire: WireId,
}

/// DAG-based circuit representation.
pub struct CircuitDag {
    graph: DiGraph<DagNode, DagEdge, u32>,
    qubit_inputs: FxHashMap<QubitId, NodeIndex>,
    qubit_outputs: FxHashMap<QubitId, NodeIndex>,
    clbit_inputs: FxHashMap<ClbitId, NodeIndex>,
    clbit_outputs: FxHashMap<ClbitId, NodeIndex>,
    global_phase: f64,
}

impl CircuitDag {
    pub fn new() -> Self;
    pub fn add_qubit(&mut self, qubit: QubitId);
    pub fn add_clbit(&mut self, clbit: ClbitId);
    pub fn apply(&mut self, instruction: Instruction) -> IrResult<NodeIndex>;
    pub fn topological_ops(&self) -> impl Iterator<Item = (NodeIndex, &Instruction)>;
    pub fn get_instruction(&self, node: NodeIndex) -> Option<&Instruction>;
    pub fn get_instruction_mut(&mut self, node: NodeIndex) -> Option<&mut Instruction>;
    pub fn remove_op(&mut self, node: NodeIndex) -> IrResult<Instruction>;
    pub fn substitute_node(&mut self, node: NodeIndex, replacement: impl IntoIterator<Item = Instruction>) -> IrResult<Vec<NodeIndex>>;
    pub fn num_qubits(&self) -> usize;
    pub fn num_clbits(&self) -> usize;
    pub fn num_ops(&self) -> usize;
    pub fn depth(&self) -> usize;
    pub fn qubits(&self) -> impl Iterator<Item = QubitId> + '_;
    pub fn clbits(&self) -> impl Iterator<Item = ClbitId> + '_;
}
```

### hiq-ir: Circuit Builder

```rust
// crates/hiq-ir/src/circuit.rs

use crate::dag::CircuitDag;
use crate::gate::{Gate, StandardGate};
use crate::parameter::ParameterExpression;
use crate::qubit::{Qubit, Clbit, QubitId, ClbitId};
use crate::error::IrResult;

/// A quantum circuit.
pub struct Circuit {
    name: String,
    qubits: Vec<Qubit>,
    clbits: Vec<Clbit>,
    dag: CircuitDag,
}

impl Circuit {
    // Construction
    pub fn new(name: impl Into<String>) -> Self;
    pub fn with_size(name: impl Into<String>, num_qubits: u32, num_clbits: u32) -> Self;
    pub fn add_qubit(&mut self) -> QubitId;
    pub fn add_qreg(&mut self, name: impl Into<String>, size: u32) -> Vec<QubitId>;
    pub fn add_clbit(&mut self) -> ClbitId;
    pub fn add_creg(&mut self, name: impl Into<String>, size: u32) -> Vec<ClbitId>;

    // Single-qubit gates
    pub fn h(&mut self, qubit: QubitId) -> IrResult<&mut Self>;
    pub fn x(&mut self, qubit: QubitId) -> IrResult<&mut Self>;
    pub fn y(&mut self, qubit: QubitId) -> IrResult<&mut Self>;
    pub fn z(&mut self, qubit: QubitId) -> IrResult<&mut Self>;
    pub fn s(&mut self, qubit: QubitId) -> IrResult<&mut Self>;
    pub fn t(&mut self, qubit: QubitId) -> IrResult<&mut Self>;
    pub fn rx(&mut self, theta: impl Into<ParameterExpression>, qubit: QubitId) -> IrResult<&mut Self>;
    pub fn ry(&mut self, theta: impl Into<ParameterExpression>, qubit: QubitId) -> IrResult<&mut Self>;
    pub fn rz(&mut self, theta: impl Into<ParameterExpression>, qubit: QubitId) -> IrResult<&mut Self>;

    // Two-qubit gates
    pub fn cx(&mut self, control: QubitId, target: QubitId) -> IrResult<&mut Self>;
    pub fn cz(&mut self, control: QubitId, target: QubitId) -> IrResult<&mut Self>;
    pub fn swap(&mut self, q1: QubitId, q2: QubitId) -> IrResult<&mut Self>;

    // IQM native gates
    pub fn prx(&mut self, theta: impl Into<ParameterExpression>, phi: impl Into<ParameterExpression>, qubit: QubitId) -> IrResult<&mut Self>;

    // Three-qubit gates
    pub fn ccx(&mut self, c1: QubitId, c2: QubitId, target: QubitId) -> IrResult<&mut Self>;

    // Other operations
    pub fn measure(&mut self, qubit: QubitId, clbit: ClbitId) -> IrResult<&mut Self>;
    pub fn measure_all(&mut self) -> IrResult<&mut Self>;
    pub fn reset(&mut self, qubit: QubitId) -> IrResult<&mut Self>;
    pub fn barrier(&mut self, qubits: impl IntoIterator<Item = QubitId>) -> IrResult<&mut Self>;
    pub fn barrier_all(&mut self) -> IrResult<&mut Self>;

    // Accessors
    pub fn name(&self) -> &str;
    pub fn num_qubits(&self) -> usize;
    pub fn num_clbits(&self) -> usize;
    pub fn depth(&self) -> usize;
    pub fn dag(&self) -> &CircuitDag;
    pub fn dag_mut(&mut self) -> &mut CircuitDag;
    pub fn into_dag(self) -> CircuitDag;

    // Pre-built circuits
    pub fn bell() -> IrResult<Self>;
    pub fn ghz(n: u32) -> IrResult<Self>;
    pub fn qft(n: u32) -> IrResult<Self>;
}
```

## Compilation Framework

### hiq-compile: Pass Trait

```rust
// crates/hiq-compile/src/pass.rs

use hiq_ir::dag::CircuitDag;
use crate::property::PropertySet;
use crate::error::CompileResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassKind {
    Analysis,
    Transformation,
}

/// A compilation pass that operates on a circuit DAG.
pub trait Pass: Send + Sync {
    fn name(&self) -> &str;
    fn kind(&self) -> PassKind;
    fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()>;
    fn should_run(&self, dag: &CircuitDag, properties: &PropertySet) -> bool { true }
}

/// Marker trait for analysis passes.
pub trait AnalysisPass: Pass {
    fn analyze(&self, dag: &CircuitDag, properties: &mut PropertySet) -> CompileResult<()>;
}

/// Marker trait for transformation passes.
pub trait TransformationPass: Pass {
    fn transform(&self, dag: &mut CircuitDag, properties: &PropertySet) -> CompileResult<()>;
}
```

### hiq-compile: PropertySet

```rust
// crates/hiq-compile/src/property.rs

use rustc_hash::FxHashMap;
use std::any::{Any, TypeId};
use hiq_ir::qubit::QubitId;

/// A mapping from logical qubits to physical qubits.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    logical_to_physical: FxHashMap<QubitId, u32>,
    physical_to_logical: FxHashMap<u32, QubitId>,
}

impl Layout {
    pub fn new() -> Self;
    pub fn add(&mut self, logical: QubitId, physical: u32);
    pub fn get_physical(&self, logical: QubitId) -> Option<u32>;
    pub fn get_logical(&self, physical: u32) -> Option<QubitId>;
    pub fn swap(&mut self, p1: u32, p2: u32);
}

/// Target device coupling map.
#[derive(Debug, Clone)]
pub struct CouplingMap {
    edges: Vec<(u32, u32)>,
    num_qubits: u32,
}

impl CouplingMap {
    pub fn new(num_qubits: u32) -> Self;
    pub fn add_edge(&mut self, q1: u32, q2: u32);
    pub fn is_connected(&self, q1: u32, q2: u32) -> bool;
    pub fn num_qubits(&self) -> u32;
    pub fn edges(&self) -> &[(u32, u32)];
    pub fn linear(n: u32) -> Self;
    pub fn full(n: u32) -> Self;
    pub fn star(n: u32) -> Self;
}

/// Basis gates for the target device.
#[derive(Debug, Clone)]
pub struct BasisGates {
    gates: Vec<String>,
}

impl BasisGates {
    pub fn new(gates: impl IntoIterator<Item = impl Into<String>>) -> Self;
    pub fn contains(&self, gate: &str) -> bool;
    pub fn gates(&self) -> &[String];
    pub fn iqm() -> Self;
    pub fn ibm() -> Self;
}

/// Properties shared between passes.
#[derive(Debug, Default)]
pub struct PropertySet {
    pub layout: Option<Layout>,
    pub coupling_map: Option<CouplingMap>,
    pub basis_gates: Option<BasisGates>,
    custom: FxHashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl PropertySet {
    pub fn new() -> Self;
    pub fn with_target(self, coupling_map: CouplingMap, basis_gates: BasisGates) -> Self;
    pub fn insert<T: Any + Send + Sync>(&mut self, value: T);
    pub fn get<T: Any>(&self) -> Option<&T>;
    pub fn get_mut<T: Any>(&mut self) -> Option<&mut T>;
}
```

### hiq-compile: PassManager

```rust
// crates/hiq-compile/src/manager.rs

use hiq_ir::dag::CircuitDag;
use crate::pass::Pass;
use crate::property::PropertySet;
use crate::error::CompileResult;

/// Manages and executes a sequence of compilation passes.
pub struct PassManager {
    passes: Vec<Box<dyn Pass>>,
}

impl PassManager {
    pub fn new() -> Self;
    pub fn add_pass(&mut self, pass: impl Pass + 'static);
    pub fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}

/// Builder for creating pass managers with preset configurations.
pub struct PassManagerBuilder {
    optimization_level: u8,
    properties: PropertySet,
}

impl PassManagerBuilder {
    pub fn new() -> Self;
    pub fn optimization_level(self, level: u8) -> Self;
    pub fn with_properties(self, properties: PropertySet) -> Self;
    pub fn build(self) -> (PassManager, PropertySet);
}
```

## Hardware Abstraction Layer

### hiq-hal: Backend Trait

```rust
// crates/hiq-hal/src/backend.rs

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use hiq_ir::Circuit;
use crate::capability::Capabilities;
use crate::job::{JobId, JobStatus};
use crate::result::ExecutionResult;
use crate::error::HalResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    pub name: String,
    pub endpoint: Option<String>,
    #[serde(skip_serializing)]
    pub token: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[async_trait]
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
    async fn capabilities(&self) -> HalResult<Capabilities>;
    async fn is_available(&self) -> HalResult<bool>;
    async fn submit(&self, circuit: &Circuit, shots: u32) -> HalResult<JobId>;
    async fn status(&self, job_id: &JobId) -> HalResult<JobStatus>;
    async fn result(&self, job_id: &JobId) -> HalResult<ExecutionResult>;
    async fn cancel(&self, job_id: &JobId) -> HalResult<()>;
    async fn wait(&self, job_id: &JobId) -> HalResult<ExecutionResult>;
}

pub trait BackendFactory: Backend + Sized {
    fn from_config(config: BackendConfig) -> HalResult<Self>;
}
```

### hiq-hal: Capabilities

```rust
// crates/hiq-hal/src/capability.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub name: String,
    pub num_qubits: u32,
    pub gate_set: GateSet,
    pub topology: Topology,
    pub max_shots: u32,
    pub is_simulator: bool,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateSet {
    pub single_qubit: Vec<String>,
    pub two_qubit: Vec<String>,
    pub native: Vec<String>,
}

impl GateSet {
    pub fn iqm() -> Self;
    pub fn ibm() -> Self;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topology {
    pub kind: TopologyKind,
    pub edges: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TopologyKind {
    FullyConnected,
    Linear,
    Star,
    Grid { rows: u32, cols: u32 },
    Custom,
}

impl Topology {
    pub fn linear(n: u32) -> Self;
    pub fn star(n: u32) -> Self;
    pub fn full(n: u32) -> Self;
}
```

### hiq-hal: Job Management

```rust
// crates/hiq-hal/src/job.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

impl JobStatus {
    pub fn is_terminal(&self) -> bool;
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: JobId,
    pub status: JobStatus,
    pub shots: u32,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}
```

### hiq-hal: Results

```rust
// crates/hiq-hal/src/result.rs

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Counts {
    counts: FxHashMap<String, u64>,
}

impl Counts {
    pub fn new() -> Self;
    pub fn insert(&mut self, bitstring: impl Into<String>, count: u64);
    pub fn get(&self, bitstring: &str) -> u64;
    pub fn iter(&self) -> impl Iterator<Item = (&String, &u64)>;
    pub fn total_shots(&self) -> u64;
    pub fn most_frequent(&self) -> Option<(&String, &u64)>;
    pub fn probabilities(&self) -> FxHashMap<String, f64>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub counts: Counts,
    pub shots: u32,
    pub execution_time_ms: Option<u64>,
    pub metadata: serde_json::Value,
}
```

## Scheduler Integration

### hiq-sched: Scheduler Trait

```rust
// crates/hiq-sched/src/scheduler.rs

use async_trait::async_trait;
use crate::error::SchedulerResult;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SchedulerJobId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulerStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct JobOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

#[async_trait]
pub trait Scheduler: Send + Sync {
    fn name(&self) -> &str;
    async fn submit(&self, job_spec: &JobSpec) -> SchedulerResult<SchedulerJobId>;
    async fn status(&self, job_id: &SchedulerJobId) -> SchedulerResult<SchedulerStatus>;
    async fn cancel(&self, job_id: &SchedulerJobId) -> SchedulerResult<()>;
    async fn output(&self, job_id: &SchedulerJobId) -> SchedulerResult<JobOutput>;
}

#[derive(Debug, Clone)]
pub struct JobSpec {
    pub name: String,
    pub script: String,
    pub partition: Option<String>,
    pub account: Option<String>,
    pub walltime: Option<std::time::Duration>,
    pub nodes: Option<u32>,
    pub environment: std::collections::HashMap<String, String>,
}
```

### hiq-sched: Slurm Adapter

```rust
// crates/hiq-sched/src/slurm.rs

use crate::scheduler::{Scheduler, SchedulerJobId, SchedulerStatus, JobSpec, JobOutput};
use crate::error::SchedulerResult;

pub struct SlurmConfig {
    pub default_partition: Option<String>,
    pub default_account: Option<String>,
    pub default_walltime: std::time::Duration,
    pub sbatch_path: String,
    pub squeue_path: String,
    pub scancel_path: String,
}

pub struct SlurmAdapter {
    config: SlurmConfig,
}

impl SlurmAdapter {
    pub fn new(config: SlurmConfig) -> Self;
    pub fn generate_script(&self, job_spec: &JobSpec) -> String;
}

#[async_trait]
impl Scheduler for SlurmAdapter {
    fn name(&self) -> &str { "slurm" }
    async fn submit(&self, job_spec: &JobSpec) -> SchedulerResult<SchedulerJobId>;
    async fn status(&self, job_id: &SchedulerJobId) -> SchedulerResult<SchedulerStatus>;
    async fn cancel(&self, job_id: &SchedulerJobId) -> SchedulerResult<()>;
    async fn output(&self, job_id: &SchedulerJobId) -> SchedulerResult<JobOutput>;
}
```

## Error Types

### hiq-ir Errors

```rust
// crates/hiq-ir/src/error.rs

use thiserror::Error;
use crate::qubit::{QubitId, ClbitId};

#[derive(Debug, Error)]
pub enum IrError {
    #[error("Qubit {0:?} not found in circuit")]
    QubitNotFound(QubitId),

    #[error("Classical bit {0:?} not found in circuit")]
    ClbitNotFound(ClbitId),

    #[error("Invalid DAG structure")]
    InvalidDag,

    #[error("Invalid node index")]
    InvalidNode,

    #[error("Gate requires {expected} qubits, got {got}")]
    QubitCountMismatch { expected: u32, got: u32 },

    #[error("Parameter '{0}' is unbound")]
    UnboundParameter(String),

    #[error("Cannot perform operation on parameterized circuit")]
    ParameterizedCircuit,
}

pub type IrResult<T> = Result<T, IrError>;
```

### hiq-compile Errors

```rust
// crates/hiq-compile/src/error.rs

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("IR error: {0}")]
    Ir(#[from] hiq_ir::error::IrError),

    #[error("Missing coupling map for routing")]
    MissingCouplingMap,

    #[error("Missing layout for routing")]
    MissingLayout,

    #[error("Routing failed: qubits {qubit1} and {qubit2} not connected")]
    RoutingFailed { qubit1: u32, qubit2: u32 },

    #[error("Gate '{0}' not in target basis")]
    GateNotInBasis(String),

    #[error("Pass '{0}' failed: {1}")]
    PassFailed(String, String),
}

pub type CompileResult<T> = Result<T, CompileError>;
```

### hiq-hal Errors

```rust
// crates/hiq-hal/src/error.rs

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HalError {
    #[error("Backend not available: {0}")]
    BackendUnavailable(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Job submission failed: {0}")]
    SubmissionFailed(String),

    #[error("Job failed: {0}")]
    JobFailed(String),

    #[error("Job cancelled")]
    JobCancelled,

    #[error("Job not found: {0}")]
    JobNotFound(String),

    #[error("Invalid circuit: {0}")]
    InvalidCircuit(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Configuration(String),
}

pub type HalResult<T> = Result<T, HalError>;
```

## CLI Structure

### hiq-cli Commands

```rust
// crates/hiq-cli/src/main.rs

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "hiq")]
#[command(about = "Rust-native quantum compilation and orchestration")]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a quantum circuit
    Compile {
        #[arg(short, long)]
        input: String,
        #[arg(short, long)]
        output: Option<String>,
        #[arg(short, long, default_value = "iqm")]
        target: String,
        #[arg(long, default_value = "1")]
        optimization_level: u8,
    },

    /// Submit a circuit for execution
    Submit {
        #[arg(short, long)]
        input: String,
        #[arg(short, long, default_value = "1024")]
        shots: u32,
        #[arg(short, long)]
        backend: String,
        #[arg(short, long)]
        wait: bool,
    },

    /// Check job status
    Status {
        job_id: String,
        #[arg(short, long)]
        backend: String,
    },

    /// Get job result
    Result {
        job_id: String,
        #[arg(short, long)]
        backend: String,
        #[arg(short, long, default_value = "table")]
        format: String,
    },

    /// List available backends
    Backends,
}
```
