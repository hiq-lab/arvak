//! Noise channel types for the Arvak IR.
//!
//! This module introduces noise as a first-class concept in the circuit IR,
//! distinguishing between noise-as-deficit (to be mitigated) and
//! noise-as-resource (to be preserved). This distinction enables quantum
//! communication protocols where channel noise serves as a security
//! resource — e.g., QKD eavesdropping detection via noise fingerprinting.
//!
//! # Semantic roles
//!
//! The [`NoiseRole`] enum is the load-bearing innovation:
//!
//! - **Deficit**: Noise the compiler may freely optimize around. Informational,
//!   like Qiskit's noise models. Injected automatically from hardware profiles.
//! - **Resource**: Noise the compiler **must** preserve. It is a protocol
//!   resource (e.g., the expected channel noise in a QKD protocol). Optimization
//!   passes must skip these nodes, treating them as untouchable.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A noise channel model.
///
/// Represents the physical noise process applied to qubits.
/// Kept deliberately lean — covers the common channels relevant to
/// ion-trap and superconducting hardware. The `Custom` variant
/// provides an escape hatch for backend-specific models.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NoiseModel {
    /// Depolarizing channel: with probability `p`, replaces the state
    /// with the maximally mixed state.
    Depolarizing {
        /// Error probability (0.0 to 1.0).
        p: f64,
    },

    /// Amplitude damping: models energy relaxation (T1 decay).
    AmplitudeDamping {
        /// Damping parameter (0.0 to 1.0).
        gamma: f64,
    },

    /// Phase damping: models dephasing (T2 decay without energy loss).
    PhaseDamping {
        /// Dephasing parameter (0.0 to 1.0).
        gamma: f64,
    },

    /// Bit-flip channel: flips |0⟩ ↔ |1⟩ with probability `p`.
    BitFlip {
        /// Flip probability (0.0 to 1.0).
        p: f64,
    },

    /// Phase-flip channel: applies Z with probability `p`.
    PhaseFlip {
        /// Flip probability (0.0 to 1.0).
        p: f64,
    },

    /// Readout error: measurement reports wrong outcome with probability `p`.
    ReadoutError {
        /// Misclassification probability (0.0 to 1.0).
        p: f64,
    },

    /// Custom noise model for backend-specific channels.
    Custom {
        /// Descriptive name (e.g., "crosstalk", "leakage").
        name: String,
        /// Key-value parameters.
        params: BTreeMap<String, f64>,
    },
}

impl NoiseModel {
    /// Get a human-readable name for this noise model.
    pub fn name(&self) -> &str {
        match self {
            NoiseModel::Depolarizing { .. } => "depolarizing",
            NoiseModel::AmplitudeDamping { .. } => "amplitude_damping",
            NoiseModel::PhaseDamping { .. } => "phase_damping",
            NoiseModel::BitFlip { .. } => "bit_flip",
            NoiseModel::PhaseFlip { .. } => "phase_flip",
            NoiseModel::ReadoutError { .. } => "readout_error",
            NoiseModel::Custom { name, .. } => name,
        }
    }

    /// Get the primary error parameter of this noise model.
    pub fn error_param(&self) -> f64 {
        match self {
            NoiseModel::Depolarizing { p } => *p,
            NoiseModel::AmplitudeDamping { gamma } => *gamma,
            NoiseModel::PhaseDamping { gamma } => *gamma,
            NoiseModel::BitFlip { p } => *p,
            NoiseModel::PhaseFlip { p } => *p,
            NoiseModel::ReadoutError { p } => *p,
            NoiseModel::Custom { params, .. } => params.values().next().copied().unwrap_or(0.0),
        }
    }
}

impl std::fmt::Display for NoiseModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoiseModel::Depolarizing { p } => write!(f, "depolarizing(p={:.4})", p),
            NoiseModel::AmplitudeDamping { gamma } => {
                write!(f, "amplitude_damping(γ={:.4})", gamma)
            }
            NoiseModel::PhaseDamping { gamma } => write!(f, "phase_damping(γ={:.4})", gamma),
            NoiseModel::BitFlip { p } => write!(f, "bit_flip(p={:.4})", p),
            NoiseModel::PhaseFlip { p } => write!(f, "phase_flip(p={:.4})", p),
            NoiseModel::ReadoutError { p } => write!(f, "readout_error(p={:.4})", p),
            NoiseModel::Custom { name, .. } => write!(f, "custom({})", name),
        }
    }
}

/// Semantic role of a noise channel in the circuit.
///
/// This is the key distinction that separates Arvak from every other
/// quantum compiler. Traditional compilers treat all noise as deficit.
/// Arvak recognizes that some noise is a protocol resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NoiseRole {
    /// Noise to mitigate. The compiler may reorder, merge, or optimize
    /// around Deficit channels. Typically injected from hardware profiles.
    Deficit,

    /// Noise as protocol resource. The compiler **must** preserve these
    /// channels — they carry semantic meaning (e.g., expected channel
    /// noise in a QKD protocol, noise fingerprint for intrusion detection).
    Resource,
}

impl std::fmt::Display for NoiseRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoiseRole::Deficit => write!(f, "deficit"),
            NoiseRole::Resource => write!(f, "resource"),
        }
    }
}

/// Hardware noise profile reported by a backend.
///
/// Lives in arvak-ir (not arvak-hal) so that both the HAL and compiler
/// can use it without circular dependencies.
///
/// The `NoiseInjectionPass` reads this profile and inserts `Deficit`
/// noise channels into the DAG at appropriate locations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NoiseProfile {
    /// Per-gate error rates, keyed by gate name (e.g., "cx" → 0.01).
    #[serde(default)]
    pub gate_errors: BTreeMap<String, f64>,

    /// T1 relaxation time per qubit in microseconds.
    #[serde(default)]
    pub t1: Option<Vec<f64>>,

    /// T2 dephasing time per qubit in microseconds.
    #[serde(default)]
    pub t2: Option<Vec<f64>>,

    /// Readout error probability per qubit.
    #[serde(default)]
    pub readout_errors: Option<Vec<f64>>,

    /// Opaque backend-specific noise fingerprint.
    ///
    /// Deliberately untyped — ion traps, superconducting qubits, and
    /// neutral atoms all have different characteristic noise signatures.
    #[serde(default)]
    pub fingerprint: Option<serde_json::Value>,
}

impl NoiseProfile {
    /// Create a new empty noise profile.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the error rate for a specific gate, if known.
    pub fn gate_error(&self, gate_name: &str) -> Option<f64> {
        self.gate_errors.get(gate_name).copied()
    }

    /// Get the T1 time for a specific qubit, if known.
    pub fn qubit_t1(&self, qubit_index: usize) -> Option<f64> {
        self.t1.as_ref().and_then(|v| v.get(qubit_index)).copied()
    }

    /// Get the T2 time for a specific qubit, if known.
    pub fn qubit_t2(&self, qubit_index: usize) -> Option<f64> {
        self.t2.as_ref().and_then(|v| v.get(qubit_index)).copied()
    }

    /// Get the readout error for a specific qubit, if known.
    pub fn qubit_readout_error(&self, qubit_index: usize) -> Option<f64> {
        self.readout_errors
            .as_ref()
            .and_then(|v| v.get(qubit_index))
            .copied()
    }

    /// Check if this profile has any noise data at all.
    pub fn is_empty(&self) -> bool {
        self.gate_errors.is_empty()
            && self.t1.is_none()
            && self.t2.is_none()
            && self.readout_errors.is_none()
            && self.fingerprint.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noise_model_names() {
        assert_eq!(NoiseModel::Depolarizing { p: 0.01 }.name(), "depolarizing");
        assert_eq!(
            NoiseModel::AmplitudeDamping { gamma: 0.02 }.name(),
            "amplitude_damping"
        );
        assert_eq!(NoiseModel::ReadoutError { p: 0.05 }.name(), "readout_error");
    }

    #[test]
    fn test_noise_model_display() {
        let m = NoiseModel::Depolarizing { p: 0.03 };
        assert_eq!(format!("{}", m), "depolarizing(p=0.0300)");
    }

    #[test]
    fn test_noise_role_display() {
        assert_eq!(format!("{}", NoiseRole::Deficit), "deficit");
        assert_eq!(format!("{}", NoiseRole::Resource), "resource");
    }

    #[test]
    fn test_noise_profile_empty() {
        let profile = NoiseProfile::new();
        assert!(profile.is_empty());
        assert_eq!(profile.gate_error("cx"), None);
        assert_eq!(profile.qubit_t1(0), None);
    }

    #[test]
    fn test_noise_profile_with_data() {
        let mut profile = NoiseProfile::new();
        profile.gate_errors.insert("cx".into(), 0.01);
        profile.gate_errors.insert("h".into(), 0.001);
        profile.t1 = Some(vec![50.0, 45.0, 55.0]);
        profile.t2 = Some(vec![30.0, 25.0, 35.0]);
        profile.readout_errors = Some(vec![0.02, 0.03, 0.015]);

        assert!(!profile.is_empty());
        assert_eq!(profile.gate_error("cx"), Some(0.01));
        assert_eq!(profile.gate_error("cz"), None);
        assert_eq!(profile.qubit_t1(1), Some(45.0));
        assert_eq!(profile.qubit_t2(2), Some(35.0));
        assert_eq!(profile.qubit_readout_error(0), Some(0.02));
        assert_eq!(profile.qubit_t1(99), None);
    }

    #[test]
    fn test_noise_profile_serialization() {
        let mut profile = NoiseProfile::new();
        profile.gate_errors.insert("cx".into(), 0.01);
        profile.t1 = Some(vec![50.0]);

        let json = serde_json::to_string(&profile).unwrap();
        let deserialized: NoiseProfile = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.gate_error("cx"), Some(0.01));
        assert_eq!(deserialized.qubit_t1(0), Some(50.0));
    }
}
