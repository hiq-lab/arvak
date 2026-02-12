//! Property-based tests for QASM3 roundtrip conversion.
//!
//! Tests that circuit → QASM3 → circuit preserves circuit structure and semantics.

use arvak_ir::{Circuit, QubitId};
use arvak_qasm3::{emit, parse};
use proptest::prelude::*;

/// Generate a random simple circuit for property testing.
///
/// Generates circuits with:
/// - 1-5 qubits
/// - 1-10 gates from a basic gate set (H, X, Y, Z, CX)
/// - Classical bits for measurements
fn arb_simple_circuit() -> impl Strategy<Value = Circuit> {
    (1_u32..=5).prop_flat_map(|num_qubits| {
        let num_clbits = num_qubits; // Match qubits for measurements
        (
            Just(num_qubits),
            Just(num_clbits),
            prop::collection::vec(arb_gate_op(num_qubits), 1..=10),
        )
            .prop_map(move |(nq, nc, ops)| {
                let mut circuit = Circuit::with_size("test", nq, nc);
                for op in ops {
                    op.apply(&mut circuit);
                }
                circuit
            })
    })
}

/// Gate operations that can be applied to a circuit.
#[derive(Debug, Clone)]
enum GateOp {
    H(u32),
    X(u32),
    Y(u32),
    Z(u32),
    CX(u32, u32),
}

impl GateOp {
    fn apply(self, circuit: &mut Circuit) {
        match self {
            GateOp::H(q) => {
                let _ = circuit.h(QubitId(q));
            }
            GateOp::X(q) => {
                let _ = circuit.x(QubitId(q));
            }
            GateOp::Y(q) => {
                let _ = circuit.y(QubitId(q));
            }
            GateOp::Z(q) => {
                let _ = circuit.z(QubitId(q));
            }
            GateOp::CX(q1, q2) => {
                let _ = circuit.cx(QubitId(q1), QubitId(q2));
            }
        }
    }
}

/// Generate a random gate operation for a circuit with given number of qubits.
fn arb_gate_op(num_qubits: u32) -> impl Strategy<Value = GateOp> {
    // For single-qubit circuits, only generate single-qubit gates
    if num_qubits < 2 {
        prop_oneof![
            (0..num_qubits).prop_map(GateOp::H),
            (0..num_qubits).prop_map(GateOp::X),
            (0..num_qubits).prop_map(GateOp::Y),
            (0..num_qubits).prop_map(GateOp::Z),
        ]
        .boxed()
    } else {
        prop_oneof![
            (0..num_qubits).prop_map(GateOp::H),
            (0..num_qubits).prop_map(GateOp::X),
            (0..num_qubits).prop_map(GateOp::Y),
            (0..num_qubits).prop_map(GateOp::Z),
            (0..num_qubits, 0..num_qubits)
                .prop_filter("Control and target must differ", |(c, t)| c != t)
                .prop_map(|(c, t)| GateOp::CX(c, t)),
        ]
        .boxed()
    }
}

proptest! {
    /// Test that circuit → QASM3 → circuit roundtrip preserves circuit structure.
    ///
    /// Properties verified:
    /// - Number of qubits is preserved
    /// - Number of classical bits is preserved
    /// - Number of operations is preserved
    /// - Circuit depth is preserved
    #[test]
    fn test_circuit_qasm_roundtrip_preserves_structure(circuit in arb_simple_circuit()) {
        // Get original circuit properties
        let original_qubits = circuit.num_qubits();
        let original_clbits = circuit.num_clbits();
        let original_ops = circuit.dag().num_ops();
        let original_depth = circuit.depth();

        // Convert to QASM3
        let qasm = emit(&circuit).expect("Failed to convert circuit to QASM3");

        // Parse back to circuit
        let parsed_circuit = parse(&qasm).expect("Failed to parse QASM3 back to circuit");

        // Verify structure is preserved
        prop_assert_eq!(parsed_circuit.num_qubits(), original_qubits,
            "Qubit count mismatch after roundtrip");
        prop_assert_eq!(parsed_circuit.num_clbits(), original_clbits,
            "Classical bit count mismatch after roundtrip");
        prop_assert_eq!(parsed_circuit.dag().num_ops(), original_ops,
            "Operation count mismatch after roundtrip");
        prop_assert_eq!(parsed_circuit.depth(), original_depth,
            "Circuit depth mismatch after roundtrip");
    }

    /// Test that converting an empty circuit works correctly.
    #[test]
    fn test_empty_circuit_roundtrip(num_qubits in 1_u32..=10, num_clbits in 0_u32..=10) {
        let circuit = Circuit::with_size("empty", num_qubits, num_clbits);

        let qasm = emit(&circuit).expect("Failed to convert empty circuit to QASM3");
        let parsed = parse(&qasm).expect("Failed to parse empty circuit QASM3");

        prop_assert_eq!(parsed.num_qubits(), num_qubits as usize);
        prop_assert_eq!(parsed.num_clbits(), num_clbits as usize);
        prop_assert_eq!(parsed.dag().num_ops(), 0);
    }

    /// Test that QASM3 generation is deterministic.
    ///
    /// Converting the same circuit multiple times should produce identical QASM3.
    #[test]
    fn test_qasm_generation_is_deterministic(circuit in arb_simple_circuit()) {
        let qasm1 = emit(&circuit).expect("First conversion failed");
        let qasm2 = emit(&circuit).expect("Second conversion failed");

        prop_assert_eq!(qasm1, qasm2, "QASM3 generation is not deterministic");
    }
}
