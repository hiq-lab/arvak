//! Input Module: QASM3 parsing, validation, hashing, and canonicalization.
//!
//! Analyzes the input circuit and produces structural metrics before
//! any compilation passes are applied.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use arvak_ir::Circuit;
use arvak_ir::instruction::InstructionKind;

use crate::error::{EvalError, EvalResult};

/// Structural metrics extracted from a circuit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuralMetrics {
    /// Number of qubits.
    pub num_qubits: usize,
    /// Number of classical bits.
    pub num_clbits: usize,
    /// Circuit depth.
    pub depth: usize,
    /// Total number of operations.
    pub total_ops: usize,
    /// Gate count by name.
    pub gate_counts: BTreeMap<String, usize>,
    /// Number of single-qubit gates.
    pub single_qubit_gates: usize,
    /// Number of two-qubit gates.
    pub two_qubit_gates: usize,
    /// Number of multi-qubit gates (3+).
    pub multi_qubit_gates: usize,
    /// Number of measurement operations.
    pub measurements: usize,
    /// Number of barrier operations.
    pub barriers: usize,
    /// Whether the circuit contains parameterized gates.
    pub has_parameters: bool,
}

/// Result of input analysis.
pub struct InputAnalysis {
    /// The parsed circuit (consumed by compilation).
    pub circuit: Circuit,
    /// SHA-256 hash of the raw input source.
    pub content_hash: String,
    /// Structural metrics of the input circuit.
    pub structural_metrics: StructuralMetrics,
}

impl InputAnalysis {
    /// Parse and analyze an `OpenQASM` 3.0 source string.
    pub fn analyze(qasm_source: &str) -> EvalResult<Self> {
        // Hash the raw input
        let content_hash = content_fingerprint(qasm_source);

        // Parse QASM3
        let circuit =
            arvak_qasm3::parse(qasm_source).map_err(|e| EvalError::Parse(e.to_string()))?;

        // Extract structural metrics
        let structural_metrics = extract_metrics(&circuit);

        Ok(Self {
            circuit,
            content_hash,
            structural_metrics,
        })
    }

    /// Convert to the serializable report form.
    pub fn into_report(self) -> InputReport {
        InputReport {
            content_hash: self.content_hash,
            num_qubits: self.structural_metrics.num_qubits,
            num_clbits: self.structural_metrics.num_clbits,
            depth: self.structural_metrics.depth,
            total_ops: self.structural_metrics.total_ops,
            gate_counts: self.structural_metrics.gate_counts,
            single_qubit_gates: self.structural_metrics.single_qubit_gates,
            two_qubit_gates: self.structural_metrics.two_qubit_gates,
            multi_qubit_gates: self.structural_metrics.multi_qubit_gates,
            measurements: self.structural_metrics.measurements,
            barriers: self.structural_metrics.barriers,
            has_parameters: self.structural_metrics.has_parameters,
        }
    }
}

/// Serializable input report section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputReport {
    /// SHA-256 hash of the raw input.
    pub content_hash: String,
    /// Number of qubits.
    pub num_qubits: usize,
    /// Number of classical bits.
    pub num_clbits: usize,
    /// Circuit depth.
    pub depth: usize,
    /// Total operation count.
    pub total_ops: usize,
    /// Gate counts by name.
    pub gate_counts: BTreeMap<String, usize>,
    /// Number of single-qubit gates.
    pub single_qubit_gates: usize,
    /// Number of two-qubit gates.
    pub two_qubit_gates: usize,
    /// Number of multi-qubit gates (3+).
    pub multi_qubit_gates: usize,
    /// Number of measurements.
    pub measurements: usize,
    /// Number of barriers.
    pub barriers: usize,
    /// Whether circuit has parameterized gates.
    pub has_parameters: bool,
}

/// Extract structural metrics from a circuit.
fn extract_metrics(circuit: &Circuit) -> StructuralMetrics {
    let dag = circuit.dag();
    let mut gate_counts = BTreeMap::new();
    let mut single_qubit_gates = 0usize;
    let mut two_qubit_gates = 0usize;
    let mut multi_qubit_gates = 0usize;
    let mut measurements = 0usize;
    let mut barriers = 0usize;
    let mut has_parameters = false;

    for (_idx, inst) in dag.topological_ops() {
        match &inst.kind {
            InstructionKind::Gate(gate) => {
                let name = gate.name().to_string();
                *gate_counts.entry(name).or_insert(0) += 1;

                match gate.num_qubits() {
                    1 => single_qubit_gates += 1,
                    2 => two_qubit_gates += 1,
                    _ => multi_qubit_gates += 1,
                }

                // Check for symbolic parameters
                if let arvak_ir::GateKind::Standard(std_gate) = &gate.kind {
                    if std_gate.is_parameterized() {
                        has_parameters = true;
                    }
                }
            }
            InstructionKind::Measure => measurements += 1,
            InstructionKind::Barrier => barriers += 1,
            _ => {}
        }
    }

    let total_ops =
        single_qubit_gates + two_qubit_gates + multi_qubit_gates + measurements + barriers;

    StructuralMetrics {
        num_qubits: circuit.num_qubits(),
        num_clbits: circuit.num_clbits(),
        depth: circuit.depth(),
        total_ops,
        gate_counts,
        single_qubit_gates,
        two_qubit_gates,
        multi_qubit_gates,
        measurements,
        barriers,
        has_parameters,
    }
}

/// Compute content fingerprint using fast non-cryptographic hashes.
///
/// Uses DJB2a + FNV-1a combined hash for a unique fingerprint.
/// Not cryptographic, but sufficient for content-addressed reproducibility.
fn content_fingerprint(input: &str) -> String {
    // DJB2a + FNV-1a combined hash for a unique fingerprint.
    // Not cryptographic, but sufficient for content-addressed reproducibility.
    let bytes = input.as_bytes();

    let mut h1: u64 = 5381;
    let mut h2: u64 = 0xcbf29ce484222325;

    for &b in bytes {
        // DJB2a
        h1 = h1.wrapping_mul(33) ^ u64::from(b);
        // FNV-1a
        h2 ^= u64::from(b);
        h2 = h2.wrapping_mul(0x100000001b3);
    }

    format!("{h1:016x}{h2:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const BELL_QASM: &str = r"
OPENQASM 3.0;
qubit[2] q;
bit[2] c;
h q[0];
cx q[0], q[1];
c = measure q;
";

    #[test]
    fn test_input_analysis() {
        let analysis = InputAnalysis::analyze(BELL_QASM).unwrap();
        assert_eq!(analysis.structural_metrics.num_qubits, 2);
        assert_eq!(analysis.structural_metrics.num_clbits, 2);
        assert!(analysis.structural_metrics.depth >= 2);
        assert_eq!(analysis.structural_metrics.single_qubit_gates, 1); // H
        assert_eq!(analysis.structural_metrics.two_qubit_gates, 1); // CX
        // `c = measure q;` on 2 qubits may produce 1 or 2 measurement ops
        assert!(analysis.structural_metrics.measurements >= 1);
        assert!(!analysis.content_hash.is_empty());
    }

    #[test]
    fn test_content_hash_deterministic() {
        let h1 = content_fingerprint(BELL_QASM);
        let h2 = content_fingerprint(BELL_QASM);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_content_hash_differs() {
        let h1 = content_fingerprint("OPENQASM 3.0;\nqubit[1] q;\nh q[0];");
        let h2 = content_fingerprint("OPENQASM 3.0;\nqubit[2] q;\nh q[0];");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_gate_counts() {
        let analysis = InputAnalysis::analyze(BELL_QASM).unwrap();
        assert_eq!(analysis.structural_metrics.gate_counts.get("h"), Some(&1));
        assert_eq!(analysis.structural_metrics.gate_counts.get("cx"), Some(&1));
    }

    #[test]
    fn test_ghz_circuit() {
        let qasm = r"
OPENQASM 3.0;
qubit[4] q;
h q[0];
cx q[0], q[1];
cx q[1], q[2];
cx q[2], q[3];
";
        let analysis = InputAnalysis::analyze(qasm).unwrap();
        assert_eq!(analysis.structural_metrics.num_qubits, 4);
        assert_eq!(analysis.structural_metrics.single_qubit_gates, 1);
        assert_eq!(analysis.structural_metrics.two_qubit_gates, 3);
    }
}
