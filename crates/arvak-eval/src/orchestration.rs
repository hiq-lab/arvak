//! Orchestration Module: Hybrid DAG construction, critical path, and batchability analysis.
//!
//! Builds a high-level DAG that represents the interleaving of quantum
//! and classical computation phases. Each node is either a quantum circuit
//! execution or a classical processing step.
//!
//! # Hybrid DAG
//!
//! ```text
//! [q1]───┐
//!        v
//! [c1]───┐
//!        v
//! [q2]───┐
//!        v
//! [c2]
//! ```
//!
//! Quantum nodes carry circuit metrics (depth, gate counts).
//! Classical nodes carry estimated processing time.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use arvak_ir::CircuitDag;
use arvak_ir::instruction::InstructionKind;

/// Type of node in the hybrid DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HybridNodeKind {
    /// A quantum circuit execution phase.
    Quantum,
    /// A classical computation phase (e.g., parameter optimization, post-processing).
    Classical,
}

impl std::fmt::Display for HybridNodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HybridNodeKind::Quantum => write!(f, "quantum"),
            HybridNodeKind::Classical => write!(f, "classical"),
        }
    }
}

/// A node in the hybrid quantum-classical DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridNode {
    /// Unique node index.
    pub index: usize,
    /// Node kind.
    pub kind: HybridNodeKind,
    /// Human-readable label.
    pub label: String,
    /// Circuit depth (quantum nodes only).
    pub depth: Option<usize>,
    /// Gate count (quantum nodes only).
    pub gate_count: Option<usize>,
    /// Number of qubits involved (quantum nodes only).
    pub num_qubits: Option<usize>,
    /// Estimated cost in abstract time units.
    pub estimated_cost: f64,
}

/// An edge in the hybrid DAG representing a dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridEdge {
    /// Source node index.
    pub from: usize,
    /// Target node index.
    pub to: usize,
    /// Type of data flowing across this edge.
    pub data_flow: DataFlow,
}

/// Kind of data flowing between hybrid nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataFlow {
    /// Measurement results flowing from quantum to classical.
    MeasurementResults,
    /// Updated parameters flowing from classical to quantum.
    Parameters,
    /// General dependency (ordering constraint).
    Dependency,
}

/// The hybrid quantum-classical DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridDag {
    /// All nodes in the DAG.
    pub nodes: Vec<HybridNode>,
    /// All edges (dependencies).
    pub edges: Vec<HybridEdge>,
}

/// Critical path analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriticalPath {
    /// Indices of nodes on the critical path.
    pub node_indices: Vec<usize>,
    /// Total cost of the critical path.
    pub total_cost: f64,
    /// Number of quantum phases on the critical path.
    pub quantum_phases: usize,
    /// Number of classical phases on the critical path.
    pub classical_phases: usize,
}

/// Batchability analysis: how many independent quantum jobs can run in parallel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchabilityAnalysis {
    /// Maximum batch width (parallel independent quantum jobs).
    pub max_parallel_quantum: usize,
    /// Total quantum phases.
    pub total_quantum_phases: usize,
    /// Total classical phases.
    pub total_classical_phases: usize,
    /// Parallelism ratio (max_parallel / total_quantum, higher is better).
    pub parallelism_ratio: f64,
    /// Whether the circuit is purely quantum (no classical interleavings).
    pub is_purely_quantum: bool,
    /// Independent quantum groups (sets of nodes that can run concurrently).
    pub parallel_groups: Vec<Vec<usize>>,
}

/// Complete orchestration analysis report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationReport {
    /// The hybrid DAG.
    pub dag: HybridDag,
    /// Critical path analysis.
    pub critical_path: CriticalPath,
    /// Batchability analysis.
    pub batchability: BatchabilityAnalysis,
    /// Summary statistics.
    pub summary: OrchestrationSummary,
}

/// High-level orchestration summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationSummary {
    /// Total nodes in hybrid DAG.
    pub total_nodes: usize,
    /// Total edges.
    pub total_edges: usize,
    /// Number of quantum phases.
    pub quantum_phases: usize,
    /// Number of classical phases.
    pub classical_phases: usize,
    /// DAG depth (longest path in node count).
    pub dag_depth: usize,
    /// Estimated total cost.
    pub estimated_total_cost: f64,
}

/// Builder for hybrid DAGs from circuit analysis.
pub struct OrchestrationAnalyzer;

impl OrchestrationAnalyzer {
    /// Analyze a compiled circuit DAG and build the orchestration report.
    ///
    /// For a single circuit without classical feedback, this produces a simple
    /// linear DAG: [quantum] -> [classical_readout].
    ///
    /// Circuits with measurements mid-circuit produce interleaved phases.
    pub fn analyze(circuit_dag: &CircuitDag, num_qubits: usize) -> OrchestrationReport {
        let hybrid_dag = Self::build_hybrid_dag(circuit_dag, num_qubits);
        let critical_path = Self::compute_critical_path(&hybrid_dag);
        let batchability = Self::analyze_batchability(&hybrid_dag);

        let summary = OrchestrationSummary {
            total_nodes: hybrid_dag.nodes.len(),
            total_edges: hybrid_dag.edges.len(),
            quantum_phases: hybrid_dag
                .nodes
                .iter()
                .filter(|n| n.kind == HybridNodeKind::Quantum)
                .count(),
            classical_phases: hybrid_dag
                .nodes
                .iter()
                .filter(|n| n.kind == HybridNodeKind::Classical)
                .count(),
            dag_depth: critical_path.node_indices.len(),
            estimated_total_cost: critical_path.total_cost,
        };

        OrchestrationReport {
            dag: hybrid_dag,
            critical_path,
            batchability,
            summary,
        }
    }

    /// Build a hybrid DAG by segmenting the circuit into quantum and classical phases.
    ///
    /// Strategy: Walk through the circuit in topological order. Group consecutive
    /// gate operations into quantum phases. Each measurement triggers a transition
    /// to a classical phase (readout/processing), followed by a new quantum phase
    /// if more gates follow.
    fn build_hybrid_dag(circuit_dag: &CircuitDag, num_qubits: usize) -> HybridDag {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        let mut current_gate_count = 0usize;
        let mut current_depth_estimate = 0usize;
        let mut saw_measurement = false;
        let mut phase_index = 0usize;

        // Walk through operations and segment into phases
        for (_idx, inst) in circuit_dag.topological_ops() {
            match &inst.kind {
                InstructionKind::Gate(_) => {
                    // If we just finished a classical phase, start new quantum phase
                    if saw_measurement && current_gate_count == 0 {
                        // Already pushed the classical node; this gate starts a new quantum phase
                    }
                    current_gate_count += 1;
                    current_depth_estimate += 1;
                    saw_measurement = false;
                }
                InstructionKind::Measure => {
                    if current_gate_count > 0 || nodes.is_empty() {
                        // Finalize current quantum phase
                        let q_node = HybridNode {
                            index: phase_index,
                            kind: HybridNodeKind::Quantum,
                            label: format!("q{}", nodes.len() / 2),
                            depth: Some(current_depth_estimate),
                            gate_count: Some(current_gate_count),
                            num_qubits: Some(num_qubits),
                            estimated_cost: estimate_quantum_cost(
                                current_depth_estimate,
                                current_gate_count,
                            ),
                        };
                        nodes.push(q_node);
                        phase_index += 1;
                        current_gate_count = 0;
                        current_depth_estimate = 0;
                    }

                    // Add classical readout phase
                    let c_node = HybridNode {
                        index: phase_index,
                        kind: HybridNodeKind::Classical,
                        label: format!("c{}", nodes.len() / 2),
                        depth: None,
                        gate_count: None,
                        num_qubits: None,
                        estimated_cost: CLASSICAL_READOUT_COST,
                    };

                    // Edge: quantum -> classical (measurement results)
                    if nodes.len() >= 1 {
                        edges.push(HybridEdge {
                            from: phase_index - 1,
                            to: phase_index,
                            data_flow: DataFlow::MeasurementResults,
                        });
                    }

                    nodes.push(c_node);
                    phase_index += 1;
                    saw_measurement = true;
                }
                InstructionKind::Barrier => {
                    // Barriers don't create new phases but add to depth
                    current_depth_estimate += 1;
                }
                _ => {}
            }
        }

        // Finalize any remaining quantum operations
        if current_gate_count > 0 {
            let q_node = HybridNode {
                index: phase_index,
                kind: HybridNodeKind::Quantum,
                label: format!(
                    "q{}",
                    nodes
                        .iter()
                        .filter(|n| n.kind == HybridNodeKind::Quantum)
                        .count()
                ),
                depth: Some(current_depth_estimate),
                gate_count: Some(current_gate_count),
                num_qubits: Some(num_qubits),
                estimated_cost: estimate_quantum_cost(current_depth_estimate, current_gate_count),
            };

            // If there was a preceding classical phase, add parameter edge
            if let Some(last) = nodes.last() {
                if last.kind == HybridNodeKind::Classical {
                    edges.push(HybridEdge {
                        from: last.index,
                        to: phase_index,
                        data_flow: DataFlow::Parameters,
                    });
                }
            }

            nodes.push(q_node);
        }

        // Handle edge case: empty circuit
        if nodes.is_empty() {
            nodes.push(HybridNode {
                index: 0,
                kind: HybridNodeKind::Quantum,
                label: "q0".into(),
                depth: Some(0),
                gate_count: Some(0),
                num_qubits: Some(num_qubits),
                estimated_cost: 0.0,
            });
        }

        HybridDag { nodes, edges }
    }

    /// Compute the critical path through the hybrid DAG.
    ///
    /// Uses a topological-order dynamic programming approach to find the
    /// longest-cost path (critical path) through the DAG.
    fn compute_critical_path(dag: &HybridDag) -> CriticalPath {
        if dag.nodes.is_empty() {
            return CriticalPath {
                node_indices: vec![],
                total_cost: 0.0,
                quantum_phases: 0,
                classical_phases: 0,
            };
        }

        let n = dag.nodes.len();
        let mut dist = vec![0.0f64; n];
        let mut predecessor = vec![None::<usize>; n];

        // Initialize: each node's cost is at least its own
        for node in &dag.nodes {
            dist[node.index] = node.estimated_cost;
        }

        // Relax edges in topological order (nodes are already ordered by index)
        for edge in &dag.edges {
            let new_cost = dist[edge.from] + dag.nodes[edge.to].estimated_cost;
            if new_cost > dist[edge.to] {
                dist[edge.to] = new_cost;
                predecessor[edge.to] = Some(edge.from);
            }
        }

        // Find the node with maximum distance (end of critical path)
        let end_idx = dist
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0);

        // Trace back the critical path
        let mut path = vec![end_idx];
        let mut current = end_idx;
        while let Some(prev) = predecessor[current] {
            path.push(prev);
            current = prev;
        }
        path.reverse();

        let total_cost = dist[end_idx];
        let quantum_phases = path
            .iter()
            .filter(|&&i| dag.nodes[i].kind == HybridNodeKind::Quantum)
            .count();
        let classical_phases = path
            .iter()
            .filter(|&&i| dag.nodes[i].kind == HybridNodeKind::Classical)
            .count();

        CriticalPath {
            node_indices: path,
            total_cost,
            quantum_phases,
            classical_phases,
        }
    }

    /// Analyze batchability: identify independent quantum phases that can run in parallel.
    fn analyze_batchability(dag: &HybridDag) -> BatchabilityAnalysis {
        let quantum_nodes: Vec<usize> = dag
            .nodes
            .iter()
            .filter(|n| n.kind == HybridNodeKind::Quantum)
            .map(|n| n.index)
            .collect();

        let classical_nodes: Vec<usize> = dag
            .nodes
            .iter()
            .filter(|n| n.kind == HybridNodeKind::Classical)
            .map(|n| n.index)
            .collect();

        let total_quantum = quantum_nodes.len();
        let total_classical = classical_nodes.len();
        let is_purely_quantum = total_classical == 0;

        // Build dependency sets for quantum nodes
        let dependent_on: BTreeMap<usize, Vec<usize>> =
            dag.edges.iter().fold(BTreeMap::new(), |mut map, edge| {
                map.entry(edge.to).or_default().push(edge.from);
                map
            });

        // Group quantum nodes by their "level" (nodes with no quantum predecessors
        // can run in parallel)
        let mut parallel_groups: Vec<Vec<usize>> = Vec::new();
        let mut assigned: BTreeMap<usize, usize> = BTreeMap::new(); // node -> group index

        for &qn in &quantum_nodes {
            // Find the latest group of any predecessor
            let max_pred_group = dependent_on
                .get(&qn)
                .map(|preds| preds.iter().filter_map(|p| assigned.get(p)).max().copied())
                .flatten();

            let group = match max_pred_group {
                Some(g) => g + 1,
                None => 0,
            };

            while parallel_groups.len() <= group {
                parallel_groups.push(Vec::new());
            }
            parallel_groups[group].push(qn);
            assigned.insert(qn, group);
        }

        let max_parallel = parallel_groups.iter().map(|g| g.len()).max().unwrap_or(0);
        let parallelism_ratio = if total_quantum > 0 {
            max_parallel as f64 / total_quantum as f64
        } else {
            1.0
        };

        BatchabilityAnalysis {
            max_parallel_quantum: max_parallel,
            total_quantum_phases: total_quantum,
            total_classical_phases: total_classical,
            parallelism_ratio,
            is_purely_quantum,
            parallel_groups,
        }
    }
}

/// Abstract cost unit for quantum circuit execution.
/// Based on depth and gate count as rough proxy for wall time.
fn estimate_quantum_cost(depth: usize, gate_count: usize) -> f64 {
    // Each gate layer ~1 microsecond, plus overhead
    let gate_time = depth as f64 * 1.0;
    let overhead = gate_count as f64 * 0.01;
    gate_time + overhead + QUANTUM_SETUP_COST
}

/// Fixed cost for setting up a quantum execution (calibration, compilation, etc.).
const QUANTUM_SETUP_COST: f64 = 10.0;

/// Fixed cost for classical readout and basic post-processing.
const CLASSICAL_READOUT_COST: f64 = 1.0;

#[cfg(test)]
mod tests {
    use super::*;
    use arvak_ir::{Circuit, ClbitId, QubitId};

    #[test]
    fn test_simple_circuit_hybrid_dag() {
        // H -> CX -> Measure: should produce [q0] -> [c0]
        let mut circuit = Circuit::with_size("bell", 2, 2);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.measure(QubitId(0), ClbitId(0)).unwrap();
        circuit.measure(QubitId(1), ClbitId(1)).unwrap();

        let dag = circuit.into_dag();
        let report = OrchestrationAnalyzer::analyze(&dag, 2);

        assert!(report.dag.nodes.len() >= 2);
        assert!(report.summary.quantum_phases >= 1);
        assert!(report.summary.classical_phases >= 1);
    }

    #[test]
    fn test_purely_quantum_circuit() {
        // No measurements: purely quantum
        let mut circuit = Circuit::with_size("qft", 3, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.cx(QubitId(1), QubitId(2)).unwrap();

        let dag = circuit.into_dag();
        let report = OrchestrationAnalyzer::analyze(&dag, 3);

        assert!(report.batchability.is_purely_quantum);
        assert_eq!(report.summary.classical_phases, 0);
        assert_eq!(report.summary.quantum_phases, 1);
    }

    #[test]
    fn test_critical_path_single_node() {
        let mut circuit = Circuit::with_size("simple", 1, 0);
        circuit.h(QubitId(0)).unwrap();

        let dag = circuit.into_dag();
        let report = OrchestrationAnalyzer::analyze(&dag, 1);

        assert!(!report.critical_path.node_indices.is_empty());
        assert!(report.critical_path.total_cost > 0.0);
    }

    #[test]
    fn test_batchability_simple() {
        let mut circuit = Circuit::with_size("test", 2, 2);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.measure(QubitId(0), ClbitId(0)).unwrap();

        let dag = circuit.into_dag();
        let report = OrchestrationAnalyzer::analyze(&dag, 2);

        // Single quantum phase -> max_parallel = 1
        assert_eq!(report.batchability.max_parallel_quantum, 1);
        assert!(report.batchability.total_quantum_phases >= 1);
    }

    #[test]
    fn test_empty_circuit() {
        let circuit = Circuit::with_size("empty", 1, 0);
        let dag = circuit.into_dag();
        let report = OrchestrationAnalyzer::analyze(&dag, 1);

        assert!(!report.dag.nodes.is_empty());
        assert_eq!(report.summary.total_nodes, 1);
    }

    #[test]
    fn test_hybrid_node_display() {
        assert_eq!(HybridNodeKind::Quantum.to_string(), "quantum");
        assert_eq!(HybridNodeKind::Classical.to_string(), "classical");
    }

    #[test]
    fn test_critical_path_cost_positive() {
        let mut circuit = Circuit::with_size("test", 3, 3);
        circuit.h(QubitId(0)).unwrap();
        circuit.h(QubitId(1)).unwrap();
        circuit.h(QubitId(2)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.cx(QubitId(1), QubitId(2)).unwrap();
        circuit.measure(QubitId(0), ClbitId(0)).unwrap();
        circuit.measure(QubitId(1), ClbitId(1)).unwrap();
        circuit.measure(QubitId(2), ClbitId(2)).unwrap();

        let dag = circuit.into_dag();
        let report = OrchestrationAnalyzer::analyze(&dag, 3);

        assert!(report.critical_path.total_cost > 0.0);
        assert!(report.critical_path.quantum_phases >= 1);
    }
}
