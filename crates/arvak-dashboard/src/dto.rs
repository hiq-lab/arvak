//! Data Transfer Objects for the dashboard API.
//!
//! These types bridge internal Arvak structures to JSON-serializable API responses.

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use arvak_ir::{Circuit, CircuitDag, Instruction, InstructionKind, QubitId};

// ============================================================================
// Circuit Visualization DTOs
// ============================================================================

/// Request to visualize a circuit.
#[derive(Debug, Deserialize)]
pub struct VisualizeRequest {
    /// QASM3 source code.
    pub qasm: String,
}

/// Request to compile a circuit.
#[derive(Debug, Deserialize)]
pub struct CompileRequest {
    /// QASM3 source code.
    pub qasm: String,
    /// Target backend (e.g., "iqm", "ibm", "simulator").
    pub target: String,
    /// Optimization level (0-3).
    #[serde(default)]
    pub optimization_level: u8,
}

/// Circuit visualization data for frontend rendering.
#[derive(Debug, Serialize)]
pub struct CircuitVisualization {
    /// Circuit name.
    pub name: String,
    /// Number of qubits.
    pub num_qubits: usize,
    /// Number of classical bits.
    pub num_clbits: usize,
    /// Circuit depth.
    pub depth: usize,
    /// Total number of operations.
    pub num_ops: usize,
    /// Operations organized by time layer for visualization.
    pub layers: Vec<CircuitLayer>,
}

/// A single time layer in the circuit.
#[derive(Debug, Serialize)]
pub struct CircuitLayer {
    /// Depth index (0-based).
    pub depth: usize,
    /// Operations at this depth.
    pub operations: Vec<OperationView>,
}

/// A single operation for visualization.
#[derive(Debug, Serialize)]
pub struct OperationView {
    /// Gate name (e.g., "h", "cx", "rx").
    pub gate: String,
    /// Display label (e.g., "H", "CX", "RX(0.79)").
    pub label: String,
    /// Qubit indices this operation acts on.
    pub qubits: Vec<u32>,
    /// Classical bit indices (for measurements).
    pub clbits: Vec<u32>,
    /// Whether this is a measurement operation.
    pub is_measurement: bool,
    /// Whether this is a barrier.
    pub is_barrier: bool,
    /// Number of qubits (for rendering multi-qubit gates).
    pub num_qubits: usize,
}

/// Response from compile endpoint.
#[derive(Debug, Serialize)]
pub struct CompileResponse {
    /// Original circuit visualization.
    pub before: CircuitVisualization,
    /// Compiled circuit visualization.
    pub after: CircuitVisualization,
    /// Compiled QASM3 output.
    pub compiled_qasm: String,
    /// Compilation statistics.
    pub stats: CompilationStats,
}

/// Compilation statistics.
#[derive(Debug, Serialize)]
pub struct CompilationStats {
    /// Original circuit depth.
    pub original_depth: usize,
    /// Compiled circuit depth.
    pub compiled_depth: usize,
    /// Gate count before compilation.
    pub gates_before: usize,
    /// Gate count after compilation.
    pub gates_after: usize,
    /// Compilation time in microseconds.
    pub compile_time_us: u64,
    /// Throughput in gates per second.
    pub throughput_gates_per_sec: u64,
}

// ============================================================================
// Backend DTOs
// ============================================================================

/// Summary of a backend for list view.
#[derive(Debug, Serialize)]
pub struct BackendSummary {
    /// Backend name.
    pub name: String,
    /// Whether this is a simulator.
    pub is_simulator: bool,
    /// Number of qubits.
    pub num_qubits: u32,
    /// Whether the backend is currently available.
    pub available: bool,
    /// Native gate set.
    pub native_gates: Vec<String>,
}

/// Detailed backend information.
#[derive(Debug, Serialize)]
pub struct BackendDetails {
    /// Backend name.
    pub name: String,
    /// Whether this is a simulator.
    pub is_simulator: bool,
    /// Number of qubits.
    pub num_qubits: u32,
    /// Maximum shots per job.
    pub max_shots: u32,
    /// Whether the backend is currently available.
    pub available: bool,
    /// Gate set information.
    pub gate_set: GateSetView,
    /// Topology information.
    pub topology: TopologyView,
}

/// Gate set information for display.
#[derive(Debug, Serialize)]
pub struct GateSetView {
    /// Single-qubit gates.
    pub single_qubit: Vec<String>,
    /// Two-qubit gates.
    pub two_qubit: Vec<String>,
    /// Native gates (hardware-native).
    pub native: Vec<String>,
}

/// Topology information for visualization.
#[derive(Debug, Serialize)]
pub struct TopologyView {
    /// Topology kind (e.g., "star", "linear", "grid").
    pub kind: String,
    /// Coupling edges as (qubit1, qubit2) pairs.
    pub edges: Vec<(u32, u32)>,
    /// Number of qubits.
    pub num_qubits: u32,
}

// ============================================================================
// Job DTOs
// ============================================================================

/// Request to create a new job.
#[derive(Debug, Deserialize)]
pub struct CreateJobRequest {
    /// Job name.
    pub name: String,
    /// QASM3 source code for the circuit.
    pub qasm: String,
    /// Number of shots.
    #[serde(default = "default_shots")]
    pub shots: u32,
    /// Target backend name (optional).
    pub backend: Option<String>,
    /// Job priority (default 100).
    #[serde(default = "default_priority")]
    pub priority: u32,
}

fn default_shots() -> u32 {
    1024
}

fn default_priority() -> u32 {
    100
}

/// Job summary for list view.
#[derive(Debug, Serialize)]
pub struct JobSummary {
    /// Job ID.
    pub id: String,
    /// Job name.
    pub name: String,
    /// Current status.
    pub status: String,
    /// Status details (e.g., SLURM job ID).
    pub status_details: Option<String>,
    /// Backend used.
    pub backend: Option<String>,
    /// Number of shots.
    pub shots: u32,
    /// Number of circuits.
    pub num_circuits: usize,
    /// Job priority.
    pub priority: u32,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Submission timestamp (ISO 8601).
    pub submitted_at: Option<String>,
    /// Completion timestamp (ISO 8601).
    pub completed_at: Option<String>,
}

/// Detailed job information.
#[derive(Debug, Serialize)]
pub struct JobDetails {
    /// Job ID.
    pub id: String,
    /// Job name.
    pub name: String,
    /// Current status.
    pub status: String,
    /// Status details.
    pub status_details: Option<String>,
    /// Backend used.
    pub backend: Option<String>,
    /// Number of shots.
    pub shots: u32,
    /// Job priority.
    pub priority: u32,
    /// QASM source for first circuit.
    pub qasm: Option<String>,
    /// Number of circuits in batch.
    pub num_circuits: usize,
    /// Creation timestamp.
    pub created_at: String,
    /// Submission timestamp.
    pub submitted_at: Option<String>,
    /// Completion timestamp.
    pub completed_at: Option<String>,
    /// Job metadata.
    pub metadata: std::collections::HashMap<String, String>,
}

/// Query parameters for listing jobs.
#[derive(Debug, Deserialize, Default)]
pub struct JobListParams {
    /// Filter by status.
    pub status: Option<String>,
    /// Limit results.
    pub limit: Option<usize>,
    /// Show only pending jobs.
    #[serde(default)]
    pub pending: bool,
    /// Show only running jobs.
    #[serde(default)]
    pub running: bool,
}

/// Result histogram data.
#[derive(Debug, Serialize)]
pub struct ResultHistogram {
    /// Job ID.
    pub job_id: String,
    /// Number of shots.
    pub shots: u32,
    /// Execution time in milliseconds.
    pub execution_time_ms: Option<u64>,
    /// Histogram bars.
    pub bars: Vec<HistogramBar>,
    /// Statistics.
    pub statistics: ResultStatistics,
}

/// A single bar in the histogram.
#[derive(Debug, Serialize)]
pub struct HistogramBar {
    /// Bitstring result.
    pub bitstring: String,
    /// Count of this outcome.
    pub count: u64,
    /// Probability (count / shots).
    pub probability: f64,
}

/// Statistics about the result.
#[derive(Debug, Serialize)]
pub struct ResultStatistics {
    /// Total number of shots.
    pub total_shots: u64,
    /// Number of unique outcomes.
    pub unique_outcomes: usize,
    /// Most frequent outcome.
    pub most_frequent: String,
    /// Most frequent count.
    pub most_frequent_count: u64,
}

// ============================================================================
// Conversion implementations
// ============================================================================

impl CircuitVisualization {
    /// Create a visualization from a Circuit.
    pub fn from_circuit(circuit: &Circuit) -> Self {
        let dag = circuit.dag();
        let layers = circuit_to_layers(dag);

        Self {
            name: circuit.name().to_string(),
            num_qubits: circuit.num_qubits(),
            num_clbits: circuit.num_clbits(),
            depth: circuit.depth(),
            num_ops: dag.num_ops(),
            layers,
        }
    }
}

/// Convert a CircuitDag to visualization layers.
///
/// This groups operations by their depth (earliest time they can execute)
/// for rendering as a circuit diagram.
fn circuit_to_layers(dag: &CircuitDag) -> Vec<CircuitLayer> {
    let mut layers: Vec<CircuitLayer> = vec![];
    let mut qubit_depth: FxHashMap<QubitId, usize> = FxHashMap::default();

    for (_node_idx, instruction) in dag.topological_ops() {
        // Calculate which layer this operation belongs to
        // (must be after all operations on its qubits)
        let op_depth = instruction
            .qubits
            .iter()
            .map(|q| qubit_depth.get(q).copied().unwrap_or(0))
            .max()
            .unwrap_or(0);

        // Ensure we have enough layers
        while layers.len() <= op_depth {
            layers.push(CircuitLayer {
                depth: layers.len(),
                operations: vec![],
            });
        }

        // Convert instruction to view
        let op_view = instruction_to_view(instruction);

        // Add to layer
        layers[op_depth].operations.push(op_view);

        // Update qubit depths for subsequent operations
        for q in &instruction.qubits {
            qubit_depth.insert(*q, op_depth + 1);
        }
    }

    layers
}

/// Convert an Instruction to an OperationView for visualization.
fn instruction_to_view(instruction: &Instruction) -> OperationView {
    let (gate, label) = match &instruction.kind {
        InstructionKind::Gate(g) => {
            let name = g.name().to_string();
            let label = format_gate_label(g);
            (name, label)
        }
        InstructionKind::Measure => ("measure".to_string(), "M".to_string()),
        InstructionKind::Reset => ("reset".to_string(), "|0⟩".to_string()),
        InstructionKind::Barrier => ("barrier".to_string(), "║".to_string()),
        InstructionKind::Delay { duration } => ("delay".to_string(), format!("D({})", duration)),
        InstructionKind::Shuttle { from_zone, to_zone } => (
            "shuttle".to_string(),
            format!("S({}-{})", from_zone, to_zone),
        ),
        InstructionKind::NoiseChannel { model, role } => (
            format!("noise_{}", role),
            format!("N({})", model.name()),
        ),
    };

    OperationView {
        gate,
        label,
        qubits: instruction.qubits.iter().map(|q| q.0).collect(),
        clbits: instruction.clbits.iter().map(|c| c.0).collect(),
        is_measurement: instruction.is_measure(),
        is_barrier: instruction.is_barrier(),
        num_qubits: instruction.qubits.len(),
    }
}

/// Format a gate label for display, including parameters.
fn format_gate_label(gate: &arvak_ir::Gate) -> String {
    use arvak_ir::GateKind;

    match &gate.kind {
        GateKind::Standard(std_gate) => format_standard_gate_label(std_gate),
        GateKind::Custom(custom) => custom.name.clone(),
    }
}

/// Format a StandardGate label with parameters.
fn format_standard_gate_label(gate: &arvak_ir::StandardGate) -> String {
    use arvak_ir::StandardGate::*;

    match gate {
        // Simple gates (no parameters)
        I => "I".to_string(),
        X => "X".to_string(),
        Y => "Y".to_string(),
        Z => "Z".to_string(),
        H => "H".to_string(),
        S => "S".to_string(),
        Sdg => "S†".to_string(),
        T => "T".to_string(),
        Tdg => "T†".to_string(),
        SX => "√X".to_string(),
        SXdg => "√X†".to_string(),
        CX => "CX".to_string(),
        CY => "CY".to_string(),
        CZ => "CZ".to_string(),
        CH => "CH".to_string(),
        Swap => "SWAP".to_string(),
        ISwap => "iSWAP".to_string(),
        CCX => "CCX".to_string(),
        CSwap => "CSWAP".to_string(),

        // Parameterized gates
        Rx(p) => format!("RX({})", format_param(p)),
        Ry(p) => format!("RY({})", format_param(p)),
        Rz(p) => format!("RZ({})", format_param(p)),
        P(p) => format!("P({})", format_param(p)),
        U(t, p, l) => format!(
            "U({},{},{})",
            format_param(t),
            format_param(p),
            format_param(l)
        ),
        CRx(p) => format!("CRX({})", format_param(p)),
        CRy(p) => format!("CRY({})", format_param(p)),
        CRz(p) => format!("CRZ({})", format_param(p)),
        CP(p) => format!("CP({})", format_param(p)),
        RXX(p) => format!("RXX({})", format_param(p)),
        RYY(p) => format!("RYY({})", format_param(p)),
        RZZ(p) => format!("RZZ({})", format_param(p)),
        PRX(t, p) => format!("PRX({},{})", format_param(t), format_param(p)),
    }
}

/// Format a parameter expression for display.
fn format_param(param: &arvak_ir::ParameterExpression) -> String {
    if let Some(value) = param.as_f64() {
        // Format as a nice number (2 decimal places, or special values)
        let pi = std::f64::consts::PI;
        if (value - pi).abs() < 1e-10 {
            "π".to_string()
        } else if (value - pi / 2.0).abs() < 1e-10 {
            "π/2".to_string()
        } else if (value - pi / 4.0).abs() < 1e-10 {
            "π/4".to_string()
        } else if (value + pi).abs() < 1e-10 {
            "-π".to_string()
        } else if (value + pi / 2.0).abs() < 1e-10 {
            "-π/2".to_string()
        } else {
            format!("{:.2}", value)
        }
    } else {
        // Symbolic parameter
        param.to_string()
    }
}

// ============================================================================
// Health check response
// ============================================================================

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Status (always "ok" if responding).
    pub status: String,
    /// Dashboard version.
    pub version: String,
}

impl Default for HealthResponse {
    fn default() -> Self {
        Self {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}
