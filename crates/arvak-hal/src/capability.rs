//! Backend capability introspection.
//!
//! This module defines the types that describe what a quantum backend can do:
//! qubit count, supported gates, connectivity topology, and hardware noise
//! characteristics. Compilers use these to decide transpilation strategy;
//! orchestrators use them for routing decisions.
//!
//! # HAL Contract v2
//!
//! The following types are part of the HAL Contract v2 specification:
//! - [`Capabilities`] — top-level hardware descriptor
//! - [`GateSet`] — supported gate operations (OpenQASM 3 naming)
//! - [`Topology`] / [`TopologyKind`] — qubit connectivity graph
//! - [`NoiseProfile`] — device-wide noise averages (gate layer, QEC-visible)
//!
//! All edges in [`Topology`] are bidirectional: if `(a, b)` is present,
//! both `a → b` and `b → a` are valid two-qubit interactions.

use serde::{Deserialize, Serialize};

// ── Re-exported from HAL Contract spec ──────────────────────────────────────
pub use hal_contract::capability::{NoiseProfile, Topology, TopologyKind};

/// Hardware capabilities of a quantum backend.
///
/// Describes what a backend can do: qubit count, supported gates,
/// connectivity, shot limits, and noise characteristics. Compilers
/// use this for transpilation decisions; orchestrators use it for
/// backend routing.
///
/// # HAL Contract v2
///
/// All fields except `features` are defined by the spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    /// Name of the backend.
    pub name: String,
    /// Number of qubits available.
    pub num_qubits: u32,
    /// Supported gate set (OpenQASM 3 naming convention).
    pub gate_set: GateSet,
    /// Qubit connectivity topology. All edges are bidirectional.
    pub topology: Topology,
    /// Maximum number of shots per job.
    pub max_shots: u32,
    /// Maximum gate operations per circuit. `None` means no backend-imposed
    /// limit (HAL Contract v2.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_circuit_ops: Option<u32>,
    /// Whether this is a simulator or emulator (`true`) vs real hardware (`false`).
    /// MUST be set from authoritative source data, not string heuristics.
    pub is_simulator: bool,
    /// Additional capability flags (HAL Contract v2.1 standardised vocabulary):
    /// `"statevector"`, `"dynamic_circuits"`, `"mid_circuit_measurement"`,
    /// `"shuttling"`, `"ion_trap"`, `"neutral_atom"`, `"photonic"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    /// Device-wide noise averages (gate layer, visible to QEC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub noise_profile: Option<NoiseProfile>,
}

impl Capabilities {
    /// Create capabilities for a simulator.
    pub fn simulator(num_qubits: u32) -> Self {
        Self {
            name: "simulator".into(),
            num_qubits,
            gate_set: GateSet::universal(),
            topology: Topology::full(num_qubits),
            max_shots: 100_000,
            max_circuit_ops: None,
            is_simulator: true,
            features: vec!["statevector".into(), "unitary".into()],
            noise_profile: None,
        }
    }

    /// Create capabilities for IQM devices.
    pub fn iqm(name: impl Into<String>, num_qubits: u32) -> Self {
        Self {
            name: name.into(),
            num_qubits,
            gate_set: GateSet::iqm(),
            topology: Topology::star(num_qubits),
            max_shots: 20_000,
            max_circuit_ops: None,
            is_simulator: false,
            features: vec![],
            noise_profile: None,
        }
    }

    /// Create capabilities for IBM devices.
    ///
    /// # Deprecated
    ///
    /// This factory uses `GateSet::ibm()` (wrong CX gate set) and
    /// `Topology::linear()` (wildly wrong for IBM heavy-hex processors).
    /// Use `IbmBackend::connect()` instead — it fetches the real gate set and
    /// topology from the IBM Cloud API.
    ///
    /// For Eagle (127q) processors use `GateSet::ibm_eagle()`;
    /// for Heron (156q) processors use `GateSet::ibm_heron()`.
    #[deprecated(
        since = "1.9.0",
        note = "Use IbmBackend::connect() — this factory has wrong gate set (CX) and wrong topology (linear)"
    )]
    #[allow(deprecated)]
    pub fn ibm(name: impl Into<String>, num_qubits: u32) -> Self {
        Self {
            name: name.into(),
            num_qubits,
            gate_set: GateSet::ibm(),
            topology: Topology::linear(num_qubits), // placeholder — use connect() for real topology
            max_shots: 100_000,
            max_circuit_ops: None,
            is_simulator: false,
            features: vec!["dynamic_circuits".into()],
            noise_profile: None,
        }
    }

    /// Create capabilities for AQT (Alpine Quantum Technologies) ion-trap devices.
    ///
    /// AQT hardware and simulators have all-to-all qubit connectivity.
    /// Maximum: 20 qubits, 2000 shots, 2000 operations per circuit.
    pub fn aqt(name: impl Into<String>, num_qubits: u32) -> Self {
        Self {
            name: name.into(),
            num_qubits,
            gate_set: GateSet::aqt(),
            topology: Topology::full(num_qubits),
            max_shots: 2_000,
            max_circuit_ops: Some(2_000),
            is_simulator: false,
            features: vec!["ion_trap".into()],
            noise_profile: None,
        }
    }

    /// Create capabilities for IonQ trapped-ion devices (Aria, Forte).
    ///
    /// IonQ hardware has all-to-all qubit connectivity.
    /// The QIS gateset supports standard gates (h, cx, rx, ry, rz, etc.)
    /// which IonQ compiles to native gates (gpi, gpi2, ms) server-side.
    pub fn ionq(name: impl Into<String>, num_qubits: u32) -> Self {
        Self {
            name: name.into(),
            num_qubits,
            gate_set: GateSet::ionq(),
            topology: Topology::full(num_qubits),
            max_shots: 1_000_000,
            max_circuit_ops: None,
            is_simulator: false,
            features: vec!["ion_trap".into()],
            noise_profile: None,
        }
    }

    /// Create capabilities for Quantinuum H1/H2 ion-trap devices.
    ///
    /// All Quantinuum hardware has all-to-all qubit connectivity.
    pub fn quantinuum(name: impl Into<String>, num_qubits: u32) -> Self {
        Self {
            name: name.into(),
            num_qubits,
            gate_set: GateSet::quantinuum(),
            topology: Topology::full(num_qubits),
            max_shots: 10_000,
            max_circuit_ops: None,
            is_simulator: false,
            features: vec!["ion_trap".into(), "mid_circuit_measurement".into()],
            noise_profile: None,
        }
    }

    /// Create capabilities for Quandela Altair photonic QPU.
    ///
    /// 5 logical qubits encoded in 10 photonic modes (dual-rail encoding, 2 modes per qubit).
    /// All-to-all connectivity via programmable beamsplitter network.
    pub fn quandela(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            num_qubits: 5,
            gate_set: GateSet::quandela(),
            topology: Topology::full(5),
            max_shots: 100_000,
            max_circuit_ops: None,
            is_simulator: false,
            features: vec!["photonic".into()],
            noise_profile: None,
        }
    }

    /// Create capabilities for a neutral-atom device (e.g., planqc, Pasqal).
    pub fn neutral_atom(name: impl Into<String>, num_qubits: u32, zones: u32) -> Self {
        Self {
            name: name.into(),
            num_qubits,
            gate_set: GateSet::neutral_atom(),
            topology: Topology::neutral_atom(num_qubits, zones),
            max_shots: 100_000,
            max_circuit_ops: None,
            is_simulator: false,
            features: vec!["shuttling".into(), "zoned".into()],
            noise_profile: None,
        }
    }

    /// Create capabilities for Braket Rigetti devices (superconducting).
    pub fn braket_rigetti(name: impl Into<String>, num_qubits: u32) -> Self {
        Self {
            name: name.into(),
            num_qubits,
            gate_set: GateSet::rigetti(),
            topology: Topology::grid(
                f64::from(num_qubits).sqrt().ceil() as u32,
                f64::from(num_qubits).sqrt().ceil() as u32,
            ),
            max_shots: 100_000,
            max_circuit_ops: None,
            is_simulator: false,
            features: vec![],
            noise_profile: None,
        }
    }

    /// Create capabilities for Braket IonQ devices (trapped-ion).
    pub fn braket_ionq(name: impl Into<String>, num_qubits: u32) -> Self {
        Self {
            name: name.into(),
            num_qubits,
            gate_set: GateSet::braket_ionq(),
            topology: Topology::full(num_qubits),
            max_shots: 100_000,
            max_circuit_ops: None,
            is_simulator: false,
            features: vec![],
            noise_profile: None,
        }
    }

    /// Create capabilities for Braket managed simulators (SV1, TN1, DM1).
    pub fn braket_simulator(name: impl Into<String>, num_qubits: u32) -> Self {
        Self {
            name: name.into(),
            num_qubits,
            gate_set: GateSet::universal(),
            topology: Topology::full(num_qubits),
            max_shots: 100_000,
            max_circuit_ops: None,
            is_simulator: true,
            features: vec!["braket_simulator".into()],
            noise_profile: None,
        }
    }

    /// Override the topology with real hardware connectivity.
    pub fn with_topology(mut self, topology: Topology) -> Self {
        self.topology = topology;
        self
    }

    /// Attach a noise profile to these capabilities.
    pub fn with_noise_profile(mut self, profile: NoiseProfile) -> Self {
        self.noise_profile = Some(profile);
        self
    }
}

/// Gate set supported by a backend.
///
/// Gate names follow the OpenQASM 3 naming convention (lowercase):
/// `h`, `cx`, `rz`, `prx`, etc.
///
/// The `native` list identifies gates that execute without decomposition.
/// If `native` is empty, all supported gates are considered native
/// (typical for simulators).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateSet {
    /// Single-qubit gates supported.
    pub single_qubit: Vec<String>,
    /// Two-qubit gates supported.
    pub two_qubit: Vec<String>,
    /// Three-qubit gates supported.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub three_qubit: Vec<String>,
    /// Native gates (execute without decomposition on this backend).
    pub native: Vec<String>,
}

impl GateSet {
    /// Create AQT (Alpine Quantum Technologies) gate set.
    ///
    /// AQT native gates: `rz` (Z rotation), `prx` (phased-X / R gate),
    /// `rxx` (Mølmer-Sørensen XX rotation).
    ///
    /// In Arvak's IR, AQT's `R` (phased-X) gate is represented as `PRX(θ, φ)`.
    /// All angles must be concrete (non-symbolic) at submission time.
    pub fn aqt() -> Self {
        Self {
            single_qubit: vec!["rz".into(), "prx".into()],
            two_qubit: vec!["rxx".into()],
            three_qubit: vec![],
            native: vec!["rz".into(), "prx".into(), "rxx".into()],
        }
    }

    /// Create IQM gate set.
    pub fn iqm() -> Self {
        Self {
            single_qubit: vec!["prx".into()],
            two_qubit: vec!["cz".into()],
            three_qubit: vec![],
            native: vec!["prx".into(), "cz".into()],
        }
    }

    /// Create IBM Eagle gate set (127-qubit processors: ibm_brussels, ibm_strasbourg, etc.).
    ///
    /// Eagle native gates: `ecr, rz, sx, x`. IBM retired CX-native hardware with Falcon.
    pub fn ibm_eagle() -> Self {
        Self {
            single_qubit: vec!["rz".into(), "sx".into(), "x".into(), "id".into()],
            two_qubit: vec!["ecr".into()],
            three_qubit: vec![],
            native: vec!["rz".into(), "sx".into(), "x".into(), "ecr".into()],
        }
    }

    /// Create IBM Heron gate set (156-qubit processors: ibm_torino, ibm_marrakesh, etc.).
    ///
    /// Heron native gates: `cz, rz, sx, x`. QAOA circuits use `h`, `rx`, and `rzz`;
    /// these are listed as supported so `validate()` accepts them, but they are NOT
    /// in `native` — Arvak's compiler decomposes them to true native gates before
    /// submission. `h` → `rz·sx·rz`, `rx(θ)` → `rz·sx·rz`, `rzz(θ)` → `cx·rz·cx`
    /// (cx further decomposes to `h·cz·h`). IBM then only handles physical routing.
    pub fn ibm_heron() -> Self {
        Self {
            single_qubit: vec![
                "rz".into(),
                "sx".into(),
                "x".into(),
                "id".into(),
                "rx".into(),
                "h".into(),
            ],
            two_qubit: vec!["cz".into(), "rzz".into()],
            three_qubit: vec![],
            native: vec![
                "rz".into(),
                "sx".into(),
                "x".into(),
                "cz".into(),
                "id".into(),
            ],
        }
    }

    /// Create IBM gate set.
    ///
    /// # Deprecated
    ///
    /// This method is wrong: it hardcodes CX as the two-qubit gate, but IBM
    /// retired CX-native hardware with Falcon processors.
    /// - For Eagle (127q) backends use [`GateSet::ibm_eagle()`] (ECR native).
    /// - For Heron (156q) backends use [`GateSet::ibm_heron()`] (CZ native).
    ///
    /// Use `IbmBackend::connect()` to fetch the real gate set from the IBM API.
    #[deprecated(
        since = "1.9.0",
        note = "Use ibm_eagle() or ibm_heron() instead; Capabilities::ibm() uses wrong CX gate set"
    )]
    pub fn ibm() -> Self {
        Self {
            single_qubit: vec!["rz".into(), "sx".into(), "x".into(), "id".into()],
            two_qubit: vec!["cx".into()],
            three_qubit: vec![],
            native: vec!["rz".into(), "sx".into(), "x".into(), "cx".into()],
        }
    }

    /// Create universal gate set.
    pub fn universal() -> Self {
        Self {
            single_qubit: vec![
                "id".into(),
                "x".into(),
                "y".into(),
                "z".into(),
                "h".into(),
                "s".into(),
                "sdg".into(),
                "t".into(),
                "tdg".into(),
                "sx".into(),
                "sxdg".into(),
                "rx".into(),
                "ry".into(),
                "rz".into(),
                "p".into(),
                "u".into(),
                "prx".into(),
            ],
            two_qubit: vec![
                "cx".into(),
                "cy".into(),
                "cz".into(),
                "ch".into(),
                "swap".into(),
                "iswap".into(),
                "crx".into(),
                "cry".into(),
                "crz".into(),
                "cp".into(),
                "rxx".into(),
                "ryy".into(),
                "rzz".into(),
            ],
            three_qubit: vec!["ccx".into(), "cswap".into()],
            native: vec![],
        }
    }

    /// Create Rigetti gate set (superconducting).
    ///
    /// Native gates: RX, RZ (single-qubit), CZ (two-qubit).
    pub fn rigetti() -> Self {
        Self {
            single_qubit: vec!["rx".into(), "rz".into()],
            two_qubit: vec!["cz".into()],
            three_qubit: vec![],
            native: vec!["rx".into(), "rz".into(), "cz".into()],
        }
    }

    /// Create IonQ gate set (trapped-ion).
    ///
    /// Uses the QIS gateset: standard gates that IonQ compiles to native
    /// gates (gpi, gpi2, ms) server-side. This avoids requiring Arvak to
    /// compile to IonQ native gates.
    pub fn ionq() -> Self {
        Self {
            single_qubit: vec![
                "h".into(),
                "x".into(),
                "y".into(),
                "z".into(),
                "rx".into(),
                "ry".into(),
                "rz".into(),
                "s".into(),
                "sdg".into(),
                "t".into(),
                "tdg".into(),
                "sx".into(),
            ],
            two_qubit: vec![
                "cx".into(),
                "swap".into(),
                "xx".into(),
                "yy".into(),
                "zz".into(),
            ],
            three_qubit: vec!["ccx".into()],
            native: vec!["gpi".into(), "gpi2".into(), "ms".into(), "zz".into()],
        }
    }

    /// Create IonQ gate set for AWS Braket (restricted to Braket's translation layer).
    ///
    /// Braket compiles to IonQ native gates (gpi, gpi2, ms) server-side,
    /// but only exposes rx, ry, rz, xx as the input gate set.
    pub fn braket_ionq() -> Self {
        Self {
            single_qubit: vec!["rx".into(), "ry".into(), "rz".into()],
            two_qubit: vec!["xx".into()],
            three_qubit: vec![],
            native: vec!["gpi".into(), "gpi2".into(), "ms".into()],
        }
    }

    /// Create Quantinuum gate set (H1/H2 ion-trap processors).
    ///
    /// Quantinuum's cloud service accepts standard QASM 2.0 gates and compiles
    /// them to its native ion-trap gate set (ZZMax/ZZPhase/U1q/Rz) internally.
    /// `rz` is listed as a "native" gate because it executes as a virtual Z
    /// rotation (zero hardware cost).
    pub fn quantinuum() -> Self {
        Self {
            single_qubit: vec![
                "rz".into(),
                "rx".into(),
                "ry".into(),
                "h".into(),
                "x".into(),
                "y".into(),
                "z".into(),
                "s".into(),
                "t".into(),
                "sdg".into(),
                "tdg".into(),
                "sx".into(),
            ],
            two_qubit: vec!["cx".into(), "cz".into(), "swap".into()],
            three_qubit: vec!["ccx".into()],
            // Rz is "free" (virtual Z); all others decompose on the server.
            native: vec!["rz".into()],
        }
    }

    /// Create Quandela gate set (photonic, dual-rail encoding via perceval-interop).
    ///
    /// Supported gates are those expressible via Perceval's dual-rail encoding.
    /// Native basis: `rz`, `h`, `cx` (minimal perceval-interop basis).
    pub fn quandela() -> Self {
        Self {
            single_qubit: vec![
                "rz".into(),
                "h".into(),
                "x".into(),
                "sx".into(),
                "rx".into(),
                "ry".into(),
            ],
            two_qubit: vec!["cx".into(), "cz".into()],
            three_qubit: vec!["ccx".into(), "ccz".into()],
            native: vec!["rz".into(), "h".into(), "cx".into()],
        }
    }

    /// Create a neutral-atom gate set.
    ///
    /// Native gates: Global RZ, Rydberg CZ/CCZ.
    pub fn neutral_atom() -> Self {
        Self {
            single_qubit: vec!["rz".into(), "rx".into(), "ry".into()],
            two_qubit: vec!["cz".into()],
            three_qubit: vec![],
            native: vec!["rz".into(), "rx".into(), "ry".into(), "cz".into()],
        }
    }

    /// Check if a gate is supported (single-qubit, two-qubit, or three-qubit).
    pub fn contains(&self, gate: &str) -> bool {
        self.single_qubit.iter().any(|g| g == gate)
            || self.two_qubit.iter().any(|g| g == gate)
            || self.three_qubit.iter().any(|g| g == gate)
    }

    /// Check if a gate is native (executes without decomposition).
    ///
    /// If the `native` list is empty, all supported gates are considered
    /// native — this is the typical case for simulators.
    pub fn is_native(&self, gate: &str) -> bool {
        if self.native.is_empty() {
            self.contains(gate)
        } else {
            self.native.iter().any(|g| g == gate)
        }
    }
}

// Topology, TopologyKind, and NoiseProfile are re-exported from
// hal_contract::capability at the top of this file. The implementations
// (factory methods, is_connected(), etc.) live in the hal-contract crate.
// Arvak-hal uses them directly without extending.

// (Topology impl + TopologyKind enum now in hal_contract::capability)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities_simulator() {
        let caps = Capabilities::simulator(10);
        assert!(caps.is_simulator);
        assert_eq!(caps.num_qubits, 10);
        assert!(caps.gate_set.contains("h"));
    }

    #[test]
    fn test_capabilities_iqm() {
        let caps = Capabilities::iqm("Garnet", 20);
        assert!(!caps.is_simulator);
        assert!(caps.gate_set.contains("prx"));
        assert!(caps.gate_set.contains("cz"));
        assert!(!caps.gate_set.contains("cx"));
    }

    #[test]
    fn test_topology_linear() {
        let topo = Topology::linear(5);
        assert!(topo.is_connected(0, 1));
        assert!(topo.is_connected(1, 2));
        assert!(!topo.is_connected(0, 2));
    }

    #[test]
    fn test_topology_star() {
        let topo = Topology::star(5);
        assert!(topo.is_connected(0, 1));
        assert!(topo.is_connected(0, 4));
        assert!(!topo.is_connected(1, 2));
    }

    #[test]
    fn test_topology_grid() {
        let topo = Topology::grid(2, 3);
        // Grid:
        // 0 - 1 - 2
        // |   |   |
        // 3 - 4 - 5
        assert!(topo.is_connected(0, 1));
        assert!(topo.is_connected(1, 2));
        assert!(topo.is_connected(0, 3));
        assert!(topo.is_connected(1, 4));
        assert!(!topo.is_connected(0, 4)); // Diagonal not connected
    }

    #[test]
    fn test_topology_neutral_atom() {
        // 6 qubits, 2 zones: zone0=[0,1,2], zone1=[3,4,5]
        let topo = Topology::neutral_atom(6, 2);
        assert_eq!(topo.kind, TopologyKind::NeutralAtom { zones: 2 });

        // Within zone 0: fully connected
        assert!(topo.is_connected(0, 1));
        assert!(topo.is_connected(0, 2));
        assert!(topo.is_connected(1, 2));

        // Within zone 1: fully connected
        assert!(topo.is_connected(3, 4));
        assert!(topo.is_connected(3, 5));
        assert!(topo.is_connected(4, 5));

        // Across zones: not connected (requires shuttling)
        assert!(!topo.is_connected(2, 3));
        assert!(!topo.is_connected(0, 5));
    }

    #[test]
    fn test_capabilities_neutral_atom() {
        let caps = Capabilities::neutral_atom("planqc-atom1", 100, 4);
        assert!(!caps.is_simulator);
        assert_eq!(caps.num_qubits, 100);
        assert!(caps.gate_set.contains("cz"));
        assert!(caps.gate_set.contains("rz"));
        assert!(!caps.gate_set.contains("cx"));
        assert!(caps.features.contains(&"shuttling".to_string()));
    }

    #[test]
    fn test_capabilities_braket_rigetti() {
        let caps = Capabilities::braket_rigetti("Ankaa-3", 84);
        assert!(!caps.is_simulator);
        assert_eq!(caps.num_qubits, 84);
        assert!(caps.gate_set.contains("rx"));
        assert!(caps.gate_set.contains("rz"));
        assert!(caps.gate_set.contains("cz"));
        assert!(!caps.gate_set.contains("cx"));
    }

    #[test]
    fn test_capabilities_braket_ionq() {
        let caps = Capabilities::braket_ionq("IonQ Aria", 25);
        assert!(!caps.is_simulator);
        assert_eq!(caps.num_qubits, 25);
        assert!(caps.gate_set.contains("rx"));
        assert!(caps.gate_set.contains("ry"));
        assert!(caps.gate_set.contains("rz"));
        assert!(caps.gate_set.contains("xx"));
        assert!(!caps.gate_set.contains("cx"));
    }

    #[test]
    fn test_capabilities_quantinuum() {
        let caps = Capabilities::quantinuum("H2-1LE", 32);
        assert!(!caps.is_simulator);
        assert_eq!(caps.num_qubits, 32);
        assert!(caps.gate_set.contains("cx"));
        assert!(caps.gate_set.contains("rz"));
        assert!(caps.gate_set.contains("h"));
        assert!(caps.gate_set.contains("ccx"));
        assert!(!caps.gate_set.contains("prx"));
        assert!(caps.features.contains(&"ion_trap".to_string()));
        assert!(
            caps.features
                .contains(&"mid_circuit_measurement".to_string())
        );
        // All-to-all topology
        assert!(caps.topology.is_connected(0, 1));
        assert!(caps.topology.is_connected(0, 31));
        assert!(caps.topology.is_connected(15, 31));
    }

    #[test]
    fn test_gate_set_quantinuum() {
        let gs = GateSet::quantinuum();
        assert!(gs.contains("rz"));
        assert!(gs.contains("rx"));
        assert!(gs.contains("cx"));
        assert!(gs.contains("cz"));
        assert!(gs.contains("ccx"));
        assert!(!gs.contains("prx"));
        assert!(!gs.contains("ecr"));
        // Rz is the only declared native gate
        assert!(gs.is_native("rz"));
        assert!(!gs.is_native("cx"));
    }

    #[test]
    fn test_capabilities_braket_simulator() {
        let caps = Capabilities::braket_simulator("SV1", 34);
        assert!(caps.is_simulator);
        assert_eq!(caps.num_qubits, 34);
        assert!(caps.gate_set.contains("h"));
        assert!(caps.gate_set.contains("cx"));
    }

    #[test]
    fn test_gate_set_is_native() {
        let gs = GateSet {
            single_qubit: vec!["h".into(), "rx".into()],
            two_qubit: vec!["cx".into()],
            three_qubit: vec![],
            native: vec!["rx".into(), "cx".into()],
        };
        assert!(gs.is_native("rx"));
        assert!(gs.is_native("cx"));
        assert!(!gs.is_native("h")); // supported but not native
    }

    #[test]
    fn test_gate_set_is_native_empty_native_list() {
        let gs = GateSet {
            single_qubit: vec!["h".into()],
            two_qubit: vec!["cx".into()],
            three_qubit: vec![],
            native: vec![],
        };
        // When native is empty, all supported gates are native
        assert!(gs.is_native("h"));
        assert!(gs.is_native("cx"));
        assert!(!gs.is_native("cz"));
    }
}
