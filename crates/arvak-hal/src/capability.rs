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
//! # HAL Contract v2.2 — Alsvid Extension
//!
//! The following types extend the contract for physical-layer attestation:
//! - [`CoolingProfile`] — physical cooling layer descriptor (QEC-invisible)
//! - [`CompressorSpec`] — cryogenic compressor hardware specification
//! - [`CompressorType`] — cryogenic compressor mechanism (HAL Contract v2.3)
//! - [`TransferFunctionSample`] — H(f) vibration-to-decoherence coupling point
//! - [`QuietWindow`] — low-vibration scheduling window within compressor cycle
//! - [`PufEnrollment`] — PUF enrollment record for hardware provenance
//! - [`DecoherenceMonitor`] — trait for real-time T1/T2* measurement and fingerprinting
//!
//! # HAL Contract v2.3 — Photonic Extension
//!
//! Additional types and methods for photonic QPU support:
//! - [`CompressorType`] — replaces `rotary_valve: bool` with an extensible enum (DEBT-Q2)
//! - [`TransferFunctionSample::visibility_modulation`] — HOM visibility metric for photonic
//!   backends (DEBT-Q3)
//! - [`DecoherenceMonitor::measure_hom_visibility`] — HOM visibility measurement (DEBT-Q1)
//! - [`DecoherenceMonitor::compute_hom_fingerprint`] — photonic PUF fingerprint (DEBT-Q1)
//!
//! All edges in [`Topology`] are bidirectional: if `(a, b)` is present,
//! both `a → b` and `b → a` are valid two-qubit interactions.

use serde::{Deserialize, Serialize};

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
    /// Physical cooling layer profile (HAL Contract v2.2, Alsvid extension).
    ///
    /// Captures the cryogenic cooling infrastructure characteristics used by
    /// Alsvid for hardware attestation, tamper detection, and quiet-window
    /// scheduling. Invisible to Quantum Error Correction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooling_profile: Option<CoolingProfile>,
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
            cooling_profile: None,
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
            cooling_profile: None,
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
            cooling_profile: None,
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
            cooling_profile: None,
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
            cooling_profile: None,
        }
    }

    /// Create capabilities for Quandela Altair photonic QPU.
    ///
    /// 5 logical qubits encoded in 10 photonic modes (dual-rail encoding, 2 modes per qubit).
    /// All-to-all connectivity via programmable beamsplitter network.
    /// Transfer function and PUF enrollment are populated after Alsvid enrollment.
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
            cooling_profile: Some(CoolingProfile::new(CompressorSpec {
                model: "Quandela Altair 4K cryocooler".into(),
                cycle_frequency_hz: 1.0,
                stage_temperatures_k: vec![4.0],
                compressor_type: CompressorType::GiffordMcMahon,
            })),
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
            cooling_profile: None,
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
            cooling_profile: None,
        }
    }

    /// Create capabilities for Braket IonQ devices (trapped-ion).
    pub fn braket_ionq(name: impl Into<String>, num_qubits: u32) -> Self {
        Self {
            name: name.into(),
            num_qubits,
            gate_set: GateSet::ionq(),
            topology: Topology::full(num_qubits),
            max_shots: 100_000,
            max_circuit_ops: None,
            is_simulator: false,
            features: vec![],
            noise_profile: None,
            cooling_profile: None,
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
            cooling_profile: None,
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

    /// Attach a cooling profile to these capabilities (HAL Contract v2.2).
    ///
    /// Enables Alsvid hardware attestation, tamper detection, and
    /// quiet-window scheduling for this backend.
    pub fn with_cooling_profile(mut self, profile: CoolingProfile) -> Self {
        self.cooling_profile = Some(profile);
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
    /// Native gates: RX, RY, RZ (single-qubit), XX (two-qubit).
    pub fn ionq() -> Self {
        Self {
            single_qubit: vec!["rx".into(), "ry".into(), "rz".into()],
            two_qubit: vec!["xx".into()],
            three_qubit: vec![],
            native: vec!["rx".into(), "ry".into(), "rz".into(), "xx".into()],
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

/// Qubit connectivity topology.
///
/// All edges are bidirectional: if `(a, b)` is listed, both `a → b`
/// and `b → a` are valid two-qubit interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topology {
    /// Kind of topology.
    pub kind: TopologyKind,
    /// Coupling edges (pairs of connected qubits). Bidirectional.
    pub edges: Vec<(u32, u32)>,
}

impl Topology {
    /// Create a linear topology.
    pub fn linear(n: u32) -> Self {
        let edges: Vec<_> = (0..n.saturating_sub(1)).map(|i| (i, i + 1)).collect();
        Self {
            kind: TopologyKind::Linear,
            edges,
        }
    }

    /// Create a star topology.
    pub fn star(n: u32) -> Self {
        let edges: Vec<_> = (1..n).map(|i| (0, i)).collect();
        Self {
            kind: TopologyKind::Star,
            edges,
        }
    }

    /// Create a fully connected topology.
    pub fn full(n: u32) -> Self {
        let mut edges = vec![];
        for i in 0..n {
            for j in (i + 1)..n {
                edges.push((i, j));
            }
        }
        Self {
            kind: TopologyKind::FullyConnected,
            edges,
        }
    }

    /// Create a grid topology.
    pub fn grid(rows: u32, cols: u32) -> Self {
        let mut edges = vec![];
        for r in 0..rows {
            for c in 0..cols {
                let idx = r * cols + c;
                // Horizontal edge
                if c + 1 < cols {
                    edges.push((idx, idx + 1));
                }
                // Vertical edge
                if r + 1 < rows {
                    edges.push((idx, idx + cols));
                }
            }
        }
        Self {
            kind: TopologyKind::Grid { rows, cols },
            edges,
        }
    }

    /// Create a custom topology from edges.
    pub fn custom(edges: Vec<(u32, u32)>) -> Self {
        Self {
            kind: TopologyKind::Custom,
            edges,
        }
    }

    /// Create a neutral-atom topology with zones.
    ///
    /// Qubits within a zone are fully connected (Rydberg interaction radius).
    /// Qubits across zones require shuttling.
    pub fn neutral_atom(num_qubits: u32, zones: u32) -> Self {
        let qubits_per_zone = num_qubits / zones.max(1);
        let mut edges = vec![];

        // Full connectivity within each zone
        for z in 0..zones {
            let start = z * qubits_per_zone;
            let end = if z == zones - 1 {
                num_qubits
            } else {
                start + qubits_per_zone
            };
            for i in start..end {
                for j in (i + 1)..end {
                    edges.push((i, j));
                }
            }
        }

        Self {
            kind: TopologyKind::NeutralAtom { zones },
            edges,
        }
    }

    /// Check if two qubits are connected.
    pub fn is_connected(&self, q1: u32, q2: u32) -> bool {
        self.edges
            .iter()
            .any(|&(a, b)| (a == q1 && b == q2) || (a == q2 && b == q1))
    }
}

/// Kind of qubit topology.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum TopologyKind {
    /// Fully connected (all-to-all).
    FullyConnected,
    /// Linear chain.
    Linear,
    /// Star topology (center connected to all).
    Star,
    /// 2D grid.
    Grid { rows: u32, cols: u32 },
    /// Heavy-hex lattice (IBM Heron/Eagle processors).
    HeavyHex,
    /// Custom topology.
    Custom,
    /// Neutral-atom topology with reconfigurable zones.
    NeutralAtom {
        /// Number of interaction zones.
        zones: u32,
    },
}

/// Device-wide noise averages reported by a backend (gate layer, QEC-visible).
///
/// These are aggregate characterization numbers — suitable for routing
/// and coarse-grained compilation decisions. Per-qubit / per-gate detail
/// lives in the IR-level noise profile (`arvak_ir::noise::NoiseProfile`),
/// which the compiler consumes directly.
///
/// All fidelity values are in `[0.0, 1.0]` where `1.0` means perfect.
/// Time values (T1, T2, gate_time) are in **microseconds**.
///
/// # Note
///
/// This profile captures noise at the gate layer (L2+), which is visible
/// to Quantum Error Correction. For physical-layer noise originating in
/// the cryogenic cooling infrastructure (QEC-invisible), see [`CoolingProfile`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoiseProfile {
    /// T1 relaxation time (device average, microseconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t1: Option<f64>,
    /// T2 dephasing time (device average, microseconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t2: Option<f64>,
    /// Average single-qubit gate fidelity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub single_qubit_fidelity: Option<f64>,
    /// Average two-qubit gate fidelity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub two_qubit_fidelity: Option<f64>,
    /// Average readout fidelity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readout_fidelity: Option<f64>,
    /// Average gate execution time (microseconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_time: Option<f64>,
}

// ─── HAL Contract v2.2 / v2.3 — Alsvid Physical Layer Extension ─────────────

/// Cryogenic compressor mechanism type.
///
/// Determines the vibration signature and PUF signal characteristics.
/// Extends the former `rotary_valve: bool` field (HAL Contract v2.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressorType {
    /// Rotary-valve pulse-tube (e.g., PWG500, PT415, PT407).
    /// Produces characteristic ~1 Hz vibration. Primary Alsvid PUF signal.
    RotaryValve,
    /// Gifford-McMahon (piston-based). Typical 4K stage in photonic QPUs.
    /// Different harmonic profile from rotary valve.
    GiffordMcMahon,
    /// Stirling-cycle (e.g., Pressure Wave Systems metal bellows, patent WO2014016415A2).
    /// Odd harmonics only — triangular pressure wave.
    Stirling,
    /// Pulse-tube without rotary valve (passive pulse tube / double-inlet).
    PulseTube,
    /// Other or unknown compressor type.
    Other(String),
}

/// Cryogenic compressor hardware specification.
///
/// Describes the physical compressor that cools the QPU to operating
/// temperature. Used by Alsvid to characterise the Physical Unclonable
/// Function (PUF) derived from compressor vibration coupling to qubit
/// decoherence.
///
/// The dominant vibration source in rotary-valve pulse-tube compressors
/// is the valve mechanism itself (not the compressor body), producing a
/// characteristic 1–1.2 Hz oscillation that is measurably synchronised
/// with T1 fluctuations in transmon qubits (Kosen et al., 2024).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressorSpec {
    /// Manufacturer and model (e.g., `"Pressure Wave Systems PWG500"`).
    pub model: String,
    /// Dominant mechanical cycle frequency in Hz.
    ///
    /// Typically 1.0–1.2 Hz for rotary-valve pulse-tube compressors (PWG500,
    /// IGLU/attoCMC). This frequency sets the fundamental PUF signal period.
    pub cycle_frequency_hz: f64,
    /// Cooling stage temperatures in Kelvin, outermost to coldest plate.
    ///
    /// Typical superconducting QPU: `[50.0, 4.0, 0.8, 0.1, 0.01]`
    /// (50K shield, 4K stage, 800mK still, 100mK cold plate, 10mK mixing chamber).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stage_temperatures_k: Vec<f64>,
    /// Compressor mechanism type (HAL Contract v2.3).
    ///
    /// Replaces the former `rotary_valve: bool` field. Determines the
    /// vibration harmonic profile and PUF signal characteristics.
    pub compressor_type: CompressorType,
}

impl CompressorSpec {
    /// Returns `true` if this compressor uses a rotary valve (pulse-tube).
    ///
    /// Convenience method for backward-compatible reading. Equivalent to
    /// `self.compressor_type == CompressorType::RotaryValve`.
    pub fn is_rotary_valve(&self) -> bool {
        self.compressor_type == CompressorType::RotaryValve
    }
}

/// One sample of the H(f) vibration-to-decoherence transfer function.
///
/// Represents the measured coupling strength between compressor vibration
/// at frequency `freq_hz` and T1 modulation amplitude `t1_modulation`.
/// A full set of samples across the relevant frequency range (0.1–100 Hz)
/// constitutes the installation fingerprint.
///
/// For photonic backends, `visibility_modulation` replaces `t1_modulation`
/// as the primary coupling metric (HAL Contract v2.3, DEBT-Q3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferFunctionSample {
    /// Frequency in Hz.
    pub freq_hz: f64,
    /// T1 modulation amplitude (normalised, unitless).
    ///
    /// `0.0` — no coupling at this frequency.
    /// `1.0` — full modulation of the T1 baseline at this frequency.
    /// Typical values near the compressor fundamental: 0.05–0.20.
    /// `None` for photonic backends (use `visibility_modulation` instead).
    pub t1_modulation: f64,
    /// HOM visibility modulation amplitude (photonic backends only).
    ///
    /// Normalised [0.0, 1.0]: fraction of baseline HOM visibility degraded
    /// by compressor vibration at this frequency.
    /// `None` for superconducting backends (use `t1_modulation` instead).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility_modulation: Option<f64>,
}

/// A quiet window: a sub-interval of the compressor cycle with minimal
/// vibration coupling to qubits.
///
/// The compressor cycle (typically ~1 second for rotary-valve units)
/// is not uniformly noisy. Dead zones around valve transitions produce
/// intervals of reduced vibration. Circuits scheduled in quiet windows
/// see lower effective decoherence.
///
/// Both `cycle_offset` and `cycle_fraction` are fractions of the full
/// cycle period (dimensionless, `[0.0, 1.0)`). To convert to absolute
/// time: multiply by `1.0 / compressor.cycle_frequency_hz`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuietWindow {
    /// Start of the quiet window as a fraction of the compressor cycle `[0.0, 1.0)`.
    pub cycle_offset: f64,
    /// Duration of the quiet window as a fraction of the compressor cycle `(0.0, 1.0]`.
    pub cycle_fraction: f64,
    /// Estimated T1 improvement factor relative to the cycle average.
    ///
    /// `1.0` — no improvement (same as average).
    /// `1.15` — 15% longer T1 during this window.
    /// Populated by the decoherence monitor after measurement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t1_improvement_factor: Option<f64>,
}

impl QuietWindow {
    /// Duration of this quiet window in seconds.
    pub fn duration_secs(&self, cycle_frequency_hz: f64) -> f64 {
        self.cycle_fraction / cycle_frequency_hz
    }
}

/// PUF enrollment record — the reference fingerprint for this installation.
///
/// Captured once during the enrollment phase by `DecoherenceMonitor::compute_fingerprint`.
/// Stored in the `CoolingProfile` and embedded in Provenance Certificates.
/// Subsequent verification measurements are compared to this record to
/// confirm hardware identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PufEnrollment {
    /// Installation-unique identifier (e.g., serial number + site hash).
    pub installation_id: String,
    /// Unix timestamp (seconds since epoch) of the enrollment measurement.
    pub enrolled_at: u64,
    /// SHA-256 fingerprint hash of the T1 modulation spectrum vector.
    ///
    /// Computed by quantising the `TransferFunctionSample` amplitudes to
    /// 8-bit bins and hashing the resulting byte vector. Hex-encoded,
    /// 64 characters.
    pub fingerprint_hash: String,
    /// Number of measurement shots used per sample point during enrollment.
    ///
    /// Higher values → more stable fingerprint. Minimum recommended: 1000.
    pub enrollment_shots: u32,
    /// Maximum acceptable Hamming distance (fraction) between enrollment
    /// and verification fingerprints for the same installation (intra-distance).
    ///
    /// For a reliable PUF at 3σ separation, `intra_distance_threshold` should
    /// be below 10% of the total bit length. Typical value: 0.08.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intra_distance_threshold: Option<f64>,
}

/// Physical cooling layer profile for a superconducting QPU.
///
/// This is the Alsvid extension to the HAL Contract (v2.2). Where
/// [`NoiseProfile`] captures gate-layer noise visible to Quantum Error
/// Correction, `CoolingProfile` captures the *physical layer below the
/// gate stack* — the vibration coupling of the cryogenic cooling
/// infrastructure to qubit decoherence.
///
/// This layer is **invisible to QEC** because QEC operates at gate-layer
/// frequencies (GHz clock). The compressor vibration signal lives at
/// 1–10 Hz, orders of magnitude lower — outside every existing error
/// correction band.
///
/// # Alsvid Use Cases
///
/// 1. **Hardware attestation** — enrol the T1 modulation spectrum as a PUF;
///    verify on each job submission to prove hardware provenance (analogous
///    to Intel SGX remote attestation, but for quantum hardware).
///
/// 2. **Tamper detection** — physical tampering (mass redistribution,
///    remounting, adjacent equipment changes) measurably shifts the vibration
///    coupling profile, invisible to any software-layer security check.
///
/// 3. **Compiler quiet-window scheduling** — inject timing hints so circuits
///    execute during low-vibration intervals, reducing effective decoherence
///    without changing any gate logic.
///
/// # References
///
/// Kosen et al., *Nature Communications* 15, 3950 (2024), `arXiv:2305.02591` —
/// experimental proof that pulse-tube compressor vibration produces
/// time-synchronised T1 oscillations in transmon qubits (ETH Zürich / Chalmers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoolingProfile {
    /// Physical compressor specification.
    pub compressor: CompressorSpec,
    /// H(f) transfer function: frequency-domain characterisation of
    /// vibration-to-decoherence coupling for this specific installation.
    ///
    /// Empty until the first `DecoherenceMonitor::compute_fingerprint` run.
    /// Manufacturing tolerances make this unique per installation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transfer_function: Vec<TransferFunctionSample>,
    /// Low-vibration scheduling windows within the compressor cycle.
    ///
    /// Populated by the decoherence monitor after a quiet-window scan.
    /// Used by the Alsvid scheduler to time circuit execution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quiet_windows: Vec<QuietWindow>,
    /// PUF enrollment record. `None` if this installation has not yet
    /// been enrolled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub puf_enrollment: Option<PufEnrollment>,
}

impl CoolingProfile {
    /// Create a new `CoolingProfile` with only compressor spec populated.
    ///
    /// Transfer function, quiet windows, and PUF enrollment are populated
    /// later by a `DecoherenceMonitor` implementation.
    pub fn new(compressor: CompressorSpec) -> Self {
        Self {
            compressor,
            transfer_function: vec![],
            quiet_windows: vec![],
            puf_enrollment: None,
        }
    }

    /// Returns `true` if this installation has been enrolled as a PUF.
    pub fn is_enrolled(&self) -> bool {
        self.puf_enrollment.is_some()
    }

    /// Dominant compressor cycle frequency in Hz.
    pub fn cycle_frequency_hz(&self) -> f64 {
        self.compressor.cycle_frequency_hz
    }

    /// Returns the best quiet window (highest T1 improvement factor), if any.
    pub fn best_quiet_window(&self) -> Option<&QuietWindow> {
        self.quiet_windows.iter().max_by(|a, b| {
            let fa = a.t1_improvement_factor.unwrap_or(1.0);
            let fb = b.t1_improvement_factor.unwrap_or(1.0);
            fa.partial_cmp(&fb).unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}

/// Decoherence monitor trait for Alsvid physical-layer attestation.
///
/// Implemented by backends that support real-time T1/T2* measurement
/// synchronised with the cryogenic compressor cycle. Provides the
/// measurement primitives required to:
/// - Characterise the H(f) vibration-to-decoherence transfer function
/// - Identify quiet windows for scheduler hints
/// - Compute and verify the QPU-PUF fingerprint
///
/// # Photonic Backends (HAL Contract v2.3)
///
/// For photonic backends, `measure_t1` and `measure_t2_star` are not
/// applicable (return `None`). Use `measure_hom_visibility` and
/// `compute_hom_fingerprint` instead, which operate on HOM visibility
/// rather than T1/T2* relaxation times.
///
/// # Contract
///
/// Implementations MUST:
/// - Return `None` for all methods if the backend is a simulator.
/// - Return `None` if the backend does not expose a compressor sync signal.
/// - Use at least `shots` measurement repetitions for statistical stability.
/// - Not expose raw qubit indices in `compute_fingerprint` output.
/// - Return `None` from `measure_t1`/`measure_t2_star` on photonic backends.
pub trait DecoherenceMonitor {
    /// Measure the current T1 relaxation time (microseconds), averaged over
    /// the specified qubit indices.
    ///
    /// Returns `None` if the measurement is not supported by this backend
    /// (e.g., simulator, no inversion-recovery pulse sequence available, or
    /// photonic backend — use `measure_hom_visibility` instead).
    fn measure_t1(&self, qubit_indices: &[u32], shots: u32) -> Option<f64>;

    /// Measure the current T2* dephasing time (microseconds, Ramsey sequence),
    /// averaged over the specified qubit indices.
    ///
    /// Returns `None` if the measurement is not supported by this backend.
    /// Photonic backends should return `None` (use `measure_hom_visibility`).
    fn measure_t2_star(&self, qubit_indices: &[u32], shots: u32) -> Option<f64>;

    /// Compute a vibration fingerprint over one full compressor cycle.
    ///
    /// Samples T1 at `sample_count` evenly-spaced phase offsets within the
    /// compressor cycle period, using `shots_per_sample` repetitions per point.
    /// The resulting T1 modulation vector is quantised to 8-bit bins and
    /// hashed (SHA-256) to produce the fingerprint.
    ///
    /// The hash is stable across measurements on the same physical installation
    /// (Hamming distance < `intra_distance_threshold`) but unique across
    /// installations due to manufacturing tolerances in compressor and
    /// cryostat mounting (inter-distance >> intra-distance at 3σ).
    ///
    /// Returns `None` if the backend does not expose a compressor sync signal,
    /// or if T1 measurement is not supported. Photonic backends should return
    /// `None` (use `compute_hom_fingerprint` instead).
    fn compute_fingerprint(&self, sample_count: u32, shots_per_sample: u32) -> Option<String>;

    /// Measure HOM (Hong-Ou-Mandel) visibility for photonic backends.
    ///
    /// Submits a 2-photon 50:50 beamsplitter circuit and measures coincidence
    /// rate. HOM visibility V = 1 - 2·P_coinc, where P_coinc is the probability
    /// of both photons exiting the same output mode.
    ///
    /// Returns `None` for non-photonic backends (use `measure_t1` instead).
    fn measure_hom_visibility(&self, shots: u32) -> Option<f64> {
        let _ = shots;
        None
    }

    /// Compute a PUF fingerprint from HOM visibility modulation over one
    /// compressor cycle (photonic backends).
    ///
    /// Samples HOM visibility at `sample_count` evenly-spaced compressor
    /// phase offsets. The visibility modulation vector is quantised to 8-bit
    /// bins and hashed (SHA-256). Populates `TransferFunctionSample::visibility_modulation`.
    ///
    /// Returns `None` for non-photonic backends (use `compute_fingerprint` instead).
    fn compute_hom_fingerprint(
        &self,
        sample_count: u32,
        shots_per_sample: u32,
    ) -> Option<Vec<TransferFunctionSample>> {
        let _ = (sample_count, shots_per_sample);
        None
    }
}

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

    // ── CoolingProfile tests (HAL Contract v2.2) ──────────────────────────

    fn pws_pwg500() -> CompressorSpec {
        CompressorSpec {
            model: "Pressure Wave Systems PWG500".into(),
            cycle_frequency_hz: 1.1,
            stage_temperatures_k: vec![50.0, 4.0, 0.8, 0.1, 0.01],
            compressor_type: CompressorType::RotaryValve,
        }
    }

    #[test]
    fn test_cooling_profile_new() {
        let profile = CoolingProfile::new(pws_pwg500());
        assert_eq!(profile.compressor.model, "Pressure Wave Systems PWG500");
        assert!((profile.cycle_frequency_hz() - 1.1).abs() < f64::EPSILON);
        assert!(!profile.is_enrolled());
        assert!(profile.transfer_function.is_empty());
        assert!(profile.quiet_windows.is_empty());
        assert!(profile.best_quiet_window().is_none());
    }

    #[test]
    fn test_cooling_profile_quiet_windows() {
        let mut profile = CoolingProfile::new(pws_pwg500());
        profile.quiet_windows = vec![
            QuietWindow {
                cycle_offset: 0.1,
                cycle_fraction: 0.15,
                t1_improvement_factor: Some(1.12),
            },
            QuietWindow {
                cycle_offset: 0.6,
                cycle_fraction: 0.10,
                t1_improvement_factor: Some(1.08),
            },
        ];
        let best = profile.best_quiet_window().unwrap();
        assert!((best.t1_improvement_factor.unwrap() - 1.12).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quiet_window_duration() {
        let qw = QuietWindow {
            cycle_offset: 0.1,
            cycle_fraction: 0.15,
            t1_improvement_factor: Some(1.12),
        };
        // At 1.1 Hz, cycle period = 1/1.1 ≈ 0.909 s; 15% of that ≈ 0.136 s
        let dur = qw.duration_secs(1.1);
        assert!((dur - 0.15 / 1.1).abs() < 1e-9);
    }

    #[test]
    fn test_cooling_profile_puf_enrollment() {
        let enrollment = PufEnrollment {
            installation_id: "PWG500-SN1234-MUENCHEN".into(),
            enrolled_at: 1_740_000_000,
            fingerprint_hash: "a".repeat(64),
            enrollment_shots: 2000,
            intra_distance_threshold: Some(0.08),
        };
        let mut profile = CoolingProfile::new(pws_pwg500());
        assert!(!profile.is_enrolled());
        profile.puf_enrollment = Some(enrollment);
        assert!(profile.is_enrolled());
    }

    #[test]
    fn test_capabilities_with_cooling_profile() {
        let compressor = pws_pwg500();
        let profile = CoolingProfile::new(compressor);
        let caps = Capabilities::iqm("IQM Garnet", 20).with_cooling_profile(profile);
        assert!(caps.cooling_profile.is_some());
        let cp = caps.cooling_profile.unwrap();
        assert!((cp.cycle_frequency_hz() - 1.1).abs() < f64::EPSILON);
        assert!(cp.compressor.is_rotary_valve());
    }

    #[test]
    fn test_cooling_profile_serialise_round_trip() {
        let profile = CoolingProfile {
            compressor: pws_pwg500(),
            transfer_function: vec![
                TransferFunctionSample {
                    freq_hz: 1.1,
                    t1_modulation: 0.12,
                    visibility_modulation: None,
                },
                TransferFunctionSample {
                    freq_hz: 2.2,
                    t1_modulation: 0.04,
                    visibility_modulation: None,
                },
            ],
            quiet_windows: vec![QuietWindow {
                cycle_offset: 0.1,
                cycle_fraction: 0.15,
                t1_improvement_factor: Some(1.12),
            }],
            puf_enrollment: None,
        };
        let json = serde_json::to_string(&profile).expect("serialise");
        let decoded: CoolingProfile = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(decoded.transfer_function.len(), 2);
        assert!((decoded.transfer_function[0].t1_modulation - 0.12).abs() < 1e-9);
        assert_eq!(decoded.quiet_windows.len(), 1);
    }
}
