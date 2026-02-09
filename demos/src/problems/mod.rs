//! Problem definitions for quantum algorithms.

pub mod hamiltonian;
pub mod maxcut;
pub mod molecules;
pub mod sensor_assignment;

pub use hamiltonian::{Pauli, PauliHamiltonian, PauliTerm};
pub use maxcut::Graph;
pub use molecules::{
    beh2_hamiltonian, exact_ground_state_energy, h2_hamiltonian, h2_hamiltonian_4q,
    h2o_hamiltonian, lih_hamiltonian,
};
pub use sensor_assignment::{
    drone_patrol_6, radar_deconfliction_8, random_sensor_network, surveillance_grid_10,
};
