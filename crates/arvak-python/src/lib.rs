//! Python bindings for the Arvak quantum compilation platform.
//!
//! This crate provides Python bindings for the core Arvak types,
//! enabling quantum circuit construction, QASM I/O, and compilation.
//!
//! # Example
//!
//! ```python
//! import arvak
//!
//! # Create a Bell state circuit
//! qc = arvak.Circuit("bell", num_qubits=2)
//! qc.h(0).cx(0, 1).measure_all()
//!
//! # Convert to QASM
//! qasm = arvak.to_qasm(qc)
//! print(qasm)
//!
//! # Parse QASM back to circuit
//! qc2 = arvak.from_qasm(qasm)
//! ```

mod circuit;
mod compile;
mod error;
mod qasm;
mod qubits;
mod simulate;

use pyo3::prelude::*;

/// Arvak: Rust-native quantum compilation platform.
///
/// This module provides:
/// - Circuit: Quantum circuit builder with fluent API
/// - QubitId, ClbitId: Qubit and classical bit identifiers
/// - from_qasm, to_qasm: QASM3 parsing and emission
/// - Layout, CouplingMap, BasisGates, PropertySet: Compilation types
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Core types
    m.add_class::<qubits::PyQubitId>()?;
    m.add_class::<qubits::PyClbitId>()?;
    m.add_class::<circuit::PyCircuit>()?;

    // Compilation types
    m.add_class::<compile::PyLayout>()?;
    m.add_class::<compile::PyCouplingMap>()?;
    m.add_class::<compile::PyBasisGates>()?;
    m.add_class::<compile::PyPropertySet>()?;

    // QASM I/O functions
    m.add_function(wrap_pyfunction!(qasm::from_qasm, m)?)?;
    m.add_function(wrap_pyfunction!(qasm::to_qasm, m)?)?;

    // Simulation
    m.add_function(wrap_pyfunction!(simulate::run_sim, m)?)?;

    Ok(())
}
