//! Python bindings for arvak-sim: Hamiltonian time-evolution circuit synthesis.
//!
//! Exposes the Trotter-Suzuki and QDrift product-formula synthesisers to Python.
//!
//! # Example
//!
//! ```python
//! from arvak.sim import Hamiltonian, HamiltonianTerm, TrotterEvolution
//!
//! h = Hamiltonian.from_terms([
//!     HamiltonianTerm.zz(0, 1, -1.0),
//!     HamiltonianTerm.x(0, -0.5),
//! ])
//! circuit = TrotterEvolution(h, 1.0, 4).first_order()
//! ```

use pyo3::prelude::*;
use rand::{SeedableRng, rngs::StdRng};

use crate::circuit::PyCircuit;

// ---------------------------------------------------------------------------
// PauliOp
// ---------------------------------------------------------------------------

/// Single-qubit Pauli operator: I, X, Y, or Z.
#[pyclass(name = "PauliOp", eq, hash, frozen, from_py_object)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PyPauliOp {
    inner: arvak_sim::PauliOp,
}

#[pymethods]
impl PyPauliOp {
    /// Identity operator.
    #[classattr]
    #[allow(non_snake_case)]
    fn I() -> Self {
        Self {
            inner: arvak_sim::PauliOp::I,
        }
    }

    /// Pauli-X operator.
    #[classattr]
    #[allow(non_snake_case)]
    fn X() -> Self {
        Self {
            inner: arvak_sim::PauliOp::X,
        }
    }

    /// Pauli-Y operator.
    #[classattr]
    #[allow(non_snake_case)]
    fn Y() -> Self {
        Self {
            inner: arvak_sim::PauliOp::Y,
        }
    }

    /// Pauli-Z operator.
    #[classattr]
    #[allow(non_snake_case)]
    fn Z() -> Self {
        Self {
            inner: arvak_sim::PauliOp::Z,
        }
    }

    fn __repr__(&self) -> &'static str {
        match self.inner {
            arvak_sim::PauliOp::I => "PauliOp.I",
            arvak_sim::PauliOp::X => "PauliOp.X",
            arvak_sim::PauliOp::Y => "PauliOp.Y",
            arvak_sim::PauliOp::Z => "PauliOp.Z",
        }
    }
}

// ---------------------------------------------------------------------------
// PauliString
// ---------------------------------------------------------------------------

/// A tensor product of Pauli operators on specific qubits.
///
/// Identity operators are omitted; qubits not listed are implicitly I.
#[pyclass(name = "PauliString", skip_from_py_object)]
#[derive(Clone)]
pub struct PyPauliString {
    inner: arvak_sim::PauliString,
}

#[pymethods]
impl PyPauliString {
    /// Construct a PauliString from a list of (qubit_index, PauliOp) pairs.
    ///
    /// Args:
    ///     ops: List of (int, PauliOp) pairs. Identity ops are dropped.
    ///
    /// Example:
    ///     >>> ps = PauliString.from_ops([(0, PauliOp.Z), (1, PauliOp.Z)])
    #[staticmethod]
    fn from_ops(ops: Vec<(u32, PyPauliOp)>) -> Self {
        let inner = arvak_sim::PauliString::from_ops(ops.into_iter().map(|(q, p)| (q, p.inner)));
        Self { inner }
    }

    /// Number of non-identity operators.
    fn __len__(&self) -> usize {
        self.inner.ops().len()
    }

    fn __repr__(&self) -> String {
        let ops: Vec<String> = self
            .inner
            .ops()
            .iter()
            .map(|(q, op)| {
                let op_str = match op {
                    arvak_sim::PauliOp::I => "I",
                    arvak_sim::PauliOp::X => "X",
                    arvak_sim::PauliOp::Y => "Y",
                    arvak_sim::PauliOp::Z => "Z",
                };
                format!("({q}, PauliOp.{op_str})")
            })
            .collect();
        format!("PauliString([{}])", ops.join(", "))
    }
}

// ---------------------------------------------------------------------------
// HamiltonianTerm
// ---------------------------------------------------------------------------

/// A single weighted Pauli term: coeff · pauli_string.
#[pyclass(name = "HamiltonianTerm", from_py_object)]
#[derive(Clone)]
pub struct PyHamiltonianTerm {
    inner: arvak_sim::HamiltonianTerm,
}

#[pymethods]
impl PyHamiltonianTerm {
    /// Create a new term from a coefficient and PauliString.
    ///
    /// Args:
    ///     coeff:  Real coefficient.
    ///     pauli:  The PauliString.
    #[new]
    fn new(coeff: f64, pauli: &PyPauliString) -> Self {
        Self {
            inner: arvak_sim::HamiltonianTerm::new(coeff, pauli.inner.clone()),
        }
    }

    /// Shorthand: single-qubit Z term (coeff · Z_qubit).
    #[staticmethod]
    fn z(qubit: u32, coeff: f64) -> Self {
        Self {
            inner: arvak_sim::HamiltonianTerm::z(qubit, coeff),
        }
    }

    /// Shorthand: two-qubit ZZ coupling (coeff · Z_q0 Z_q1).
    #[staticmethod]
    fn zz(q0: u32, q1: u32, coeff: f64) -> Self {
        Self {
            inner: arvak_sim::HamiltonianTerm::zz(q0, q1, coeff),
        }
    }

    /// Shorthand: single-qubit X term (coeff · X_qubit).
    #[staticmethod]
    fn x(qubit: u32, coeff: f64) -> Self {
        Self {
            inner: arvak_sim::HamiltonianTerm::x(qubit, coeff),
        }
    }

    /// The coefficient of this term.
    #[getter]
    fn coeff(&self) -> f64 {
        self.inner.coeff
    }

    fn __repr__(&self) -> String {
        format!("HamiltonianTerm(coeff={}, pauli=...)", self.inner.coeff)
    }
}

// ---------------------------------------------------------------------------
// Hamiltonian
// ---------------------------------------------------------------------------

/// A sum-of-Pauli-strings Hamiltonian: H = Σ_k c_k · P_k.
///
/// Example:
///     >>> from arvak.sim import Hamiltonian, HamiltonianTerm
///     >>> h = Hamiltonian.from_terms([
///     ...     HamiltonianTerm.zz(0, 1, -1.0),
///     ...     HamiltonianTerm.x(0, -0.5),
///     ... ])
///     >>> print(h.n_terms())
///     2
#[pyclass(name = "Hamiltonian", skip_from_py_object)]
#[derive(Clone)]
pub struct PyHamiltonian {
    inner: arvak_sim::Hamiltonian,
}

#[pymethods]
impl PyHamiltonian {
    /// Create from a list of HamiltonianTerm objects.
    #[staticmethod]
    fn from_terms(terms: Vec<PyHamiltonianTerm>) -> Self {
        let inner =
            arvak_sim::Hamiltonian::from_terms(terms.into_iter().map(|t| t.inner).collect());
        Self { inner }
    }

    /// Spectral norm upper bound: Σ |c_k|.  Used by QDrift for error bounds.
    #[pyo3(name = "lambda_")]
    fn lambda_(&self) -> f64 {
        self.inner.lambda()
    }

    /// Minimum number of qubits needed to represent this Hamiltonian.
    fn min_qubits(&self) -> u32 {
        self.inner.min_qubits()
    }

    /// Number of terms in the Hamiltonian.
    fn n_terms(&self) -> usize {
        self.inner.n_terms()
    }

    fn __repr__(&self) -> String {
        format!(
            "Hamiltonian(n_terms={}, min_qubits={}, lambda={:.4})",
            self.inner.n_terms(),
            self.inner.min_qubits(),
            self.inner.lambda(),
        )
    }
}

// ---------------------------------------------------------------------------
// TrotterEvolution
// ---------------------------------------------------------------------------

/// Trotter-Suzuki product-formula time-evolution synthesiser.
///
/// Approximates exp(-i H t) using first- or second-order Trotter formulas.
///
/// Args:
///     hamiltonian: The Hamiltonian H = Σ c_k P_k.
///     t:           Total evolution time.
///     n_steps:     Number of Trotter slices (higher → more accurate, deeper circuit).
///
/// Example:
///     >>> evol = TrotterEvolution(h, 1.0, 4)
///     >>> circuit = evol.first_order()
#[pyclass(name = "TrotterEvolution")]
pub struct PyTrotterEvolution {
    inner: arvak_sim::TrotterEvolution,
}

#[pymethods]
impl PyTrotterEvolution {
    #[new]
    fn new(hamiltonian: &PyHamiltonian, t: f64, n_steps: usize) -> Self {
        Self {
            inner: arvak_sim::TrotterEvolution::new(hamiltonian.inner.clone(), t, n_steps),
        }
    }

    /// Synthesise a first-order (Lie-Trotter) circuit.
    ///
    /// Error O(t²/n). Each Trotter step applies every Hamiltonian term once.
    ///
    /// Returns:
    ///     arvak.Circuit approximating exp(-iHt).
    ///
    /// Raises:
    ///     RuntimeError: If the Hamiltonian is empty or n_steps is 0.
    fn first_order(&self, py: Python<'_>) -> PyResult<Py<PyCircuit>> {
        let circuit = self.inner.first_order().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Trotter synthesis failed: {e}"))
        })?;
        Py::new(py, PyCircuit { inner: circuit })
    }

    /// Synthesise a second-order (Suzuki-Trotter) circuit.
    ///
    /// Error O(t³/n²). Each step uses a symmetric forward+backward sweep.
    ///
    /// Returns:
    ///     arvak.Circuit approximating exp(-iHt).
    ///
    /// Raises:
    ///     RuntimeError: If the Hamiltonian is empty or n_steps is 0.
    fn second_order(&self, py: Python<'_>) -> PyResult<Py<PyCircuit>> {
        let circuit = self.inner.second_order().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Trotter synthesis failed: {e}"))
        })?;
        Py::new(py, PyCircuit { inner: circuit })
    }

    fn __repr__(&self) -> String {
        "TrotterEvolution(...)".to_string()
    }
}

// ---------------------------------------------------------------------------
// QDriftEvolution
// ---------------------------------------------------------------------------

/// QDrift stochastic product-formula time-evolution synthesiser.
///
/// Approximates exp(-i H t) by randomly sampling Hamiltonian terms with
/// probability proportional to their coefficients (Campbell 2019).
///
/// Error (diamond norm): O(λ² t² / N).
///
/// Args:
///     hamiltonian: The Hamiltonian H = Σ c_k P_k.
///     t:           Total evolution time.
///     n_samples:   Number of random channel samples N (higher → more accurate).
///
/// Example:
///     >>> evol = QDriftEvolution(h, 1.0, 20)
///     >>> circuit = evol.circuit(seed=42)
#[pyclass(name = "QDriftEvolution")]
pub struct PyQDriftEvolution {
    inner: arvak_sim::QDriftEvolution,
}

#[pymethods]
impl PyQDriftEvolution {
    #[new]
    fn new(hamiltonian: &PyHamiltonian, t: f64, n_samples: usize) -> Self {
        Self {
            inner: arvak_sim::QDriftEvolution::new(hamiltonian.inner.clone(), t, n_samples),
        }
    }

    /// Synthesise a QDrift circuit.
    ///
    /// Args:
    ///     seed: Optional integer seed for reproducible circuits. If None,
    ///           the thread-local RNG is used (non-deterministic).
    ///
    /// Returns:
    ///     arvak.Circuit approximating exp(-iHt).
    ///
    /// Raises:
    ///     RuntimeError: If the Hamiltonian is empty or n_samples is 0.
    #[pyo3(signature = (seed=None))]
    fn circuit(&self, py: Python<'_>, seed: Option<u64>) -> PyResult<Py<PyCircuit>> {
        let result = if let Some(s) = seed {
            let rng = StdRng::seed_from_u64(s);
            self.inner.circuit_with_rng(rng)
        } else {
            self.inner.circuit()
        };

        let circuit = result.map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("QDrift synthesis failed: {e}"))
        })?;
        Py::new(py, PyCircuit { inner: circuit })
    }

    fn __repr__(&self) -> String {
        "QDriftEvolution(...)".to_string()
    }
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

/// Register the `arvak.sim` submodule.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "sim")?;

    m.add_class::<PyPauliOp>()?;
    m.add_class::<PyPauliString>()?;
    m.add_class::<PyHamiltonianTerm>()?;
    m.add_class::<PyHamiltonian>()?;
    m.add_class::<PyTrotterEvolution>()?;
    m.add_class::<PyQDriftEvolution>()?;

    parent.add_submodule(&m)?;
    Ok(())
}
