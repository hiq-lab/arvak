//! Noise injection pass: maps hardware noise profile to IR noise channels.
//!
//! Reads a [`NoiseProfile`] from the [`PropertySet`] and injects
//! `Deficit`-tagged noise channels into the circuit DAG.

use arvak_ir::CircuitDag;
use arvak_ir::instruction::{Instruction, InstructionKind};
use arvak_ir::noise::{NoiseModel, NoiseProfile, NoiseRole};

use crate::error::CompileResult;
use crate::pass::{Pass, PassKind};
use crate::property::PropertySet;

/// Injects hardware noise channels into the circuit DAG.
///
/// Reads a [`NoiseProfile`] from the property set and inserts
/// `Deficit`-tagged noise channels after gates and before measurements.
pub struct NoiseInjectionPass;

impl Default for NoiseInjectionPass {
    fn default() -> Self {
        Self::new()
    }
}

impl NoiseInjectionPass {
    /// Create a new noise injection pass.
    pub fn new() -> Self {
        Self
    }
}

impl Pass for NoiseInjectionPass {
    fn name(&self) -> &'static str {
        "NoiseInjection"
    }

    fn kind(&self) -> PassKind {
        PassKind::Transformation
    }

    fn should_run(&self, _dag: &CircuitDag, properties: &PropertySet) -> bool {
        properties
            .get::<NoiseProfile>()
            .is_some_and(|p| !p.is_empty())
    }

    fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()> {
        let profile = match properties.get::<NoiseProfile>() {
            Some(p) => p.clone(),
            None => return Ok(()),
        };

        // Collect injection points (can't mutate DAG while iterating)
        let mut gate_injections: Vec<Instruction> = Vec::new();
        let mut readout_injections: Vec<Instruction> = Vec::new();

        for (_idx, inst) in dag.topological_ops() {
            match &inst.kind {
                InstructionKind::Gate(gate) => {
                    let gate_name = gate.name();
                    if let Some(error_rate) = profile.gate_error(gate_name) {
                        if error_rate > 0.0 {
                            for &qubit in &inst.qubits {
                                gate_injections.push(Instruction::noise_channel(
                                    NoiseModel::Depolarizing { p: error_rate },
                                    NoiseRole::Deficit,
                                    qubit,
                                ));
                            }
                        }
                    }
                }
                InstructionKind::Measure => {
                    for &qubit in &inst.qubits {
                        // Note: This uses the logical qubit ID as the physical qubit
                        // index into the readout error array. This assumes that logical
                        // qubit IDs correspond to physical qubit indices, which is true
                        // when this pass runs before layout/routing or when a trivial
                        // layout (logical == physical) is in effect.
                        let qubit_idx = qubit.0 as usize;
                        if let Some(readout_err) = profile.qubit_readout_error(qubit_idx) {
                            if readout_err > 0.0 {
                                readout_injections.push(Instruction::noise_channel(
                                    NoiseModel::ReadoutError { p: readout_err },
                                    NoiseRole::Deficit,
                                    qubit,
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Apply injections
        for inst in gate_injections {
            dag.apply(inst)?;
        }
        for inst in readout_injections {
            dag.apply(inst)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arvak_ir::{Circuit, QubitId};

    fn sample_profile() -> NoiseProfile {
        let mut profile = NoiseProfile::new();
        profile.gate_errors.insert("h".into(), 0.001);
        profile.gate_errors.insert("cx".into(), 0.01);
        profile.readout_errors = Some(vec![0.02, 0.03]);
        profile
    }

    #[test]
    fn test_should_not_run_without_profile() {
        let dag = CircuitDag::new();
        let props = PropertySet::new();
        let pass = NoiseInjectionPass::new();
        assert!(!pass.should_run(&dag, &props));
    }

    #[test]
    fn test_should_not_run_with_empty_profile() {
        let dag = CircuitDag::new();
        let mut props = PropertySet::new();
        props.insert(NoiseProfile::new());
        let pass = NoiseInjectionPass::new();
        assert!(!pass.should_run(&dag, &props));
    }

    #[test]
    fn test_should_run_with_profile() {
        let dag = CircuitDag::new();
        let mut props = PropertySet::new();
        props.insert(sample_profile());
        let pass = NoiseInjectionPass::new();
        assert!(pass.should_run(&dag, &props));
    }

    #[test]
    fn test_injects_gate_noise() {
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        let initial_ops = dag.num_ops();
        assert_eq!(initial_ops, 2);

        let mut props = PropertySet::new();
        props.insert(sample_profile());

        NoiseInjectionPass::new().run(&mut dag, &mut props).unwrap();

        // H on q0 → 1 depolarizing, CX on q0,q1 → 2 depolarizing
        assert_eq!(dag.num_ops(), 5);
    }

    #[test]
    fn test_injects_readout_noise() {
        use arvak_ir::ClbitId;

        let mut circuit = Circuit::with_size("test", 2, 2);
        circuit.h(QubitId(0)).unwrap();
        circuit.measure(QubitId(0), ClbitId(0)).unwrap();
        circuit.measure(QubitId(1), ClbitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        assert_eq!(dag.num_ops(), 3);

        let mut props = PropertySet::new();
        props.insert(sample_profile());

        NoiseInjectionPass::new().run(&mut dag, &mut props).unwrap();

        // H→1 depolarizing, 2 measures→2 readout errors = 3+3=6
        assert_eq!(dag.num_ops(), 6);
    }

    #[test]
    fn test_no_injection_for_unknown_gates() {
        let mut circuit = Circuit::with_size("test", 1, 0);
        circuit.t(QubitId(0)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new();
        props.insert(sample_profile());

        NoiseInjectionPass::new().run(&mut dag, &mut props).unwrap();

        assert_eq!(dag.num_ops(), 1);
    }
}
