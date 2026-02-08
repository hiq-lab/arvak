//! Quantum circuit generators for demos.

pub mod grover;
pub mod qaoa;
pub mod qi_nutshell;
pub mod vqe;

pub use grover::grover_circuit;
pub use qaoa::{
    InitStrategy, ParameterBounds, graph_aware_initial_parameters,
    initial_parameters_with_strategy, qaoa_circuit,
};
pub use qi_nutshell::{
    Basis, EveStrategy, bb84_circuit, bb84_multi_round, bb84_qec_circuit, bbm92_circuit,
    optimal_symmetric_angle, pccm_fidelities, pccm_qber,
};
pub use vqe::two_local_ansatz;
