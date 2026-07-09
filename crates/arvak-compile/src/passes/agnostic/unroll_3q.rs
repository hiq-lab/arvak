//! Decompose gates on three or more qubits before routing.
//!
//! The routing passes only understand one- and two-qubit operations:
//! `BasicRouting` leaves wider gates on non-adjacent qubits and
//! `SabreRouting` does not track them at all. Following the standard
//! pipeline design (cf. Qiskit's `Unroll3qOrMore`), everything wider than
//! two qubits is expanded into one- and two-qubit standard gates before
//! layout/routing runs.
//!
//! Found by property-based fuzzing (2026-07-08): a bare `ccx` compiled at
//! optimization level 0 produced CX gates on uncoupled qubit pairs.

use arvak_ir::{CircuitDag, GateKind, InstructionKind};

use crate::error::{CompileError, CompileResult};
use crate::pass::{Pass, PassKind};
use crate::passes::target::decompose_to_simpler;
use crate::property::PropertySet;

/// Expand >=3-qubit standard gates into 1q/2q gates.
pub struct Unroll3q;

impl Pass for Unroll3q {
    fn name(&self) -> &'static str {
        "Unroll3q"
    }

    fn kind(&self) -> PassKind {
        PassKind::Transformation
    }

    fn run(&self, dag: &mut CircuitDag, _properties: &mut PropertySet) -> CompileResult<()> {
        let needs_unroll = dag.topological_ops().any(|(_, inst)| {
            matches!(&inst.kind, InstructionKind::Gate(_)) && inst.qubits.len() >= 3
        });
        if !needs_unroll {
            return Ok(());
        }

        let mut new_dag = CircuitDag::new();
        for q in dag.qubits().collect::<Vec<_>>() {
            new_dag.add_qubit(q);
        }
        for c in dag.clbits().collect::<Vec<_>>() {
            new_dag.add_clbit(c);
        }

        for (_, inst) in dag.topological_ops() {
            let wide_standard = match &inst.kind {
                InstructionKind::Gate(g) if inst.qubits.len() >= 3 => match &g.kind {
                    GateKind::Standard(std_gate) => Some(std_gate.clone()),
                    GateKind::Custom(_) => {
                        return Err(CompileError::PassFailed {
                            name: "Unroll3q".into(),
                            reason: format!(
                                "cannot decompose {}-qubit custom gate '{}' for routing",
                                inst.qubits.len(),
                                g.name()
                            ),
                        });
                    }
                },
                _ => None,
            };

            if let Some(std_gate) = wide_standard {
                let steps = decompose_to_simpler(&std_gate, &inst.qubits).ok_or_else(|| {
                    CompileError::PassFailed {
                        name: "Unroll3q".into(),
                        reason: format!("no decomposition for wide gate '{std_gate:?}'"),
                    }
                })?;
                for step in steps {
                    debug_assert!(step.qubits.len() <= 2);
                    new_dag.apply(step).map_err(CompileError::Ir)?;
                }
            } else {
                new_dag.apply(inst.clone()).map_err(CompileError::Ir)?;
            }
        }

        new_dag.set_global_phase(dag.global_phase());
        new_dag.set_level(dag.level());
        *dag = new_dag;
        Ok(())
    }
}
