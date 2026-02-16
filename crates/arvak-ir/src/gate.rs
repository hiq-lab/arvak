//! Quantum gate types.

use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::parameter::ParameterExpression;

/// Standard gates with known semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StandardGate {
    // Single-qubit Pauli gates
    /// Identity gate.
    I,
    /// Pauli-X gate.
    X,
    /// Pauli-Y gate.
    Y,
    /// Pauli-Z gate.
    Z,

    // Single-qubit Clifford gates
    /// Hadamard gate.
    H,
    /// S gate (sqrt(Z)).
    S,
    /// S-dagger gate.
    Sdg,
    /// T gate (fourth root of Z).
    T,
    /// T-dagger gate.
    Tdg,
    /// sqrt(X) gate.
    SX,
    /// sqrt(X)-dagger gate.
    SXdg,

    // Single-qubit rotation gates
    /// Rotation around X axis.
    Rx(ParameterExpression),
    /// Rotation around Y axis.
    Ry(ParameterExpression),
    /// Rotation around Z axis.
    Rz(ParameterExpression),
    /// Phase gate.
    P(ParameterExpression),
    /// Universal single-qubit gate U(θ, φ, λ).
    U(
        ParameterExpression,
        ParameterExpression,
        ParameterExpression,
    ),

    // Two-qubit gates
    /// Controlled-X (CNOT) gate.
    CX,
    /// Controlled-Y gate.
    CY,
    /// Controlled-Z gate.
    CZ,
    /// Controlled-Hadamard gate.
    CH,
    /// SWAP gate.
    Swap,
    /// iSWAP gate.
    ISwap,
    /// Controlled rotation around X.
    CRx(ParameterExpression),
    /// Controlled rotation around Y.
    CRy(ParameterExpression),
    /// Controlled rotation around Z.
    CRz(ParameterExpression),
    /// Controlled phase gate.
    CP(ParameterExpression),
    /// XX rotation gate.
    RXX(ParameterExpression),
    /// YY rotation gate.
    RYY(ParameterExpression),
    /// ZZ rotation gate.
    RZZ(ParameterExpression),

    // Three-qubit gates
    /// Toffoli gate (CCX).
    CCX,
    /// Fredkin gate (CSWAP).
    CSwap,

    // IQM native gates
    /// Phased RX gate: PRX(θ, φ) = RZ(φ) · RX(θ) · RZ(-φ).
    PRX(ParameterExpression, ParameterExpression),
}

impl StandardGate {
    /// Get the name of this gate.
    #[inline]
    pub fn name(&self) -> &'static str {
        match self {
            StandardGate::I => "id",
            StandardGate::X => "x",
            StandardGate::Y => "y",
            StandardGate::Z => "z",
            StandardGate::H => "h",
            StandardGate::S => "s",
            StandardGate::Sdg => "sdg",
            StandardGate::T => "t",
            StandardGate::Tdg => "tdg",
            StandardGate::SX => "sx",
            StandardGate::SXdg => "sxdg",
            StandardGate::Rx(_) => "rx",
            StandardGate::Ry(_) => "ry",
            StandardGate::Rz(_) => "rz",
            StandardGate::P(_) => "p",
            StandardGate::U(_, _, _) => "u",
            StandardGate::CX => "cx",
            StandardGate::CY => "cy",
            StandardGate::CZ => "cz",
            StandardGate::CH => "ch",
            StandardGate::Swap => "swap",
            StandardGate::ISwap => "iswap",
            StandardGate::CRx(_) => "crx",
            StandardGate::CRy(_) => "cry",
            StandardGate::CRz(_) => "crz",
            StandardGate::CP(_) => "cp",
            StandardGate::RXX(_) => "rxx",
            StandardGate::RYY(_) => "ryy",
            StandardGate::RZZ(_) => "rzz",
            StandardGate::CCX => "ccx",
            StandardGate::CSwap => "cswap",
            StandardGate::PRX(_, _) => "prx",
        }
    }

    /// Get the number of qubits this gate operates on.
    #[inline]
    pub fn num_qubits(&self) -> u32 {
        match self {
            StandardGate::I
            | StandardGate::X
            | StandardGate::Y
            | StandardGate::Z
            | StandardGate::H
            | StandardGate::S
            | StandardGate::Sdg
            | StandardGate::T
            | StandardGate::Tdg
            | StandardGate::SX
            | StandardGate::SXdg
            | StandardGate::Rx(_)
            | StandardGate::Ry(_)
            | StandardGate::Rz(_)
            | StandardGate::P(_)
            | StandardGate::U(_, _, _)
            | StandardGate::PRX(_, _) => 1,

            StandardGate::CX
            | StandardGate::CY
            | StandardGate::CZ
            | StandardGate::CH
            | StandardGate::Swap
            | StandardGate::ISwap
            | StandardGate::CRx(_)
            | StandardGate::CRy(_)
            | StandardGate::CRz(_)
            | StandardGate::CP(_)
            | StandardGate::RXX(_)
            | StandardGate::RYY(_)
            | StandardGate::RZZ(_) => 2,

            StandardGate::CCX | StandardGate::CSwap => 3,
        }
    }

    /// Check if this gate has parameters.
    pub fn is_parameterized(&self) -> bool {
        match self {
            StandardGate::Rx(p)
            | StandardGate::Ry(p)
            | StandardGate::Rz(p)
            | StandardGate::P(p)
            | StandardGate::CRx(p)
            | StandardGate::CRy(p)
            | StandardGate::CRz(p)
            | StandardGate::CP(p)
            | StandardGate::RXX(p)
            | StandardGate::RYY(p)
            | StandardGate::RZZ(p) => p.is_symbolic(),

            StandardGate::U(a, b, c) => a.is_symbolic() || b.is_symbolic() || c.is_symbolic(),

            StandardGate::PRX(theta, phi) => theta.is_symbolic() || phi.is_symbolic(),

            _ => false,
        }
    }

    /// Get parameters of this gate.
    pub fn parameters(&self) -> Vec<&ParameterExpression> {
        match self {
            StandardGate::Rx(p)
            | StandardGate::Ry(p)
            | StandardGate::Rz(p)
            | StandardGate::P(p)
            | StandardGate::CRx(p)
            | StandardGate::CRy(p)
            | StandardGate::CRz(p)
            | StandardGate::CP(p)
            | StandardGate::RXX(p)
            | StandardGate::RYY(p)
            | StandardGate::RZZ(p) => vec![p],

            StandardGate::U(a, b, c) => vec![a, b, c],

            StandardGate::PRX(theta, phi) => vec![theta, phi],

            _ => vec![],
        }
    }
}

/// A quantum gate, either standard or custom.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GateKind {
    /// A standard gate with known semantics.
    Standard(StandardGate),
    /// A custom user-defined gate.
    Custom(CustomGate),
}

impl GateKind {
    /// Get the name of this gate.
    #[inline]
    pub fn name(&self) -> &str {
        match self {
            GateKind::Standard(g) => g.name(),
            GateKind::Custom(g) => &g.name,
        }
    }

    /// Get the number of qubits.
    #[inline]
    pub fn num_qubits(&self) -> u32 {
        match self {
            GateKind::Standard(g) => g.num_qubits(),
            GateKind::Custom(g) => g.num_qubits,
        }
    }
}

/// A user-defined or decomposed gate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomGate {
    /// The name of the gate.
    pub name: String,
    /// The number of qubits it operates on.
    pub num_qubits: u32,
    /// Parameters of the gate.
    pub params: Vec<ParameterExpression>,
    /// Optional unitary matrix (row-major, 2^n × 2^n).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matrix: Option<Vec<Complex64>>,
}

impl CustomGate {
    /// Create a new custom gate.
    pub fn new(name: impl Into<String>, num_qubits: u32) -> Self {
        Self {
            name: name.into(),
            num_qubits,
            params: vec![],
            matrix: None,
        }
    }

    /// Add parameters to the gate.
    #[must_use]
    pub fn with_params(mut self, params: Vec<ParameterExpression>) -> Self {
        self.params = params;
        self
    }

    /// Add a unitary matrix to the gate.
    ///
    /// # Panics
    ///
    /// Panics if `matrix.len()` does not equal `(2^num_qubits)^2`.
    #[must_use]
    pub fn with_matrix(mut self, matrix: Vec<Complex64>) -> Self {
        let dim = 1usize << self.num_qubits;
        assert_eq!(
            matrix.len(),
            dim * dim,
            "Matrix length {} does not match expected {} for {}-qubit gate",
            matrix.len(),
            dim * dim,
            self.num_qubits,
        );
        self.matrix = Some(matrix);
        self
    }
}

/// Classical condition for conditional gates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassicalCondition {
    /// The name of the classical register.
    pub register: String,
    /// The value to compare against.
    pub value: u64,
}

impl ClassicalCondition {
    /// Create a new classical condition.
    pub fn new(register: impl Into<String>, value: u64) -> Self {
        Self {
            register: register.into(),
            value,
        }
    }
}

/// A gate with associated metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gate {
    /// The kind of gate.
    pub kind: GateKind,
    /// Optional label for the gate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Optional classical condition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<ClassicalCondition>,
}

impl Gate {
    /// Create a new gate from a standard gate.
    pub fn standard(gate: StandardGate) -> Self {
        Self {
            kind: GateKind::Standard(gate),
            label: None,
            condition: None,
        }
    }

    /// Create a new gate from a custom gate.
    pub fn custom(gate: CustomGate) -> Self {
        Self {
            kind: GateKind::Custom(gate),
            label: None,
            condition: None,
        }
    }

    /// Add a label to the gate.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Add a classical condition to the gate.
    #[must_use]
    pub fn with_condition(mut self, condition: ClassicalCondition) -> Self {
        self.condition = Some(condition);
        self
    }

    /// Get the name of this gate.
    pub fn name(&self) -> &str {
        self.kind.name()
    }

    /// Get the number of qubits.
    pub fn num_qubits(&self) -> u32 {
        self.kind.num_qubits()
    }
}

impl From<StandardGate> for Gate {
    fn from(gate: StandardGate) -> Self {
        Gate::standard(gate)
    }
}

impl From<CustomGate> for Gate {
    fn from(gate: CustomGate) -> Self {
        Gate::custom(gate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn test_standard_gate_properties() {
        assert_eq!(StandardGate::H.num_qubits(), 1);
        assert_eq!(StandardGate::CX.num_qubits(), 2);
        assert_eq!(StandardGate::CCX.num_qubits(), 3);

        assert!(!StandardGate::H.is_parameterized());
        assert!(!StandardGate::Rx(ParameterExpression::constant(PI)).is_parameterized());
        assert!(StandardGate::Rx(ParameterExpression::symbol("theta")).is_parameterized());
    }

    #[test]
    fn test_gate_creation() {
        let h = Gate::standard(StandardGate::H);
        assert_eq!(h.name(), "h");
        assert_eq!(h.num_qubits(), 1);
        assert!(h.label.is_none());
        assert!(h.condition.is_none());

        let h_labeled = Gate::standard(StandardGate::H).with_label("my_hadamard");
        assert_eq!(h_labeled.label, Some("my_hadamard".to_string()));
    }

    #[test]
    fn test_custom_gate() {
        let custom = CustomGate::new("my_gate", 2)
            .with_params(vec![ParameterExpression::constant(PI / 4.0)]);

        assert_eq!(custom.name, "my_gate");
        assert_eq!(custom.num_qubits, 2);
        assert_eq!(custom.params.len(), 1);
    }
}
