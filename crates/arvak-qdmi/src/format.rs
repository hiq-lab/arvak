// SPDX-License-Identifier: Apache-2.0
//! Circuit format types and negotiation.

use crate::ffi;

/// Circuit serialization formats that a QDMI device may accept.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CircuitFormat {
    /// OpenQASM 2.0
    OpenQasm2,
    /// OpenQASM 3.0
    OpenQasm3,
    /// QIR (Quantum Intermediate Representation)
    Qir,
    /// Device-native gate sequence (opaque to the framework)
    NativeGates,
    /// A format we don't recognise but the device advertised.
    Custom(String),
}

impl CircuitFormat {
    /// Preference rank (lower = more preferred for Arvak).
    pub(crate) fn preference_rank(&self) -> u32 {
        match self {
            CircuitFormat::OpenQasm3 => 0,
            CircuitFormat::Qir => 1,
            CircuitFormat::OpenQasm2 => 2,
            CircuitFormat::NativeGates => 3,
            CircuitFormat::Custom(_) => 10,
        }
    }

    /// Parse a format name string returned by a QDMI device.
    pub fn from_device_string(s: &str) -> Self {
        match s.to_ascii_lowercase().trim() {
            "openqasm2" | "qasm2" | "openqasm 2" | "openqasm 2.0" => CircuitFormat::OpenQasm2,
            "openqasm3" | "qasm3" | "openqasm 3" | "openqasm 3.0" => CircuitFormat::OpenQasm3,
            "qir" => CircuitFormat::Qir,
            "native" | "nativegates" => CircuitFormat::NativeGates,
            other => CircuitFormat::Custom(other.to_string()),
        }
    }

    /// MIME-style identifier to pass back to the device during job submission.
    pub fn as_device_string(&self) -> &str {
        match self {
            CircuitFormat::OpenQasm2 => "openqasm2",
            CircuitFormat::OpenQasm3 => "openqasm3",
            CircuitFormat::Qir => "qir",
            CircuitFormat::NativeGates => "native",
            CircuitFormat::Custom(s) => s.as_str(),
        }
    }

    /// Convert a QDMI program format code to a `CircuitFormat`.
    ///
    /// Returns `None` for formats Arvak doesn't natively support (e.g. QPY, IQM JSON).
    pub fn from_qdmi_format(fmt: ffi::QdmiProgramFormat) -> Option<Self> {
        match fmt {
            ffi::QDMI_PROGRAM_FORMAT_QASM2 => Some(CircuitFormat::OpenQasm2),
            ffi::QDMI_PROGRAM_FORMAT_QASM3 => Some(CircuitFormat::OpenQasm3),
            ffi::QDMI_PROGRAM_FORMAT_QIRBASESTRING
            | ffi::QDMI_PROGRAM_FORMAT_QIRADAPTIVESTRING => Some(CircuitFormat::Qir),
            // QPY, IQM JSON, binary QIR modules, calibration — not directly supported
            _ => None,
        }
    }

    /// Convert a `CircuitFormat` to the QDMI program format code for job submission.
    pub fn to_qdmi_format(&self) -> Option<ffi::QdmiProgramFormat> {
        match self {
            CircuitFormat::OpenQasm2 => Some(ffi::QDMI_PROGRAM_FORMAT_QASM2),
            CircuitFormat::OpenQasm3 => Some(ffi::QDMI_PROGRAM_FORMAT_QASM3),
            CircuitFormat::Qir => Some(ffi::QDMI_PROGRAM_FORMAT_QIRBASESTRING),
            _ => None,
        }
    }
}

/// Pick the best format from a set the device supports, optionally honouring a
/// user preference.
pub fn negotiate_format(
    supported: &[CircuitFormat],
    preferred: Option<&CircuitFormat>,
) -> Option<CircuitFormat> {
    // If the caller has a preference and the device supports it, use it.
    if let Some(pref) = preferred {
        if supported.contains(pref) {
            return Some(pref.clone());
        }
    }
    // Otherwise pick by our preference ranking.
    let mut ranked: Vec<_> = supported.to_vec();
    ranked.sort_by_key(|f| f.preference_rank());
    ranked.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_negotiation_prefers_user_choice() {
        let supported = vec![CircuitFormat::OpenQasm2, CircuitFormat::OpenQasm3];
        let result = negotiate_format(&supported, Some(&CircuitFormat::OpenQasm2));
        assert_eq!(result, Some(CircuitFormat::OpenQasm2));
    }

    #[test]
    fn test_negotiation_falls_back_to_ranked() {
        let supported = vec![CircuitFormat::OpenQasm2, CircuitFormat::Qir];
        let result = negotiate_format(&supported, Some(&CircuitFormat::NativeGates));
        // NativeGates not supported, should pick Qir (rank 1) over OpenQasm2 (rank 2)
        assert_eq!(result, Some(CircuitFormat::Qir));
    }

    #[test]
    fn test_negotiation_empty() {
        let result = negotiate_format(&[], None);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_format_strings() {
        assert_eq!(CircuitFormat::from_device_string("qasm3"), CircuitFormat::OpenQasm3);
        assert_eq!(CircuitFormat::from_device_string("OpenQASM 2.0"), CircuitFormat::OpenQasm2);
        assert_eq!(
            CircuitFormat::from_device_string("something_else"),
            CircuitFormat::Custom("something_else".into())
        );
    }

    #[test]
    fn test_qdmi_format_roundtrip() {
        assert_eq!(
            CircuitFormat::from_qdmi_format(ffi::QDMI_PROGRAM_FORMAT_QASM2),
            Some(CircuitFormat::OpenQasm2)
        );
        assert_eq!(
            CircuitFormat::from_qdmi_format(ffi::QDMI_PROGRAM_FORMAT_QASM3),
            Some(CircuitFormat::OpenQasm3)
        );
        assert_eq!(
            CircuitFormat::OpenQasm2.to_qdmi_format(),
            Some(ffi::QDMI_PROGRAM_FORMAT_QASM2)
        );
    }
}
