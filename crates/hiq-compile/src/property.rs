//! PropertySet and related types for pass communication.

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::any::{Any, TypeId};

use hiq_ir::QubitId;

/// A mapping from logical qubits to physical qubits.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Layout {
    /// Map from logical qubit to physical qubit index.
    logical_to_physical: FxHashMap<QubitId, u32>,
    /// Map from physical qubit index to logical qubit.
    physical_to_logical: FxHashMap<u32, QubitId>,
}

impl Layout {
    /// Create a new empty layout.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a trivial layout (logical qubit i -> physical qubit i).
    pub fn trivial(num_qubits: u32) -> Self {
        let mut layout = Self::new();
        for i in 0..num_qubits {
            layout.add(QubitId(i), i);
        }
        layout
    }

    /// Add a mapping from logical to physical qubit.
    pub fn add(&mut self, logical: QubitId, physical: u32) {
        self.logical_to_physical.insert(logical, physical);
        self.physical_to_logical.insert(physical, logical);
    }

    /// Get the physical qubit for a logical qubit.
    pub fn get_physical(&self, logical: QubitId) -> Option<u32> {
        self.logical_to_physical.get(&logical).copied()
    }

    /// Get the logical qubit for a physical qubit.
    pub fn get_logical(&self, physical: u32) -> Option<QubitId> {
        self.physical_to_logical.get(&physical).copied()
    }

    /// Swap two physical qubits in the layout.
    pub fn swap(&mut self, p1: u32, p2: u32) {
        let l1 = self.physical_to_logical.get(&p1).copied();
        let l2 = self.physical_to_logical.get(&p2).copied();

        if let Some(l1) = l1 {
            self.logical_to_physical.insert(l1, p2);
            self.physical_to_logical.insert(p2, l1);
        } else {
            self.physical_to_logical.remove(&p2);
        }

        if let Some(l2) = l2 {
            self.logical_to_physical.insert(l2, p1);
            self.physical_to_logical.insert(p1, l2);
        } else {
            self.physical_to_logical.remove(&p1);
        }
    }

    /// Get the number of mapped qubits.
    pub fn len(&self) -> usize {
        self.logical_to_physical.len()
    }

    /// Check if the layout is empty.
    pub fn is_empty(&self) -> bool {
        self.logical_to_physical.is_empty()
    }

    /// Iterate over (logical, physical) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (QubitId, u32)> + '_ {
        self.logical_to_physical.iter().map(|(&l, &p)| (l, p))
    }
}

/// Target device coupling map.
///
/// The coupling map defines which pairs of physical qubits can
/// interact with two-qubit gates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouplingMap {
    /// List of connected qubit pairs (bidirectional).
    edges: Vec<(u32, u32)>,
    /// Number of physical qubits.
    num_qubits: u32,
    /// Adjacency list for fast lookup.
    #[serde(skip)]
    adjacency: FxHashMap<u32, Vec<u32>>,
}

impl CouplingMap {
    /// Create a new coupling map with the given number of qubits.
    pub fn new(num_qubits: u32) -> Self {
        Self {
            edges: vec![],
            num_qubits,
            adjacency: FxHashMap::default(),
        }
    }

    /// Add an edge between two qubits (bidirectional).
    pub fn add_edge(&mut self, q1: u32, q2: u32) {
        self.edges.push((q1, q2));
        self.adjacency.entry(q1).or_default().push(q2);
        self.adjacency.entry(q2).or_default().push(q1);
    }

    /// Check if two qubits are directly connected.
    pub fn is_connected(&self, q1: u32, q2: u32) -> bool {
        self.adjacency
            .get(&q1)
            .is_some_and(|neighbors| neighbors.contains(&q2))
    }

    /// Get the number of physical qubits.
    pub fn num_qubits(&self) -> u32 {
        self.num_qubits
    }

    /// Get the coupling edges.
    pub fn edges(&self) -> &[(u32, u32)] {
        &self.edges
    }

    /// Get neighbors of a qubit.
    pub fn neighbors(&self, qubit: u32) -> impl Iterator<Item = u32> + '_ {
        self.adjacency
            .get(&qubit)
            .map(|v| v.iter().copied())
            .into_iter()
            .flatten()
    }

    /// Create a linear coupling map (0-1-2-3-...).
    pub fn linear(n: u32) -> Self {
        let mut map = Self::new(n);
        for i in 0..n.saturating_sub(1) {
            map.add_edge(i, i + 1);
        }
        map
    }

    /// Create a fully connected coupling map.
    pub fn full(n: u32) -> Self {
        let mut map = Self::new(n);
        for i in 0..n {
            for j in (i + 1)..n {
                map.add_edge(i, j);
            }
        }
        map
    }

    /// Create a star topology (center qubit connected to all others).
    pub fn star(n: u32) -> Self {
        let mut map = Self::new(n);
        for i in 1..n {
            map.add_edge(0, i);
        }
        map
    }

    /// Calculate shortest path distance between two qubits.
    pub fn distance(&self, from: u32, to: u32) -> Option<u32> {
        if from == to {
            return Some(0);
        }

        // BFS
        let mut visited = FxHashMap::default();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back((from, 0u32));
        visited.insert(from, 0u32);

        while let Some((current, dist)) = queue.pop_front() {
            for &neighbor in self.adjacency.get(&current).into_iter().flatten() {
                if neighbor == to {
                    return Some(dist + 1);
                }
                if let std::collections::hash_map::Entry::Vacant(e) = visited.entry(neighbor) {
                    e.insert(dist + 1);
                    queue.push_back((neighbor, dist + 1));
                }
            }
        }

        None
    }
}

/// Basis gates for the target device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasisGates {
    /// List of gate names in the basis.
    gates: Vec<String>,
}

impl BasisGates {
    /// Create a new basis gates set.
    pub fn new(gates: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            gates: gates.into_iter().map(|g| g.into()).collect(),
        }
    }

    /// Check if a gate is in the basis.
    pub fn contains(&self, gate: &str) -> bool {
        self.gates.iter().any(|g| g == gate)
    }

    /// Get the basis gates.
    pub fn gates(&self) -> &[String] {
        &self.gates
    }

    /// Create IQM basis gates (PRX + CZ).
    pub fn iqm() -> Self {
        Self::new(["prx", "cz", "measure", "barrier"])
    }

    /// Create IBM basis gates (RZ + SX + X + CX).
    pub fn ibm() -> Self {
        Self::new(["rz", "sx", "x", "cx", "measure", "barrier", "id"])
    }

    /// Create a universal basis (all standard gates).
    pub fn universal() -> Self {
        Self::new([
            "id", "x", "y", "z", "h", "s", "sdg", "t", "tdg", "sx", "sxdg", "rx", "ry", "rz", "p",
            "u", "cx", "cy", "cz", "ch", "swap", "iswap", "crx", "cry", "crz", "cp", "rxx", "ryy",
            "rzz", "ccx", "cswap", "prx", "measure", "reset", "barrier",
        ])
    }
}

/// Properties shared between compilation passes.
///
/// The PropertySet allows passes to communicate by storing
/// and retrieving typed values. Standard properties like
/// layout, coupling map, and basis gates have dedicated fields.
#[derive(Debug, Default)]
pub struct PropertySet {
    /// Qubit layout mapping.
    pub layout: Option<Layout>,
    /// Target coupling map.
    pub coupling_map: Option<CouplingMap>,
    /// Target basis gates.
    pub basis_gates: Option<BasisGates>,
    /// Custom properties.
    custom: FxHashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl PropertySet {
    /// Create a new empty property set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a property set with target configuration.
    pub fn with_target(mut self, coupling_map: CouplingMap, basis_gates: BasisGates) -> Self {
        self.coupling_map = Some(coupling_map);
        self.basis_gates = Some(basis_gates);
        self
    }

    /// Set the layout.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = Some(layout);
        self
    }

    /// Insert a custom property.
    pub fn insert<T: Any + Send + Sync>(&mut self, value: T) {
        self.custom.insert(TypeId::of::<T>(), Box::new(value));
    }

    /// Get a custom property.
    pub fn get<T: Any>(&self) -> Option<&T> {
        self.custom
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref())
    }

    /// Get a mutable custom property.
    pub fn get_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.custom
            .get_mut(&TypeId::of::<T>())
            .and_then(|v| v.downcast_mut())
    }

    /// Remove a custom property.
    pub fn remove<T: Any>(&mut self) -> Option<T> {
        self.custom
            .remove(&TypeId::of::<T>())
            .and_then(|v| v.downcast().ok())
            .map(|v| *v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hiq_ir::QubitId;

    #[test]
    fn test_layout_trivial() {
        let layout = Layout::trivial(5);
        assert_eq!(layout.get_physical(QubitId(0)), Some(0));
        assert_eq!(layout.get_physical(QubitId(4)), Some(4));
        assert_eq!(layout.get_logical(2), Some(QubitId(2)));
    }

    #[test]
    fn test_layout_swap() {
        let mut layout = Layout::trivial(3);
        layout.swap(0, 2);

        assert_eq!(layout.get_physical(QubitId(0)), Some(2));
        assert_eq!(layout.get_physical(QubitId(2)), Some(0));
        assert_eq!(layout.get_logical(0), Some(QubitId(2)));
        assert_eq!(layout.get_logical(2), Some(QubitId(0)));
    }

    #[test]
    fn test_coupling_map_linear() {
        let map = CouplingMap::linear(5);
        assert!(map.is_connected(0, 1));
        assert!(map.is_connected(1, 2));
        assert!(!map.is_connected(0, 2));
        assert_eq!(map.distance(0, 4), Some(4));
    }

    #[test]
    fn test_coupling_map_star() {
        let map = CouplingMap::star(5);
        assert!(map.is_connected(0, 1));
        assert!(map.is_connected(0, 4));
        assert!(!map.is_connected(1, 2));
        assert_eq!(map.distance(1, 2), Some(2));
    }

    #[test]
    fn test_basis_gates() {
        let iqm = BasisGates::iqm();
        assert!(iqm.contains("prx"));
        assert!(iqm.contains("cz"));
        assert!(!iqm.contains("cx"));

        let ibm = BasisGates::ibm();
        assert!(ibm.contains("cx"));
        assert!(ibm.contains("rz"));
        assert!(!ibm.contains("prx"));
    }

    #[test]
    fn test_property_set_custom() {
        let mut props = PropertySet::new();

        #[derive(Debug, PartialEq)]
        struct CustomData(i32);

        props.insert(CustomData(42));
        assert_eq!(props.get::<CustomData>(), Some(&CustomData(42)));

        let removed = props.remove::<CustomData>();
        assert_eq!(removed, Some(CustomData(42)));
        assert_eq!(props.get::<CustomData>(), None);
    }
}
