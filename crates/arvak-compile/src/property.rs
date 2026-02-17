//! `PropertySet` and related types for pass communication.
//!
//! This module provides the [`PropertySet`] type, which enables compilation passes
//! to share data with each other. It contains both standard properties (layout,
//! coupling map, basis gates) and supports arbitrary custom properties.
//!
//! # Overview
//!
//! During quantum circuit compilation, multiple passes need to share information:
//! - **Layout pass** determines which logical qubits map to which physical qubits
//! - **Routing pass** uses the coupling map to insert SWAP gates
//! - **Translation pass** uses basis gates to decompose unsupported gates
//!
//! The `PropertySet` acts as a shared context passed through all compilation passes.
//!
//! # Examples
//!
//! ## Basic usage with target configuration
//!
//! ```
//! use arvak_compile::{PropertySet, CouplingMap, BasisGates};
//!
//! // Create a property set for an IQM backend
//! let props = PropertySet::new()
//!     .with_target(
//!         CouplingMap::linear(5),  // 5-qubit linear chain
//!         BasisGates::iqm(),       // PRX + CZ native gates
//!     );
//!
//! assert!(props.coupling_map.is_some());
//! assert!(props.basis_gates.as_ref().unwrap().contains("prx"));
//! ```
//!
//! ## Using the `PassManager` with `PropertySet`
//!
//! ```
//! use arvak_compile::{PassManagerBuilder, CouplingMap, BasisGates};
//!
//! let (pass_manager, props) = PassManagerBuilder::new()
//!     .with_optimization_level(2)
//!     .with_target(CouplingMap::star(5), BasisGates::ibm())
//!     .build();
//!
//! // The pass manager is now configured with the target properties
//! assert!(!pass_manager.is_empty());
//! ```
//!
//! ## Custom properties for pass communication
//!
//! ```
//! use arvak_compile::PropertySet;
//!
//! // Define a custom property type
//! #[derive(Debug, Clone, PartialEq)]
//! struct OptimizationStats {
//!     gates_removed: usize,
//!     depth_reduction: usize,
//! }
//!
//! let mut props = PropertySet::new();
//!
//! // Insert custom property
//! props.insert(OptimizationStats {
//!     gates_removed: 15,
//!     depth_reduction: 3,
//! });
//!
//! // Retrieve it later
//! let stats = props.get::<OptimizationStats>().unwrap();
//! assert_eq!(stats.gates_removed, 15);
//! ```

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::any::{Any, TypeId};

use arvak_ir::QubitId;

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
    ///
    /// If the physical qubit is already mapped to a different logical qubit,
    /// the old mapping is removed first to keep both maps consistent.
    /// Similarly, if the logical qubit is already mapped to a different physical
    /// qubit, that old physical mapping is removed.
    pub fn add(&mut self, logical: QubitId, physical: u32) {
        // Remove conflicting physical → logical mapping if it exists.
        if let Some(&old_logical) = self.physical_to_logical.get(&physical) {
            if old_logical != logical {
                self.logical_to_physical.remove(&old_logical);
            }
        }
        // Remove conflicting logical → physical mapping if it exists.
        if let Some(&old_physical) = self.logical_to_physical.get(&logical) {
            if old_physical != physical {
                self.physical_to_logical.remove(&old_physical);
            }
        }
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
///
/// ## Performance
///
/// On construction, a distance matrix is precomputed using BFS from each
/// node. This enables O(1) `distance()` lookups and O(distance) path
/// reconstruction during routing, eliminating per-gate BFS.
///
/// ## Deserialization
///
/// After deserialization, call [`rebuild_caches()`](Self::rebuild_caches) to
/// recompute the adjacency list and distance/predecessor matrices (which are
/// skipped during serialization). Without this call, `distance()` will fall
/// back to per-query BFS, and `shortest_path()` will return `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouplingMap {
    /// List of connected qubit pairs (bidirectional).
    edges: Vec<(u32, u32)>,
    /// Number of physical qubits.
    num_qubits: u32,
    /// Adjacency list for fast lookup.
    #[serde(skip)]
    adjacency: FxHashMap<u32, Vec<u32>>,
    /// Precomputed all-pairs distance matrix. `dist_matrix[from][to]` is the
    /// shortest-path distance, or `u32::MAX` if unreachable.
    /// Computed lazily on first access or eagerly via `precompute_distances()`.
    #[serde(skip)]
    dist_matrix: Vec<Vec<u32>>,
    /// Precomputed predecessor matrix for shortest-path reconstruction.
    /// `pred_matrix[from][to]` is the next hop on the shortest path from→to.
    #[serde(skip)]
    pred_matrix: Vec<Vec<u32>>,
}

impl CouplingMap {
    /// Create a new coupling map with the given number of qubits.
    pub fn new(num_qubits: u32) -> Self {
        Self {
            edges: vec![],
            num_qubits,
            adjacency: FxHashMap::default(),
            dist_matrix: vec![],
            pred_matrix: vec![],
        }
    }

    /// Add an edge between two qubits (bidirectional).
    ///
    /// Duplicate edges (including reversed pairs) are silently ignored.
    pub fn add_edge(&mut self, q1: u32, q2: u32) {
        // Check for duplicates in either direction.
        if self
            .edges
            .iter()
            .any(|&(a, b)| (a == q1 && b == q2) || (a == q2 && b == q1))
        {
            return;
        }
        self.edges.push((q1, q2));
        self.adjacency.entry(q1).or_default().push(q2);
        self.adjacency.entry(q2).or_default().push(q1);
    }

    /// Precompute all-pairs shortest paths using BFS from each node.
    /// Called automatically by factory methods (linear, star, full, zoned).
    fn precompute_distances(&mut self) {
        let n = self.num_qubits as usize;
        self.dist_matrix = vec![vec![u32::MAX; n]; n];
        self.pred_matrix = vec![vec![u32::MAX; n]; n];

        for src in 0..n {
            self.dist_matrix[src][src] = 0;
            let mut queue = std::collections::VecDeque::new();
            queue.push_back(src as u32);

            while let Some(current) = queue.pop_front() {
                let cur = current as usize;
                for &neighbor in self.adjacency.get(&current).into_iter().flatten() {
                    let nb = neighbor as usize;
                    if self.dist_matrix[src][nb] == u32::MAX {
                        self.dist_matrix[src][nb] = self.dist_matrix[src][cur] + 1;
                        // Predecessor on path from src to nb is current
                        self.pred_matrix[src][nb] = current;
                        queue.push_back(neighbor);
                    }
                }
            }
        }
    }

    /// Rebuild the adjacency list and distance/predecessor matrices from the
    /// edge list. Must be called after deserialization to restore O(1) distance
    /// lookups and shortest-path reconstruction.
    pub fn rebuild_caches(&mut self) {
        self.adjacency.clear();
        for &(q1, q2) in &self.edges {
            self.adjacency.entry(q1).or_default().push(q2);
            self.adjacency.entry(q2).or_default().push(q1);
        }
        self.precompute_distances();
    }

    /// Check if two qubits are directly connected.
    #[inline]
    pub fn is_connected(&self, q1: u32, q2: u32) -> bool {
        self.adjacency
            .get(&q1)
            .is_some_and(|neighbors| neighbors.contains(&q2))
    }

    /// Get the number of physical qubits.
    #[inline]
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
        map.precompute_distances();
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
        map.precompute_distances();
        map
    }

    /// Create a star topology (center qubit connected to all others).
    pub fn star(n: u32) -> Self {
        let mut map = Self::new(n);
        for i in 1..n {
            map.add_edge(0, i);
        }
        map.precompute_distances();
        map
    }

    /// Create a zoned coupling map for neutral-atom devices.
    ///
    /// Qubits within each zone are fully connected; qubits across zones are not
    /// (they require shuttle operations).
    pub fn zoned(num_qubits: u32, zones: u32) -> Self {
        let mut map = Self::new(num_qubits);
        let qubits_per_zone = num_qubits / zones.max(1);

        for z in 0..zones {
            let start = z * qubits_per_zone;
            let end = if z == zones - 1 {
                num_qubits
            } else {
                start + qubits_per_zone
            };
            for i in start..end {
                for j in (i + 1)..end {
                    map.add_edge(i, j);
                }
            }
        }

        map.precompute_distances();
        map
    }

    /// O(1) shortest-path distance lookup using the precomputed matrix.
    /// Falls back to BFS if the matrix has not been precomputed.
    pub fn distance(&self, from: u32, to: u32) -> Option<u32> {
        if from == to {
            return Some(0);
        }

        let (f, t) = (from as usize, to as usize);
        if f < self.dist_matrix.len() && t < self.dist_matrix[f].len() {
            let d = self.dist_matrix[f][t];
            return if d == u32::MAX { None } else { Some(d) };
        }

        // Fallback BFS (for manually-constructed maps without precompute)
        self.distance_bfs(from, to)
    }

    /// Reconstruct shortest path from→to using the predecessor matrix.
    /// Returns `None` if no path exists.
    pub fn shortest_path(&self, from: u32, to: u32) -> Option<Vec<u32>> {
        if from == to {
            return Some(vec![from]);
        }

        let (f, t) = (from as usize, to as usize);
        if f >= self.pred_matrix.len() || t >= self.pred_matrix[f].len() {
            return None;
        }

        if self.dist_matrix[f][t] == u32::MAX {
            return None;
        }

        // Reconstruct from→to using predecessor chain
        let mut path = vec![to];
        let mut current = to;
        while current != from {
            let pred = self.pred_matrix[f][current as usize];
            if pred == u32::MAX {
                return None;
            }
            path.push(pred);
            current = pred;
        }
        path.reverse();
        Some(path)
    }

    /// BFS fallback for distance computation.
    fn distance_bfs(&self, from: u32, to: u32) -> Option<u32> {
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
            gates: gates.into_iter().map(std::convert::Into::into).collect(),
        }
    }

    /// Check if a gate is in the basis.
    ///
    /// Note: This uses linear search over the gate list. For large basis gate
    /// sets, consider using a `HashSet<String>` for O(1) lookups instead.
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

    /// Create IBM Heron basis gates (RZ + SX + X + CZ).
    pub fn heron() -> Self {
        Self::new([
            "rz", "sx", "x", "cz", "id", "rx", "rzz", "measure", "barrier",
        ])
    }

    /// Create neutral-atom basis gates (RZ + RX + RY + CZ + shuttle).
    pub fn neutral_atom() -> Self {
        Self::new(["rz", "rx", "ry", "cz", "measure", "barrier", "shuttle"])
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
/// The `PropertySet` allows passes to communicate by storing and retrieving
/// typed values. Standard properties like layout, coupling map, and basis
/// gates have dedicated public fields for convenience.
///
/// # Standard Properties
///
/// | Field | Type | Description |
/// |-------|------|-------------|
/// | `layout` | [`Layout`] | Logical-to-physical qubit mapping |
/// | `coupling_map` | [`CouplingMap`] | Device connectivity graph |
/// | `basis_gates` | [`BasisGates`] | Native gate set for the target |
///
/// # Custom Properties
///
/// Passes can store arbitrary data using the type-safe [`insert`](Self::insert)
/// and [`get`](Self::get) methods. Each type can have at most one value stored.
///
/// # Examples
///
/// ```
/// use arvak_compile::{PropertySet, CouplingMap, BasisGates, Layout};
/// use arvak_ir::QubitId;
///
/// let mut props = PropertySet::new();
///
/// // Set up target device
/// props.coupling_map = Some(CouplingMap::linear(5));
/// props.basis_gates = Some(BasisGates::iqm());
///
/// // Layout is typically set by the layout pass
/// props.layout = Some(Layout::trivial(5));
///
/// // Check connectivity
/// let cm = props.coupling_map.as_ref().unwrap();
/// assert!(cm.is_connected(0, 1));
/// assert!(!cm.is_connected(0, 2));
/// ```
#[derive(Debug, Default)]
pub struct PropertySet {
    /// Qubit layout mapping (logical → physical).
    ///
    /// Set by layout passes, used by routing and translation passes.
    pub layout: Option<Layout>,

    /// Target coupling map defining allowed two-qubit interactions.
    ///
    /// Should be set before running routing passes.
    pub coupling_map: Option<CouplingMap>,

    /// Target basis gates for gate decomposition.
    ///
    /// Should be set before running translation passes.
    pub basis_gates: Option<BasisGates>,

    /// Custom properties storage (type-erased).
    custom: FxHashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl PropertySet {
    /// Create a new empty property set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a property set with target configuration.
    ///
    /// # Example
    ///
    /// ```
    /// use arvak_compile::{PropertySet, CouplingMap, BasisGates};
    ///
    /// let props = PropertySet::new()
    ///     .with_target(CouplingMap::linear(5), BasisGates::ibm());
    ///
    /// assert!(props.coupling_map.is_some());
    /// assert!(props.basis_gates.is_some());
    /// ```
    #[must_use]
    pub fn with_target(mut self, coupling_map: CouplingMap, basis_gates: BasisGates) -> Self {
        self.coupling_map = Some(coupling_map);
        self.basis_gates = Some(basis_gates);
        self
    }

    /// Set the layout.
    ///
    /// # Example
    ///
    /// ```
    /// use arvak_compile::{PropertySet, Layout};
    ///
    /// let props = PropertySet::new()
    ///     .with_layout(Layout::trivial(3));
    ///
    /// assert!(props.layout.is_some());
    /// ```
    #[must_use]
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
    use arvak_ir::QubitId;

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
    #[allow(clippy::items_after_statements)]
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
