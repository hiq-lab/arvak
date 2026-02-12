//! Python wrappers for compilation types.

use pyo3::prelude::*;

use crate::qubits::PyQubitId;

/// A mapping from logical qubits to physical qubits.
///
/// The layout defines how logical qubits in a circuit map to physical
/// qubits on the target hardware.
#[pyclass(name = "Layout", from_py_object)]
#[derive(Clone)]
pub struct PyLayout {
    pub(crate) inner: arvak_compile::Layout,
}

#[pymethods]
impl PyLayout {
    /// Create a new empty layout.
    #[new]
    fn new() -> Self {
        Self {
            inner: arvak_compile::Layout::new(),
        }
    }

    /// Create a trivial layout (logical qubit i maps to physical qubit i).
    ///
    /// Args:
    ///     num_qubits: The number of qubits.
    ///
    /// Returns:
    ///     A Layout with trivial mapping.
    #[staticmethod]
    fn trivial(num_qubits: u32) -> Self {
        Self {
            inner: arvak_compile::Layout::trivial(num_qubits),
        }
    }

    /// Add a mapping from logical to physical qubit.
    ///
    /// Args:
    ///     logical: The logical qubit (QubitId or int).
    ///     physical: The physical qubit index.
    fn add(&mut self, logical: u32, physical: u32) {
        self.inner.add(arvak_ir::QubitId(logical), physical);
    }

    /// Get the physical qubit for a logical qubit.
    ///
    /// Args:
    ///     logical: The logical qubit index.
    ///
    /// Returns:
    ///     The physical qubit index, or None if not mapped.
    fn get_physical(&self, logical: u32) -> Option<u32> {
        self.inner.get_physical(arvak_ir::QubitId(logical))
    }

    /// Get the logical qubit for a physical qubit.
    ///
    /// Args:
    ///     physical: The physical qubit index.
    ///
    /// Returns:
    ///     The logical QubitId, or None if not mapped.
    fn get_logical(&self, physical: u32) -> Option<PyQubitId> {
        self.inner.get_logical(physical).map(|q| PyQubitId(q.0))
    }

    /// Swap two physical qubits in the layout.
    fn swap(&mut self, p1: u32, p2: u32) {
        self.inner.swap(p1, p2);
    }

    /// Get the number of mapped qubits.
    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __repr__(&self) -> String {
        let mappings: Vec<String> = self
            .inner
            .iter()
            .map(|(l, p)| format!("{}→{}", l.0, p))
            .collect();
        format!("Layout({{{}}})", mappings.join(", "))
    }
}

/// Target device coupling map.
///
/// The coupling map defines which pairs of physical qubits can
/// interact with two-qubit gates.
#[pyclass(name = "CouplingMap", from_py_object)]
#[derive(Clone)]
pub struct PyCouplingMap {
    pub(crate) inner: arvak_compile::CouplingMap,
}

#[pymethods]
impl PyCouplingMap {
    /// Create a new coupling map with the given number of qubits.
    #[new]
    fn new(num_qubits: u32) -> Self {
        Self {
            inner: arvak_compile::CouplingMap::new(num_qubits),
        }
    }

    /// Add an edge between two qubits (bidirectional).
    fn add_edge(&mut self, q1: u32, q2: u32) {
        self.inner.add_edge(q1, q2);
    }

    /// Check if two qubits are directly connected.
    fn is_connected(&self, q1: u32, q2: u32) -> bool {
        self.inner.is_connected(q1, q2)
    }

    /// Get the number of physical qubits.
    #[getter]
    fn num_qubits(&self) -> u32 {
        self.inner.num_qubits()
    }

    /// Get the coupling edges as a list of tuples.
    fn edges(&self) -> Vec<(u32, u32)> {
        self.inner.edges().to_vec()
    }

    /// Calculate shortest path distance between two qubits.
    ///
    /// Returns None if the qubits are not connected.
    fn distance(&self, from: u32, to: u32) -> Option<u32> {
        self.inner.distance(from, to)
    }

    /// Create a linear coupling map (0-1-2-3-...).
    #[staticmethod]
    fn linear(n: u32) -> Self {
        Self {
            inner: arvak_compile::CouplingMap::linear(n),
        }
    }

    /// Create a fully connected coupling map.
    #[staticmethod]
    fn full(n: u32) -> Self {
        Self {
            inner: arvak_compile::CouplingMap::full(n),
        }
    }

    /// Create a star topology (center qubit connected to all others).
    #[staticmethod]
    fn star(n: u32) -> Self {
        Self {
            inner: arvak_compile::CouplingMap::star(n),
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "CouplingMap(num_qubits={}, edges={})",
            self.inner.num_qubits(),
            self.inner.edges().len()
        )
    }
}

/// Basis gates for the target device.
///
/// The basis gates define the native gate set that the target
/// hardware supports.
#[pyclass(name = "BasisGates", from_py_object)]
#[derive(Clone)]
pub struct PyBasisGates {
    pub(crate) inner: arvak_compile::BasisGates,
}

#[pymethods]
impl PyBasisGates {
    /// Create a new basis gates set from a list of gate names.
    #[new]
    fn new(gates: Vec<String>) -> Self {
        Self {
            inner: arvak_compile::BasisGates::new(gates),
        }
    }

    /// Check if a gate is in the basis.
    fn contains(&self, gate: &str) -> bool {
        self.inner.contains(gate)
    }

    /// Get the basis gates as a list.
    fn gates(&self) -> Vec<String> {
        self.inner.gates().to_vec()
    }

    /// Create IQM basis gates (PRX + CZ).
    #[staticmethod]
    fn iqm() -> Self {
        Self {
            inner: arvak_compile::BasisGates::iqm(),
        }
    }

    /// Create IBM basis gates (RZ + SX + X + CX).
    #[staticmethod]
    fn ibm() -> Self {
        Self {
            inner: arvak_compile::BasisGates::ibm(),
        }
    }

    /// Create a universal basis (all standard gates).
    #[staticmethod]
    fn universal() -> Self {
        Self {
            inner: arvak_compile::BasisGates::universal(),
        }
    }

    fn __repr__(&self) -> String {
        format!("BasisGates({:?})", self.inner.gates())
    }
}

/// Properties shared between compilation passes.
///
/// The PropertySet allows passes to communicate by storing target
/// configuration and intermediate results.
#[pyclass(name = "PropertySet")]
pub struct PyPropertySet {
    pub(crate) inner: arvak_compile::PropertySet,
}

#[pymethods]
impl PyPropertySet {
    /// Create a new empty property set.
    #[new]
    fn new() -> Self {
        Self {
            inner: arvak_compile::PropertySet::new(),
        }
    }

    /// Set the layout.
    fn set_layout(&mut self, layout: PyLayout) {
        self.inner.layout = Some(layout.inner);
    }

    /// Get the layout.
    fn get_layout(&self) -> Option<PyLayout> {
        self.inner.layout.clone().map(|l| PyLayout { inner: l })
    }

    /// Set the coupling map.
    fn set_coupling_map(&mut self, coupling_map: PyCouplingMap) {
        self.inner.coupling_map = Some(coupling_map.inner);
    }

    /// Get the coupling map.
    fn get_coupling_map(&self) -> Option<PyCouplingMap> {
        self.inner
            .coupling_map
            .clone()
            .map(|c| PyCouplingMap { inner: c })
    }

    /// Set the basis gates.
    fn set_basis_gates(&mut self, basis_gates: PyBasisGates) {
        self.inner.basis_gates = Some(basis_gates.inner);
    }

    /// Get the basis gates.
    fn get_basis_gates(&self) -> Option<PyBasisGates> {
        self.inner
            .basis_gates
            .clone()
            .map(|b| PyBasisGates { inner: b })
    }

    /// Configure the property set with target hardware.
    ///
    /// Args:
    ///     coupling_map: The device coupling map.
    ///     basis_gates: The native gate set.
    ///
    /// Returns:
    ///     self for method chaining.
    fn with_target(
        mut slf: PyRefMut<'_, Self>,
        coupling_map: PyCouplingMap,
        basis_gates: PyBasisGates,
    ) -> PyRefMut<'_, Self> {
        slf.inner.coupling_map = Some(coupling_map.inner);
        slf.inner.basis_gates = Some(basis_gates.inner);
        slf
    }

    fn __repr__(&self) -> String {
        let has_layout = self.inner.layout.is_some();
        let has_coupling = self.inner.coupling_map.is_some();
        let has_basis = self.inner.basis_gates.is_some();
        format!(
            "PropertySet(layout={}, coupling_map={}, basis_gates={})",
            has_layout, has_coupling, has_basis
        )
    }
}
