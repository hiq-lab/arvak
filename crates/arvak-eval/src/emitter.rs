//! Emitter Compliance Module: Materialization coverage and loss documentation.
//!
//! Analyzes how well a compiled circuit maps to a specific backend's
//! native gate set and documents what capabilities are lost during
//! the materialization process.
//!
//! Supports IQM, IBM, and CUDA-Q target backends.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use arvak_hal::Capabilities;
use arvak_ir::instruction::InstructionKind;
use arvak_ir::{Circuit, CircuitDag};

use crate::error::EvalResult;

/// Target backend for materialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmitTarget {
    /// IQM native gate set (PRX, CZ).
    Iqm,
    /// IBM native gate set (SX, RZ, CX).
    Ibm,
    /// CUDA-Q / simulator (universal gate set).
    CudaQ,
}

impl EmitTarget {
    /// Parse a target name string into an EmitTarget.
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "iqm" => Some(Self::Iqm),
            "ibm" => Some(Self::Ibm),
            "cuda-q" | "cudaq" | "simulator" => Some(Self::CudaQ),
            _ => None,
        }
    }

    /// Return the native gate names for this target.
    pub fn native_gates(&self) -> &'static [&'static str] {
        match self {
            Self::Iqm => &["prx", "cz", "id"],
            Self::Ibm => &["sx", "rz", "cx", "id", "x"],
            Self::CudaQ => &[
                "h", "x", "y", "z", "s", "sdg", "t", "tdg", "sx", "sxdg", "rx", "ry", "rz", "p",
                "u", "cx", "cy", "cz", "ch", "swap", "iswap", "crx", "cry", "crz", "cp", "rxx",
                "ryy", "rzz", "ccx", "cswap", "prx", "id",
            ],
        }
    }

    /// Human-readable name for the target.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Iqm => "IQM",
            Self::Ibm => "IBM",
            Self::CudaQ => "CUDA-Q",
        }
    }
}

/// Classification of a gate during materialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaterializationStatus {
    /// Gate is natively supported by the target.
    Native,
    /// Gate requires decomposition into native gates.
    Decomposed,
    /// Gate cannot be materialized for this target.
    Lost,
}

/// Record of a single gate type's materialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateMaterialization {
    /// Gate name.
    pub gate_name: String,
    /// Number of occurrences in the circuit.
    pub count: usize,
    /// Materialization status.
    pub status: MaterializationStatus,
    /// Expected decomposition cost (in native gate count), if decomposed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decomposition_cost: Option<usize>,
    /// Explanation of the materialization.
    pub note: String,
}

/// Coverage metrics for materialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageMetrics {
    /// Total gates in the circuit.
    pub total_gates: usize,
    /// Gates natively supported.
    pub native_count: usize,
    /// Gates requiring decomposition.
    pub decomposed_count: usize,
    /// Gates that cannot be materialized.
    pub lost_count: usize,
    /// Coverage ratio (native / total).
    pub native_coverage: f64,
    /// Materialization ratio ((native + decomposed) / total).
    pub materializable_coverage: f64,
    /// Estimated gate expansion factor from decomposition.
    pub estimated_expansion: f64,
}

/// A single loss record documenting a capability lost during materialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LossRecord {
    /// Gate or capability that was lost.
    pub capability: String,
    /// Loss category.
    pub category: LossCategory,
    /// Impact description.
    pub impact: String,
    /// Estimated cost in native gates (if decomposable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_gate_cost: Option<usize>,
}

/// Category of capability loss.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LossCategory {
    /// Gate decomposed into native gates (adds overhead).
    Decomposition,
    /// Gate lowered to a different abstraction level.
    Lowering,
    /// Feature not supported by the target.
    Unsupported,
}

/// Result of attempting QASM3 emission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmissionResult {
    /// Whether emission to QASM3 was successful.
    pub success: bool,
    /// Number of lines in the emitted output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_count: Option<usize>,
    /// Error message (if emission failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Complete emitter compliance report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmitterReport {
    /// Target backend name.
    pub target: String,
    /// Gate-by-gate materialization analysis.
    pub gate_materializations: Vec<GateMaterialization>,
    /// Coverage metrics.
    pub coverage: CoverageMetrics,
    /// Loss documentation.
    pub losses: Vec<LossRecord>,
    /// QASM3 emission result.
    pub emission: EmissionResult,
    /// Whether the circuit is fully materializable (no lost gates).
    pub fully_materializable: bool,
}

/// The emitter compliance analyzer.
pub struct EmitterAnalyzer;

impl EmitterAnalyzer {
    /// Analyze emitter compliance for a compiled circuit against a target backend.
    pub fn analyze(
        dag: &CircuitDag,
        emit_target: &EmitTarget,
        _capabilities: &Capabilities,
    ) -> EvalResult<EmitterReport> {
        let native_gates = emit_target.native_gates();

        // 1. Analyze gate materialization
        let (gate_materializations, coverage) = Self::analyze_gates(dag, native_gates);

        // 2. Document losses
        let losses = Self::document_losses(&gate_materializations, emit_target);

        // 3. Attempt QASM3 emission
        let emission = Self::attempt_emission(dag);

        let fully_materializable = coverage.lost_count == 0;

        Ok(EmitterReport {
            target: emit_target.display_name().to_string(),
            gate_materializations,
            coverage,
            losses,
            emission,
            fully_materializable,
        })
    }

    fn analyze_gates(
        dag: &CircuitDag,
        native_gates: &[&str],
    ) -> (Vec<GateMaterialization>, CoverageMetrics) {
        let mut gate_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut total_gates = 0usize;

        for (_idx, inst) in dag.topological_ops() {
            if let InstructionKind::Gate(gate) = &inst.kind {
                let name = gate.name().to_string();
                *gate_counts.entry(name).or_insert(0) += 1;
                total_gates += 1;
            }
        }

        let mut materializations = Vec::new();
        let mut native_count = 0usize;
        let mut decomposed_count = 0usize;
        let mut lost_count = 0usize;
        let mut total_decomposition_cost = 0usize;

        for (gate_name, count) in &gate_counts {
            let (status, decomp_cost, note) = if native_gates.contains(&gate_name.as_str()) {
                (
                    MaterializationStatus::Native,
                    None,
                    format!("'{}' is native on target", gate_name),
                )
            } else if let Some(cost) = decomposition_cost(gate_name) {
                (
                    MaterializationStatus::Decomposed,
                    Some(cost),
                    format!("'{}' decomposes into ~{} native gates", gate_name, cost),
                )
            } else {
                (
                    MaterializationStatus::Lost,
                    None,
                    format!("'{}' cannot be materialized for this target", gate_name),
                )
            };

            match &status {
                MaterializationStatus::Native => native_count += count,
                MaterializationStatus::Decomposed => {
                    decomposed_count += count;
                    if let Some(cost) = decomp_cost {
                        total_decomposition_cost += cost * count;
                    }
                }
                MaterializationStatus::Lost => lost_count += count,
            }

            materializations.push(GateMaterialization {
                gate_name: gate_name.clone(),
                count: *count,
                status,
                decomposition_cost: decomp_cost,
                note,
            });
        }

        let native_coverage = if total_gates > 0 {
            native_count as f64 / total_gates as f64
        } else {
            1.0
        };

        let materializable_coverage = if total_gates > 0 {
            (native_count + decomposed_count) as f64 / total_gates as f64
        } else {
            1.0
        };

        let estimated_expansion = if total_gates > 0 {
            (native_count + total_decomposition_cost) as f64 / total_gates as f64
        } else {
            1.0
        };

        let coverage = CoverageMetrics {
            total_gates,
            native_count,
            decomposed_count,
            lost_count,
            native_coverage,
            materializable_coverage,
            estimated_expansion,
        };

        (materializations, coverage)
    }

    fn document_losses(
        materializations: &[GateMaterialization],
        emit_target: &EmitTarget,
    ) -> Vec<LossRecord> {
        let mut losses = Vec::new();

        for mat in materializations {
            match mat.status {
                MaterializationStatus::Decomposed => {
                    losses.push(LossRecord {
                        capability: mat.gate_name.clone(),
                        category: LossCategory::Decomposition,
                        impact: format!(
                            "{} occurrence(s) of '{}' require decomposition (~{} native gates each)",
                            mat.count,
                            mat.gate_name,
                            mat.decomposition_cost.unwrap_or(0),
                        ),
                        native_gate_cost: mat.decomposition_cost,
                    });
                }
                MaterializationStatus::Lost => {
                    losses.push(LossRecord {
                        capability: mat.gate_name.clone(),
                        category: LossCategory::Unsupported,
                        impact: format!(
                            "{} occurrence(s) of '{}' cannot be mapped to {}",
                            mat.count,
                            mat.gate_name,
                            emit_target.display_name(),
                        ),
                        native_gate_cost: None,
                    });
                }
                MaterializationStatus::Native => {}
            }
        }

        losses
    }

    fn attempt_emission(dag: &CircuitDag) -> EmissionResult {
        let circuit = Circuit::from_dag(dag.clone());
        match arvak_qasm3::emit(&circuit) {
            Ok(qasm3) => {
                let line_count = qasm3.lines().count();
                EmissionResult {
                    success: true,
                    line_count: Some(line_count),
                    error: None,
                }
            }
            Err(e) => EmissionResult {
                success: false,
                line_count: None,
                error: Some(e.to_string()),
            },
        }
    }
}

/// Estimated decomposition cost (in native gates) for common gates.
fn decomposition_cost(gate_name: &str) -> Option<usize> {
    match gate_name {
        // Single-qubit: decompose into 1-3 rotation gates
        "h" => Some(3),    // Rz + Ry or equivalent
        "x" => Some(1),    // Rx(pi) or PRX(pi, 0)
        "y" => Some(1),    // Ry(pi) or PRX(pi, pi/2)
        "z" => Some(1),    // Rz(pi) or virtual-Z
        "s" => Some(1),    // Rz(pi/2)
        "sdg" => Some(1),  // Rz(-pi/2)
        "t" => Some(1),    // Rz(pi/4)
        "tdg" => Some(1),  // Rz(-pi/4)
        "sx" => Some(1),   // Rx(pi/2)
        "sxdg" => Some(1), // Rx(-pi/2)
        "rx" => Some(1),   // PRX(theta, 0)
        "ry" => Some(1),   // PRX(theta, pi/2)
        "rz" => Some(1),   // Virtual-Z or PRX sequence
        "p" => Some(1),    // Phase gate -> Rz
        "u" => Some(3),    // U3 -> 3 rotations
        "id" => Some(0),   // Identity -> no-op
        // Two-qubit gates
        "cx" => Some(3),    // CZ + H on target
        "cy" => Some(5),    // CZ + rotations
        "cz" => Some(1),    // Often native (IQM)
        "ch" => Some(7),    // Complex decomposition
        "swap" => Some(3),  // 3 CX or 3 CZ + singles
        "iswap" => Some(2), // CZ + singles
        "crx" => Some(5),   // CZ + rotations
        "cry" => Some(5),   // CZ + rotations
        "crz" => Some(3),   // CZ + Rz
        "cp" => Some(3),    // CZ + phase
        // Interaction gates
        "rxx" => Some(6), // 2 CX + rotations
        "ryy" => Some(6), // 2 CX + rotations
        "rzz" => Some(4), // 2 CX + Rz
        // Three-qubit gates
        "ccx" => Some(15),   // Toffoli -> ~15 gates
        "cswap" => Some(17), // Fredkin -> ~17 gates
        // PRX (IQM native)
        "prx" => Some(1), // Often native
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arvak_ir::{Circuit, ClbitId, QubitId};

    #[test]
    fn test_emit_target_from_name() {
        assert_eq!(EmitTarget::from_name("iqm"), Some(EmitTarget::Iqm));
        assert_eq!(EmitTarget::from_name("ibm"), Some(EmitTarget::Ibm));
        assert_eq!(EmitTarget::from_name("cuda-q"), Some(EmitTarget::CudaQ));
        assert_eq!(EmitTarget::from_name("cudaq"), Some(EmitTarget::CudaQ));
        assert_eq!(EmitTarget::from_name("simulator"), Some(EmitTarget::CudaQ));
        assert_eq!(EmitTarget::from_name("unknown"), None);
    }

    #[test]
    fn test_iqm_coverage_bell() {
        // Bell state: H, CX — neither native on IQM
        let mut circuit = Circuit::with_size("bell", 2, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();

        let dag = circuit.into_dag();
        let caps = Capabilities::iqm("test", 20);
        let report = EmitterAnalyzer::analyze(&dag, &EmitTarget::Iqm, &caps).unwrap();

        assert_eq!(report.coverage.total_gates, 2);
        assert_eq!(report.coverage.native_count, 0);
        assert_eq!(report.coverage.decomposed_count, 2);
        assert_eq!(report.coverage.lost_count, 0);
        assert!(report.fully_materializable);
        assert!(report.coverage.native_coverage < 0.01);
        assert!((report.coverage.materializable_coverage - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_ibm_coverage_bell() {
        // H not native on IBM, CX is native
        let mut circuit = Circuit::with_size("bell", 2, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();

        let dag = circuit.into_dag();
        let caps = Capabilities::ibm("test", 20);
        let report = EmitterAnalyzer::analyze(&dag, &EmitTarget::Ibm, &caps).unwrap();

        assert_eq!(report.coverage.total_gates, 2);
        assert_eq!(report.coverage.native_count, 1); // CX
        assert_eq!(report.coverage.decomposed_count, 1); // H
        assert!(report.fully_materializable);
    }

    #[test]
    fn test_cudaq_all_native() {
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();

        let dag = circuit.into_dag();
        let caps = Capabilities::simulator(10);
        let report = EmitterAnalyzer::analyze(&dag, &EmitTarget::CudaQ, &caps).unwrap();

        assert_eq!(report.coverage.native_count, 2);
        assert_eq!(report.coverage.decomposed_count, 0);
        assert!((report.coverage.native_coverage - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_emission_success() {
        let mut circuit = Circuit::with_size("test", 2, 2);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.measure(QubitId(0), ClbitId(0)).unwrap();

        let dag = circuit.into_dag();
        let caps = Capabilities::simulator(10);
        let report = EmitterAnalyzer::analyze(&dag, &EmitTarget::CudaQ, &caps).unwrap();

        assert!(report.emission.success);
        assert!(report.emission.line_count.unwrap() > 0);
    }

    #[test]
    fn test_loss_documentation() {
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();

        let dag = circuit.into_dag();
        let caps = Capabilities::iqm("test", 20);
        let report = EmitterAnalyzer::analyze(&dag, &EmitTarget::Iqm, &caps).unwrap();

        // Both H and CX need decomposition on IQM
        assert!(!report.losses.is_empty());
        assert!(
            report
                .losses
                .iter()
                .all(|l| l.category == LossCategory::Decomposition)
        );
    }

    #[test]
    fn test_decomposition_costs() {
        assert_eq!(decomposition_cost("h"), Some(3));
        assert_eq!(decomposition_cost("cx"), Some(3));
        assert_eq!(decomposition_cost("ccx"), Some(15));
        assert_eq!(decomposition_cost("id"), Some(0));
        assert_eq!(decomposition_cost("unknown_gate"), None);
    }

    #[test]
    fn test_coverage_empty_circuit() {
        let circuit = Circuit::with_size("empty", 1, 0);
        let dag = circuit.into_dag();
        let caps = Capabilities::simulator(10);
        let report = EmitterAnalyzer::analyze(&dag, &EmitTarget::CudaQ, &caps).unwrap();

        assert_eq!(report.coverage.total_gates, 0);
        assert!((report.coverage.native_coverage - 1.0).abs() < 1e-10);
        assert!(report.fully_materializable);
    }

    #[test]
    fn test_expansion_factor() {
        // IQM: H (cost 3) + CX (cost 3) = 6 native gates from 2 original
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();

        let dag = circuit.into_dag();
        let caps = Capabilities::iqm("test", 20);
        let report = EmitterAnalyzer::analyze(&dag, &EmitTarget::Iqm, &caps).unwrap();

        // (0 native + 6 decomposition cost) / 2 total = 3.0
        assert!(report.coverage.estimated_expansion > 1.0);
    }
}
