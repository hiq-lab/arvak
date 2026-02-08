//! Backend capabilities.

use arvak_ir::noise::NoiseProfile;
use serde::{Deserialize, Serialize};

/// Capabilities of a quantum backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    /// Name of the backend.
    pub name: String,
    /// Number of qubits available.
    pub num_qubits: u32,
    /// Supported gate set.
    pub gate_set: GateSet,
    /// Qubit topology.
    pub topology: Topology,
    /// Maximum number of shots per job.
    pub max_shots: u32,
    /// Whether this is a simulator.
    pub is_simulator: bool,
    /// Additional features supported.
    #[serde(default)]
    pub features: Vec<String>,
    /// Hardware noise profile.
    #[serde(default)]
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
            is_simulator: false,
            features: vec![],
            noise_profile: None,
        }
    }

    /// Create capabilities for IBM devices.
    pub fn ibm(name: impl Into<String>, num_qubits: u32) -> Self {
        Self {
            name: name.into(),
            num_qubits,
            gate_set: GateSet::ibm(),
            topology: Topology::linear(num_qubits), // Simplified
            max_shots: 100_000,
            is_simulator: false,
            features: vec!["dynamic_circuits".into()],
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
            is_simulator: false,
            features: vec!["shuttling".into(), "zoned".into()],
            noise_profile: None,
        }
    }

    /// Attach a noise profile to these capabilities.
    pub fn with_noise_profile(mut self, profile: NoiseProfile) -> Self {
        self.noise_profile = Some(profile);
        self
    }
}

/// Gate set supported by a backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateSet {
    /// Single-qubit gates.
    pub single_qubit: Vec<String>,
    /// Two-qubit gates.
    pub two_qubit: Vec<String>,
    /// Native gates (preferred for this backend).
    pub native: Vec<String>,
}

impl GateSet {
    /// Create IQM gate set.
    pub fn iqm() -> Self {
        Self {
            single_qubit: vec!["prx".into()],
            two_qubit: vec!["cz".into()],
            native: vec!["prx".into(), "cz".into()],
        }
    }

    /// Create IBM gate set.
    pub fn ibm() -> Self {
        Self {
            single_qubit: vec!["rz".into(), "sx".into(), "x".into(), "id".into()],
            two_qubit: vec!["cx".into()],
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
            native: vec![],
        }
    }

    /// Create a neutral-atom gate set.
    ///
    /// Native gates: Global RZ, Rydberg CZ/CCZ.
    pub fn neutral_atom() -> Self {
        Self {
            single_qubit: vec!["rz".into(), "rx".into(), "ry".into()],
            two_qubit: vec!["cz".into()],
            native: vec!["rz".into(), "rx".into(), "ry".into(), "cz".into()],
        }
    }

    /// Check if a gate is supported.
    pub fn contains(&self, gate: &str) -> bool {
        self.single_qubit.iter().any(|g| g == gate) || self.two_qubit.iter().any(|g| g == gate)
    }
}

/// Qubit topology.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topology {
    /// Kind of topology.
    pub kind: TopologyKind,
    /// Coupling edges (pairs of connected qubits).
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
    /// Custom topology.
    Custom,
    /// Neutral-atom topology with reconfigurable zones.
    NeutralAtom {
        /// Number of interaction zones.
        zones: u32,
    },
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
}
