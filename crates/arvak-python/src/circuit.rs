//! Python wrapper for the Circuit class.

use pyo3::prelude::*;

use crate::error::ir_to_py_err;
use crate::qubits::{PyClbitId, PyQubitId};

/// Convert a Python object to a QubitId.
/// Accepts either a QubitId or an integer.
fn to_qubit_id(obj: &Bound<'_, PyAny>) -> PyResult<arvak_ir::QubitId> {
    if let Ok(qid) = obj.extract::<PyQubitId>() {
        Ok(qid.into())
    } else if let Ok(index) = obj.extract::<u32>() {
        Ok(arvak_ir::QubitId(index))
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err(
            "Expected QubitId or int",
        ))
    }
}

/// Convert a Python object to a ClbitId.
/// Accepts either a ClbitId or an integer.
fn to_clbit_id(obj: &Bound<'_, PyAny>) -> PyResult<arvak_ir::ClbitId> {
    if let Ok(cid) = obj.extract::<PyClbitId>() {
        Ok(cid.into())
    } else if let Ok(index) = obj.extract::<u32>() {
        Ok(arvak_ir::ClbitId(index))
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err(
            "Expected ClbitId or int",
        ))
    }
}

/// A quantum circuit.
///
/// This class provides a high-level API for building quantum circuits
/// with a fluent interface that supports method chaining.
///
/// Example:
///     >>> qc = Circuit("bell", num_qubits=2)
///     >>> qc.h(0).cx(0, 1).measure_all()
///     >>> print(qc.depth())
///     2
#[pyclass(name = "Circuit", from_py_object)]
pub struct PyCircuit {
    pub(crate) inner: arvak_ir::Circuit,
}

#[pymethods]
impl PyCircuit {
    /// Create a new quantum circuit.
    ///
    /// Args:
    ///     name: The name of the circuit.
    ///     num_qubits: Initial number of qubits (default: 0).
    ///     num_clbits: Initial number of classical bits (default: 0).
    ///
    /// Returns:
    ///     A new Circuit instance.
    #[new]
    #[pyo3(signature = (name, num_qubits=0, num_clbits=0))]
    fn new(name: &str, num_qubits: u32, num_clbits: u32) -> Self {
        Self {
            inner: arvak_ir::Circuit::with_size(name, num_qubits, num_clbits),
        }
    }

    /// Get the name of the circuit.
    #[getter]
    fn name(&self) -> &str {
        self.inner.name()
    }

    /// Get the number of qubits in the circuit.
    #[getter]
    fn num_qubits(&self) -> usize {
        self.inner.num_qubits()
    }

    /// Get the number of classical bits in the circuit.
    #[getter]
    fn num_clbits(&self) -> usize {
        self.inner.num_clbits()
    }

    /// Get the depth of the circuit.
    fn depth(&self) -> usize {
        self.inner.depth()
    }

    /// Get the number of operations (gates + measurements) in the circuit.
    fn size(&self) -> usize {
        self.inner.dag().num_ops()
    }

    /// Add a qubit to the circuit.
    ///
    /// Returns:
    ///     The QubitId of the new qubit.
    fn add_qubit(&mut self) -> PyQubitId {
        self.inner.add_qubit().into()
    }

    /// Add a classical bit to the circuit.
    ///
    /// Returns:
    ///     The ClbitId of the new classical bit.
    fn add_clbit(&mut self) -> PyClbitId {
        self.inner.add_clbit().into()
    }

    /// Add a quantum register with multiple qubits.
    ///
    /// Args:
    ///     name: The name of the register.
    ///     size: The number of qubits in the register.
    ///
    /// Returns:
    ///     A list of QubitIds for the new qubits.
    fn add_qreg(&mut self, name: &str, size: u32) -> Vec<PyQubitId> {
        self.inner
            .add_qreg(name, size)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Add a classical register with multiple bits.
    ///
    /// Args:
    ///     name: The name of the register.
    ///     size: The number of classical bits in the register.
    ///
    /// Returns:
    ///     A list of ClbitIds for the new classical bits.
    fn add_creg(&mut self, name: &str, size: u32) -> Vec<PyClbitId> {
        self.inner
            .add_creg(name, size)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    // =========================================================================
    // Single-qubit gates (fluent API using Py<Self>)
    // =========================================================================

    /// Apply a Hadamard gate.
    ///
    /// Args:
    ///     qubit: The qubit to apply the gate to (QubitId or int).
    ///
    /// Returns:
    ///     self for method chaining.
    fn h(slf: Py<Self>, py: Python<'_>, qubit: &Bound<'_, PyAny>) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py).inner.h(qid).map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a Pauli-X gate.
    fn x(slf: Py<Self>, py: Python<'_>, qubit: &Bound<'_, PyAny>) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py).inner.x(qid).map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a Pauli-Y gate.
    fn y(slf: Py<Self>, py: Python<'_>, qubit: &Bound<'_, PyAny>) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py).inner.y(qid).map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a Pauli-Z gate.
    fn z(slf: Py<Self>, py: Python<'_>, qubit: &Bound<'_, PyAny>) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py).inner.z(qid).map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply an S gate (sqrt(Z)).
    fn s(slf: Py<Self>, py: Python<'_>, qubit: &Bound<'_, PyAny>) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py).inner.s(qid).map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply an S-dagger gate.
    fn sdg(slf: Py<Self>, py: Python<'_>, qubit: &Bound<'_, PyAny>) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py).inner.sdg(qid).map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a T gate (fourth root of Z).
    fn t(slf: Py<Self>, py: Python<'_>, qubit: &Bound<'_, PyAny>) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py).inner.t(qid).map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a T-dagger gate.
    fn tdg(slf: Py<Self>, py: Python<'_>, qubit: &Bound<'_, PyAny>) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py).inner.tdg(qid).map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a sqrt(X) gate.
    fn sx(slf: Py<Self>, py: Python<'_>, qubit: &Bound<'_, PyAny>) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py).inner.sx(qid).map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply an Rx rotation gate.
    ///
    /// Args:
    ///     theta: The rotation angle in radians.
    ///     qubit: The qubit to apply the gate to.
    fn rx(
        slf: Py<Self>,
        py: Python<'_>,
        theta: f64,
        qubit: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py)
            .inner
            .rx(theta, qid)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply an Ry rotation gate.
    ///
    /// Args:
    ///     theta: The rotation angle in radians.
    ///     qubit: The qubit to apply the gate to.
    fn ry(
        slf: Py<Self>,
        py: Python<'_>,
        theta: f64,
        qubit: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py)
            .inner
            .ry(theta, qid)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply an Rz rotation gate.
    ///
    /// Args:
    ///     theta: The rotation angle in radians.
    ///     qubit: The qubit to apply the gate to.
    fn rz(
        slf: Py<Self>,
        py: Python<'_>,
        theta: f64,
        qubit: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py)
            .inner
            .rz(theta, qid)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a phase gate.
    ///
    /// Args:
    ///     theta: The phase angle in radians.
    ///     qubit: The qubit to apply the gate to.
    fn p(
        slf: Py<Self>,
        py: Python<'_>,
        theta: f64,
        qubit: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py)
            .inner
            .p(theta, qid)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a universal U gate.
    ///
    /// Args:
    ///     theta: The first rotation angle.
    ///     phi: The second rotation angle.
    ///     lam: The third rotation angle (lambda).
    ///     qubit: The qubit to apply the gate to.
    #[pyo3(name = "u")]
    fn u_gate(
        slf: Py<Self>,
        py: Python<'_>,
        theta: f64,
        phi: f64,
        lam: f64,
        qubit: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py)
            .inner
            .u(theta, phi, lam, qid)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    // =========================================================================
    // Two-qubit gates
    // =========================================================================

    /// Apply a CNOT (CX) gate.
    ///
    /// Args:
    ///     control: The control qubit.
    ///     target: The target qubit.
    fn cx(
        slf: Py<Self>,
        py: Python<'_>,
        control: &Bound<'_, PyAny>,
        target: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let ctrl = to_qubit_id(control)?;
        let tgt = to_qubit_id(target)?;
        slf.borrow_mut(py)
            .inner
            .cx(ctrl, tgt)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a CY gate.
    fn cy(
        slf: Py<Self>,
        py: Python<'_>,
        control: &Bound<'_, PyAny>,
        target: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let ctrl = to_qubit_id(control)?;
        let tgt = to_qubit_id(target)?;
        slf.borrow_mut(py)
            .inner
            .cy(ctrl, tgt)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a CZ gate.
    fn cz(
        slf: Py<Self>,
        py: Python<'_>,
        control: &Bound<'_, PyAny>,
        target: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let ctrl = to_qubit_id(control)?;
        let tgt = to_qubit_id(target)?;
        slf.borrow_mut(py)
            .inner
            .cz(ctrl, tgt)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a SWAP gate.
    fn swap(
        slf: Py<Self>,
        py: Python<'_>,
        q1: &Bound<'_, PyAny>,
        q2: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let q1_id = to_qubit_id(q1)?;
        let q2_id = to_qubit_id(q2)?;
        slf.borrow_mut(py)
            .inner
            .swap(q1_id, q2_id)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply an iSWAP gate.
    fn iswap(
        slf: Py<Self>,
        py: Python<'_>,
        q1: &Bound<'_, PyAny>,
        q2: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let q1_id = to_qubit_id(q1)?;
        let q2_id = to_qubit_id(q2)?;
        slf.borrow_mut(py)
            .inner
            .iswap(q1_id, q2_id)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a controlled-Rz gate.
    fn crz(
        slf: Py<Self>,
        py: Python<'_>,
        theta: f64,
        control: &Bound<'_, PyAny>,
        target: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let ctrl = to_qubit_id(control)?;
        let tgt = to_qubit_id(target)?;
        slf.borrow_mut(py)
            .inner
            .crz(theta, ctrl, tgt)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a controlled-phase gate.
    fn cp(
        slf: Py<Self>,
        py: Python<'_>,
        theta: f64,
        control: &Bound<'_, PyAny>,
        target: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let ctrl = to_qubit_id(control)?;
        let tgt = to_qubit_id(target)?;
        slf.borrow_mut(py)
            .inner
            .cp(theta, ctrl, tgt)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    // =========================================================================
    // IQM native gates
    // =========================================================================

    /// Apply a phased RX gate (IQM native).
    ///
    /// PRX(θ, φ) = RZ(φ) · RX(θ) · RZ(-φ)
    fn prx(
        slf: Py<Self>,
        py: Python<'_>,
        theta: f64,
        phi: f64,
        qubit: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py)
            .inner
            .prx(theta, phi, qid)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    // =========================================================================
    // Three-qubit gates
    // =========================================================================

    /// Apply a Toffoli (CCX) gate.
    fn ccx(
        slf: Py<Self>,
        py: Python<'_>,
        c1: &Bound<'_, PyAny>,
        c2: &Bound<'_, PyAny>,
        target: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let c1_id = to_qubit_id(c1)?;
        let c2_id = to_qubit_id(c2)?;
        let tgt = to_qubit_id(target)?;
        slf.borrow_mut(py)
            .inner
            .ccx(c1_id, c2_id, tgt)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a Fredkin (CSWAP) gate.
    fn cswap(
        slf: Py<Self>,
        py: Python<'_>,
        control: &Bound<'_, PyAny>,
        t1: &Bound<'_, PyAny>,
        t2: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let ctrl = to_qubit_id(control)?;
        let t1_id = to_qubit_id(t1)?;
        let t2_id = to_qubit_id(t2)?;
        slf.borrow_mut(py)
            .inner
            .cswap(ctrl, t1_id, t2_id)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    // =========================================================================
    // Other operations
    // =========================================================================

    /// Measure a qubit to a classical bit.
    ///
    /// Args:
    ///     qubit: The qubit to measure.
    ///     clbit: The classical bit to store the result.
    fn measure(
        slf: Py<Self>,
        py: Python<'_>,
        qubit: &Bound<'_, PyAny>,
        clbit: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        let cid = to_clbit_id(clbit)?;
        slf.borrow_mut(py)
            .inner
            .measure(qid, cid)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Measure all qubits to corresponding classical bits.
    ///
    /// If there are not enough classical bits, they will be added.
    fn measure_all(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<Self>> {
        slf.borrow_mut(py)
            .inner
            .measure_all()
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Reset a qubit to |0⟩.
    fn reset(slf: Py<Self>, py: Python<'_>, qubit: &Bound<'_, PyAny>) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py).inner.reset(qid).map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a barrier to all qubits.
    fn barrier_all(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<Self>> {
        slf.borrow_mut(py)
            .inner
            .barrier_all()
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    /// Apply a delay to a qubit.
    fn delay(
        slf: Py<Self>,
        py: Python<'_>,
        qubit: &Bound<'_, PyAny>,
        duration: u64,
    ) -> PyResult<Py<Self>> {
        let qid = to_qubit_id(qubit)?;
        slf.borrow_mut(py)
            .inner
            .delay(qid, duration)
            .map_err(ir_to_py_err)?;
        Ok(slf)
    }

    // =========================================================================
    // Pre-built circuits
    // =========================================================================

    /// Create a Bell state circuit.
    ///
    /// Returns:
    ///     A 2-qubit circuit that creates a Bell state.
    #[staticmethod]
    fn bell() -> PyResult<Self> {
        let circuit = arvak_ir::Circuit::bell().map_err(ir_to_py_err)?;
        Ok(Self { inner: circuit })
    }

    /// Create a GHZ state circuit.
    ///
    /// Args:
    ///     n: The number of qubits.
    ///
    /// Returns:
    ///     An n-qubit circuit that creates a GHZ state.
    #[staticmethod]
    fn ghz(n: u32) -> PyResult<Self> {
        let circuit = arvak_ir::Circuit::ghz(n).map_err(ir_to_py_err)?;
        Ok(Self { inner: circuit })
    }

    /// Create a QFT circuit (without measurements).
    ///
    /// Args:
    ///     n: The number of qubits.
    ///
    /// Returns:
    ///     An n-qubit QFT circuit.
    #[staticmethod]
    fn qft(n: u32) -> PyResult<Self> {
        let circuit = arvak_ir::Circuit::qft(n).map_err(ir_to_py_err)?;
        Ok(Self { inner: circuit })
    }

    fn __repr__(&self) -> String {
        format!(
            "Circuit('{}', num_qubits={}, num_clbits={}, depth={})",
            self.inner.name(),
            self.inner.num_qubits(),
            self.inner.num_clbits(),
            self.inner.depth()
        )
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}

impl Clone for PyCircuit {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
