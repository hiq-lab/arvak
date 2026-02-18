//! Device ARN to capabilities mapping.
//!
//! Maps known Braket device ARNs to Arvak `Capabilities` presets.
//! Falls back to dynamic construction from device info for unknown devices.

use arvak_hal::Capabilities;

// ──────────────────────────────────────────────────────────────────────
// Known device ARNs
// ──────────────────────────────────────────────────────────────────────

/// Rigetti Ankaa-3 (84 qubits, superconducting).
pub const RIGETTI_ANKAA_3: &str = "arn:aws:braket:us-west-1::device/qpu/rigetti/Ankaa-3";

/// IonQ Aria (25 qubits, trapped-ion).
pub const IONQ_ARIA: &str = "arn:aws:braket:us-east-1::device/qpu/ionq/Aria-1";

/// IonQ Aria 2 (25 qubits, trapped-ion).
pub const IONQ_ARIA_2: &str = "arn:aws:braket:us-east-1::device/qpu/ionq/Aria-2";

/// IonQ Forte (36 qubits, trapped-ion).
pub const IONQ_FORTE: &str = "arn:aws:braket:us-east-1::device/qpu/ionq/Forte-1";

/// IQM Garnet (20 qubits, superconducting).
pub const IQM_GARNET: &str = "arn:aws:braket:eu-north-1::device/qpu/iqm/Garnet";

/// SV1 state vector simulator.
pub const SV1: &str = "arn:aws:braket:::device/quantum-simulator/amazon/sv1";

/// TN1 tensor network simulator.
pub const TN1: &str = "arn:aws:braket:::device/quantum-simulator/amazon/tn1";

/// DM1 density matrix simulator.
pub const DM1: &str = "arn:aws:braket:::device/quantum-simulator/amazon/dm1";

// ──────────────────────────────────────────────────────────────────────
// Preset capabilities
// ──────────────────────────────────────────────────────────────────────

/// Get capabilities for a known Braket device ARN.
///
/// Returns `None` for unknown devices — caller should fall back to
/// dynamic discovery via the Braket API.
pub fn capabilities_for_device(device_arn: &str) -> Option<Capabilities> {
    match device_arn {
        RIGETTI_ANKAA_3 => Some(Capabilities::braket_rigetti("Rigetti Ankaa-3", 84)),
        IONQ_ARIA | IONQ_ARIA_2 => Some(Capabilities::braket_ionq("IonQ Aria", 25)),
        IONQ_FORTE => Some(Capabilities::braket_ionq("IonQ Forte", 36)),
        IQM_GARNET => Some(Capabilities::iqm("IQM Garnet", 20)),
        SV1 => Some(Capabilities::braket_simulator("Amazon SV1", 34)),
        TN1 => Some(Capabilities::braket_simulator("Amazon TN1", 50)),
        DM1 => Some(Capabilities::braket_simulator("Amazon DM1", 17)),
        _ => None,
    }
}

/// Map a friendly device name to its ARN.
pub fn arn_for_name(name: &str) -> Option<&'static str> {
    match name.to_lowercase().as_str() {
        "rigetti" | "ankaa" | "ankaa-3" | "ankaa3" => Some(RIGETTI_ANKAA_3),
        "ionq" | "aria" | "aria-1" => Some(IONQ_ARIA),
        "aria-2" => Some(IONQ_ARIA_2),
        "forte" | "forte-1" => Some(IONQ_FORTE),
        "iqm-garnet" => Some(IQM_GARNET),
        "sv1" | "braket-sv1" => Some(SV1),
        "tn1" | "braket-tn1" => Some(TN1),
        "dm1" | "braket-dm1" => Some(DM1),
        _ => None,
    }
}

/// Extract provider name from a device ARN.
pub fn provider_from_arn(device_arn: &str) -> &str {
    // ARN format: arn:aws:braket:<region>::device/<type>/<provider>/<device>
    device_arn.split('/').nth(2).unwrap_or("unknown")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_device_capabilities() {
        let caps = capabilities_for_device(RIGETTI_ANKAA_3).unwrap();
        assert_eq!(caps.num_qubits, 84);
        assert!(!caps.is_simulator);
        assert!(caps.gate_set.contains("rx"));
        assert!(caps.gate_set.contains("cz"));
    }

    #[test]
    fn test_ionq_capabilities() {
        let caps = capabilities_for_device(IONQ_ARIA).unwrap();
        assert_eq!(caps.num_qubits, 25);
        assert!(!caps.is_simulator);
        assert!(caps.gate_set.contains("rx"));
        assert!(caps.gate_set.contains("ry"));
        assert!(caps.gate_set.contains("xx"));
    }

    #[test]
    fn test_simulator_capabilities() {
        let caps = capabilities_for_device(SV1).unwrap();
        assert!(caps.is_simulator);
        assert_eq!(caps.num_qubits, 34);
    }

    #[test]
    fn test_unknown_device() {
        assert!(capabilities_for_device("arn:aws:braket:::device/qpu/unknown/foo").is_none());
    }

    #[test]
    fn test_arn_for_name() {
        assert_eq!(arn_for_name("rigetti"), Some(RIGETTI_ANKAA_3));
        assert_eq!(arn_for_name("sv1"), Some(SV1));
        assert_eq!(arn_for_name("ionq"), Some(IONQ_ARIA));
        assert!(arn_for_name("nonexistent").is_none());
    }

    #[test]
    fn test_provider_from_arn() {
        assert_eq!(provider_from_arn(RIGETTI_ANKAA_3), "rigetti");
        assert_eq!(provider_from_arn(IONQ_ARIA), "ionq");
        assert_eq!(provider_from_arn(SV1), "amazon");
    }
}
