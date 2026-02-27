//! `arvak-sim` — Hamiltonian time-evolution circuit synthesis.
//!
//! Converts a sum-of-Paulis Hamiltonian into an `arvak_ir::Circuit` that
//! approximates `exp(-i H t)` using:
//!
//! - **Trotter-Suzuki** product formulas (first- and second-order)
//! - **QDrift** randomised product formula (Campbell 2019)
//!
//! The resulting circuits are hardware-agnostic and can be passed directly
//! to any Arvak compiler pass (basis translation, routing, optimisation).
//!
//! # Quick start
//!
//! ```rust
//! use arvak_sim::hamiltonian::{Hamiltonian, HamiltonianTerm};
//! use arvak_sim::trotter::TrotterEvolution;
//!
//! // Transverse-field Ising model: H = -J·ZZ - h·X
//! let h = Hamiltonian::from_terms(vec![
//!     HamiltonianTerm::zz(0, 1, -1.0),   // -J ZZ
//!     HamiltonianTerm::x(0, -0.5),        // -h X₀
//!     HamiltonianTerm::x(1, -0.5),        // -h X₁
//! ]);
//!
//! let evol = TrotterEvolution::new(h, 1.0 /* t */, 10 /* steps */);
//! let circuit = evol.first_order().unwrap();
//! assert_eq!(circuit.num_qubits(), 2);
//! ```

pub mod error;
pub mod hamiltonian;
pub mod qdrift;
pub mod synthesis;
pub mod trotter;

pub use error::{SimError, SimResult};
pub use hamiltonian::{Hamiltonian, HamiltonianTerm, PauliOp, PauliString};
pub use qdrift::QDriftEvolution;
pub use trotter::TrotterEvolution;
