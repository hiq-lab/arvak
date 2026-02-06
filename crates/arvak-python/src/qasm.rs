//! QASM3 parsing and emission functions.

use pyo3::prelude::*;

use crate::circuit::PyCircuit;
use crate::error::parse_to_py_err;

/// Parse an OpenQASM 3 string into a Circuit.
///
/// Args:
///     qasm: The QASM3 source code as a string.
///
/// Returns:
///     A Circuit object representing the parsed circuit.
///
/// Raises:
///     RuntimeError: If parsing fails due to syntax errors or unsupported features.
///
/// Example:
///     >>> qasm = '''
///     ... OPENQASM 3.0;
///     ... qubit[2] q;
///     ... h q[0];
///     ... cx q[0], q[1];
///     ... '''
///     >>> qc = from_qasm(qasm)
///     >>> qc.num_qubits
///     2
#[pyfunction]
pub fn from_qasm(qasm: &str) -> PyResult<PyCircuit> {
    let circuit = hiq_qasm3::parse(qasm).map_err(parse_to_py_err)?;
    Ok(PyCircuit { inner: circuit })
}

/// Emit a Circuit as an OpenQASM 3 string.
///
/// Args:
///     circuit: The Circuit to convert to QASM.
///
/// Returns:
///     A string containing the QASM3 representation of the circuit.
///
/// Example:
///     >>> qc = Circuit("test", num_qubits=2)
///     >>> qc.h(0).cx(0, 1)
///     >>> print(to_qasm(qc))
///     OPENQASM 3.0;
///     qubit[2] q;
///     h q[0];
///     cx q[0], q[1];
#[pyfunction]
pub fn to_qasm(circuit: &PyCircuit) -> PyResult<String> {
    hiq_qasm3::emit(&circuit.inner).map_err(parse_to_py_err)
}
