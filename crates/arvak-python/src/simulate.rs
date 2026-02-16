//! Python bindings for the local simulator.
//!
//! Exposes `run_sim(circuit, shots) -> dict` which calls the Rust statevector
//! simulator directly (no gRPC, no async runtime).

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::circuit::PyCircuit;

/// Run a circuit on the built-in statevector simulator.
///
/// Returns a dictionary mapping bitstrings to counts, e.g.
/// `{"00": 487, "11": 513}` for a Bell state with 1000 shots.
///
/// This calls the Rust simulator directly — no network, no async runtime,
/// no external dependencies.
///
/// # Arguments
/// * `circuit` - An Arvak Circuit object
/// * `shots` - Number of measurement shots (1–1_000_000)
///
/// # Raises
/// * `RuntimeError` - If the circuit has too many qubits (>20) or shots is 0
///
/// # Example
/// ```python
/// import arvak
/// bell = arvak.Circuit.bell()
/// counts = arvak.run_sim(bell, 1000)
/// print(counts)  # {'00': 512, '11': 488}
/// ```
#[pyfunction]
#[pyo3(signature = (circuit, shots=1024))]
pub fn run_sim(circuit: &PyCircuit, shots: u32, py: Python<'_>) -> PyResult<Py<PyDict>> {
    if shots == 0 {
        return Err(pyo3::exceptions::PyValueError::new_err("shots must be > 0"));
    }

    #[cfg(feature = "simulator")]
    {
        use arvak_adapter_sim::SimulatorBackend;

        let backend = SimulatorBackend::new();

        // Validate circuit size
        if circuit.inner.num_qubits() > 20 {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Circuit has {} qubits but the built-in simulator supports up to 20. \
                 Use SimulatorBackend::with_max_qubits() for larger circuits (slow).",
                circuit.inner.num_qubits()
            )));
        }

        // Release the GIL during simulation (may take a while for many shots)
        let circuit_clone = circuit.inner.clone();
        let result = py.detach(move || backend.run_simulation(&circuit_clone, shots));

        let result = result.map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Simulation failed: {e}"))
        })?;

        // Convert Counts → Python dict
        let dict = PyDict::new(py);
        for (bitstring, count) in result.counts.iter() {
            dict.set_item(bitstring, count)?;
        }

        Ok(dict.into())
    }

    #[cfg(not(feature = "simulator"))]
    {
        let _ = (circuit, shots, py);
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "Simulator not available. Rebuild arvak with the 'simulator' feature enabled.",
        ))
    }
}
