//! Python wrappers for qubit and classical bit types.

use pyo3::prelude::*;

/// Unique identifier for a qubit within a circuit.
///
/// QubitId is a lightweight wrapper around a u32 index that identifies
/// a specific qubit in a quantum circuit.
#[pyclass(name = "QubitId", from_py_object)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PyQubitId(pub u32);

#[pymethods]
impl PyQubitId {
    /// Create a new qubit identifier.
    ///
    /// Args:
    ///     index: The integer index of the qubit (0-based).
    ///
    /// Returns:
    ///     A new QubitId instance.
    #[new]
    fn new(index: u32) -> Self {
        PyQubitId(index)
    }

    /// Get the integer index of this qubit.
    #[getter]
    fn index(&self) -> u32 {
        self.0
    }

    fn __repr__(&self) -> String {
        format!("QubitId({})", self.0)
    }

    fn __str__(&self) -> String {
        format!("q{}", self.0)
    }

    fn __hash__(&self) -> u64 {
        self.0 as u64
    }

    fn __eq__(&self, other: &PyQubitId) -> bool {
        self.0 == other.0
    }

    fn __int__(&self) -> u32 {
        self.0
    }
}

impl From<PyQubitId> for arvak_ir::QubitId {
    fn from(q: PyQubitId) -> Self {
        arvak_ir::QubitId(q.0)
    }
}

impl From<arvak_ir::QubitId> for PyQubitId {
    fn from(q: arvak_ir::QubitId) -> Self {
        PyQubitId(q.0)
    }
}

/// Unique identifier for a classical bit within a circuit.
///
/// ClbitId is a lightweight wrapper around a u32 index that identifies
/// a specific classical bit in a quantum circuit.
#[pyclass(name = "ClbitId", from_py_object)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PyClbitId(pub u32);

#[pymethods]
impl PyClbitId {
    /// Create a new classical bit identifier.
    ///
    /// Args:
    ///     index: The integer index of the classical bit (0-based).
    ///
    /// Returns:
    ///     A new ClbitId instance.
    #[new]
    fn new(index: u32) -> Self {
        PyClbitId(index)
    }

    /// Get the integer index of this classical bit.
    #[getter]
    fn index(&self) -> u32 {
        self.0
    }

    fn __repr__(&self) -> String {
        format!("ClbitId({})", self.0)
    }

    fn __str__(&self) -> String {
        format!("c{}", self.0)
    }

    fn __hash__(&self) -> u64 {
        self.0 as u64
    }

    fn __eq__(&self, other: &PyClbitId) -> bool {
        self.0 == other.0
    }

    fn __int__(&self) -> u32 {
        self.0
    }
}

impl From<PyClbitId> for arvak_ir::ClbitId {
    fn from(c: PyClbitId) -> Self {
        arvak_ir::ClbitId(c.0)
    }
}

impl From<arvak_ir::ClbitId> for PyClbitId {
    fn from(c: arvak_ir::ClbitId) -> Self {
        PyClbitId(c.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qubit_id_conversion() {
        let py_id = PyQubitId(5);
        let rust_id: arvak_ir::QubitId = py_id.into();
        assert_eq!(rust_id.0, 5);

        let back: PyQubitId = rust_id.into();
        assert_eq!(back.0, 5);
    }

    #[test]
    fn test_clbit_id_conversion() {
        let py_id = PyClbitId(3);
        let rust_id: arvak_ir::ClbitId = py_id.into();
        assert_eq!(rust_id.0, 3);

        let back: PyClbitId = rust_id.into();
        assert_eq!(back.0, 3);
    }
}
