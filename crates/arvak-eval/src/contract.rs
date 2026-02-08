//! QDMI Contract Checker: safety classification against device capabilities.
//!
//! Evaluates every operation in a compiled circuit against the target
//! device's capability contract and assigns a safety tag:
//!
//! - **Safe**: Gate is natively supported by the target.
//! - **Conditional**: Gate may be supported through decomposition.
//! - **Violating**: Gate cannot be executed on the target.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use arvak_hal::Capabilities;
use arvak_ir::CircuitDag;
use arvak_ir::instruction::InstructionKind;

/// Safety classification for a gate against a QDMI contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SafetyTag {
    /// Gate is natively supported.
    Safe,
    /// Gate may be supported through decomposition or lowering.
    Conditional,
    /// Gate violates the device contract.
    Violating,
}

impl std::fmt::Display for SafetyTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SafetyTag::Safe => write!(f, "safe"),
            SafetyTag::Conditional => write!(f, "conditional"),
            SafetyTag::Violating => write!(f, "violating"),
        }
    }
}

/// Result of checking a single operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCheck {
    /// Name of the gate/operation.
    pub gate_name: String,
    /// Safety classification.
    pub tag: SafetyTag,
    /// Reason for the classification.
    pub reason: String,
}

/// Full QDMI contract compliance report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractReport {
    /// Target backend name.
    pub target_name: String,
    /// Target qubit count.
    pub target_qubits: u32,
    /// Number of safe operations.
    pub safe_count: usize,
    /// Number of conditionally safe operations.
    pub conditional_count: usize,
    /// Number of violating operations.
    pub violating_count: usize,
    /// Overall compliance status.
    pub compliant: bool,
    /// Per-gate-type checks.
    pub gate_checks: Vec<GateCheck>,
    /// Summary by gate type.
    pub gate_summary: BTreeMap<String, SafetyTag>,
}

/// The contract checker.
pub struct ContractChecker;

impl ContractChecker {
    /// Check a compiled circuit DAG against device capabilities.
    pub fn check(dag: &CircuitDag, capabilities: &Capabilities) -> ContractReport {
        let mut gate_checks = Vec::new();
        let mut gate_summary: BTreeMap<String, SafetyTag> = BTreeMap::new();
        let mut safe_count = 0usize;
        let mut conditional_count = 0usize;
        let mut violating_count = 0usize;

        // Collect unique gate types and their checks
        for (_idx, inst) in dag.topological_ops() {
            let (gate_name, tag, reason) = match &inst.kind {
                InstructionKind::Gate(gate) => {
                    let name = gate.name().to_string();
                    let tag = classify_gate(&name, inst.qubits.len(), capabilities);
                    let reason =
                        explain_classification(&name, inst.qubits.len(), &tag, capabilities);
                    (name, tag, reason)
                }
                InstructionKind::Measure => (
                    "measure".into(),
                    SafetyTag::Safe,
                    "Measurement is universally supported".into(),
                ),
                InstructionKind::Reset => (
                    "reset".into(),
                    SafetyTag::Safe,
                    "Reset is universally supported".into(),
                ),
                InstructionKind::Barrier => (
                    "barrier".into(),
                    SafetyTag::Safe,
                    "Barrier is a scheduling directive".into(),
                ),
                InstructionKind::Delay { .. } => (
                    "delay".into(),
                    SafetyTag::Conditional,
                    "Delay support depends on backend".into(),
                ),
                InstructionKind::Shuttle { .. } => {
                    let tag = if capabilities.features.contains(&"shuttling".to_string()) {
                        SafetyTag::Safe
                    } else {
                        SafetyTag::Violating
                    };
                    (
                        "shuttle".into(),
                        tag,
                        format!("Shuttle requires shuttling capability"),
                    )
                }
            };

            match tag {
                SafetyTag::Safe => safe_count += 1,
                SafetyTag::Conditional => conditional_count += 1,
                SafetyTag::Violating => violating_count += 1,
            }

            // Record per-gate-type summary (worst tag wins)
            gate_summary
                .entry(gate_name.clone())
                .and_modify(|existing| {
                    *existing = worst_tag(*existing, tag);
                })
                .or_insert(tag);

            gate_checks.push(GateCheck {
                gate_name,
                tag,
                reason,
            });
        }

        let compliant = violating_count == 0;

        ContractReport {
            target_name: capabilities.name.clone(),
            target_qubits: capabilities.num_qubits,
            safe_count,
            conditional_count,
            violating_count,
            compliant,
            gate_checks,
            gate_summary,
        }
    }
}

/// Classify a gate against device capabilities.
fn classify_gate(gate_name: &str, num_qubits: usize, caps: &Capabilities) -> SafetyTag {
    // Check if gate is in the native gate set
    if caps.gate_set.native.iter().any(|g| g == gate_name) {
        return SafetyTag::Safe;
    }

    // Check if gate is in the supported (non-native) gate set
    if caps.gate_set.contains(gate_name) {
        return SafetyTag::Safe;
    }

    // Check qubit connectivity for multi-qubit gates
    if num_qubits >= 2 && caps.num_qubits == 0 {
        return SafetyTag::Violating;
    }

    // Known decomposable gates
    if is_decomposable(gate_name) {
        return SafetyTag::Conditional;
    }

    SafetyTag::Violating
}

/// Generate a human-readable explanation for a classification.
fn explain_classification(
    gate_name: &str,
    _num_qubits: usize,
    tag: &SafetyTag,
    caps: &Capabilities,
) -> String {
    match tag {
        SafetyTag::Safe => {
            if caps.gate_set.native.iter().any(|g| g == gate_name) {
                format!("'{}' is a native gate on {}", gate_name, caps.name)
            } else {
                format!("'{}' is supported by {}", gate_name, caps.name)
            }
        }
        SafetyTag::Conditional => {
            format!(
                "'{}' is not natively supported but can be decomposed into native gates on {}",
                gate_name, caps.name
            )
        }
        SafetyTag::Violating => {
            format!(
                "'{}' is not supported and cannot be decomposed for {}",
                gate_name, caps.name
            )
        }
    }
}

/// Check if a gate is known to be decomposable into common native gate sets.
fn is_decomposable(gate_name: &str) -> bool {
    matches!(
        gate_name,
        "h" | "x"
            | "y"
            | "z"
            | "s"
            | "sdg"
            | "t"
            | "tdg"
            | "sx"
            | "sxdg"
            | "rx"
            | "ry"
            | "rz"
            | "p"
            | "u"
            | "cx"
            | "cy"
            | "cz"
            | "ch"
            | "swap"
            | "iswap"
            | "crx"
            | "cry"
            | "crz"
            | "cp"
            | "rxx"
            | "ryy"
            | "rzz"
            | "ccx"
            | "cswap"
            | "prx"
            | "id"
    )
}

/// Return the worse of two safety tags.
fn worst_tag(a: SafetyTag, b: SafetyTag) -> SafetyTag {
    match (a, b) {
        (SafetyTag::Violating, _) | (_, SafetyTag::Violating) => SafetyTag::Violating,
        (SafetyTag::Conditional, _) | (_, SafetyTag::Conditional) => SafetyTag::Conditional,
        _ => SafetyTag::Safe,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arvak_ir::{Circuit, ClbitId, QubitId};

    fn iqm_caps() -> Capabilities {
        Capabilities::iqm("IQM Garnet", 20)
    }

    fn simulator_caps() -> Capabilities {
        Capabilities::simulator(10)
    }

    #[test]
    fn test_safe_gate() {
        let caps = iqm_caps();
        assert_eq!(classify_gate("prx", 1, &caps), SafetyTag::Safe);
        assert_eq!(classify_gate("cz", 2, &caps), SafetyTag::Safe);
    }

    #[test]
    fn test_conditional_gate() {
        let caps = iqm_caps();
        // CX is decomposable into CZ + single-qubit gates
        assert_eq!(classify_gate("cx", 2, &caps), SafetyTag::Conditional);
        assert_eq!(classify_gate("h", 1, &caps), SafetyTag::Conditional);
    }

    #[test]
    fn test_simulator_all_safe() {
        let caps = simulator_caps();
        assert_eq!(classify_gate("h", 1, &caps), SafetyTag::Safe);
        assert_eq!(classify_gate("cx", 2, &caps), SafetyTag::Safe);
        // CCX is a 3-qubit gate not listed in GateSet, but is decomposable
        assert_eq!(classify_gate("ccx", 3, &caps), SafetyTag::Conditional);
    }

    #[test]
    fn test_contract_check_bell_state() {
        let mut circuit = Circuit::with_size("bell", 2, 2);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.measure(QubitId(0), ClbitId(0)).unwrap();
        circuit.measure(QubitId(1), ClbitId(1)).unwrap();

        let dag = circuit.into_dag();
        let caps = simulator_caps();
        let report = ContractChecker::check(&dag, &caps);

        assert!(report.compliant);
        assert!(report.violating_count == 0);
    }

    #[test]
    fn test_contract_check_iqm() {
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();

        let dag = circuit.into_dag();
        let caps = iqm_caps();
        let report = ContractChecker::check(&dag, &caps);

        // H and CX are not native on IQM, but are decomposable
        assert!(report.conditional_count > 0);
        assert_eq!(report.violating_count, 0);
    }

    #[test]
    fn test_worst_tag() {
        assert_eq!(worst_tag(SafetyTag::Safe, SafetyTag::Safe), SafetyTag::Safe);
        assert_eq!(
            worst_tag(SafetyTag::Safe, SafetyTag::Conditional),
            SafetyTag::Conditional
        );
        assert_eq!(
            worst_tag(SafetyTag::Conditional, SafetyTag::Violating),
            SafetyTag::Violating
        );
    }
}
