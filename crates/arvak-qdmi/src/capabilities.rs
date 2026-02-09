// SPDX-License-Identifier: Apache-2.0
//! Structured device capability model.
//!
//! Queries a QDMI device session for all available properties and assembles
//! them into a [`DeviceCapabilities`] struct that the Arvak compiler can
//! consume for topology-aware routing, noise-aware optimisation, and gate
//! decomposition.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use crate::error::{QdmiError, Result};
use crate::ffi;
use crate::format::CircuitFormat;
use crate::session::DeviceSession;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Opaque identifier for a qubit site on a device.
///
/// We store the raw pointer value as a `usize` so that `SiteId` is `Copy`,
/// `Hash`, etc. The actual pointer is only meaningful within a single session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SiteId(pub usize);

/// Opaque identifier for a gate operation on a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationId(pub usize);

/// Complete device capabilities extracted from QDMI.
#[derive(Debug, Clone)]
pub struct DeviceCapabilities {
    /// Human-readable device name.
    pub name: String,

    /// QDMI library version (if reported).
    pub version: Option<String>,

    /// Total number of qubits.
    pub num_qubits: usize,

    /// Ordered list of site (qubit) identifiers.
    pub sites: Vec<SiteId>,

    /// Qubit connectivity graph.
    pub coupling_map: CouplingMap,

    /// Available gate operations.
    pub operations: Vec<OperationId>,

    /// Per-qubit physical properties.
    pub site_properties: HashMap<SiteId, SiteProperties>,

    /// Per-gate properties, keyed by (operation, site-tuple).
    pub operation_properties: HashMap<OperationId, OperationProperties>,

    /// Circuit formats the device can accept.
    pub supported_formats: Vec<CircuitFormat>,
}

/// Per-qubit properties.
#[derive(Debug, Clone, Default)]
pub struct SiteProperties {
    /// T₁ relaxation time.
    pub t1: Option<Duration>,
    /// T₂ dephasing time.
    pub t2: Option<Duration>,
    /// Single-shot readout error rate (0.0 – 1.0).
    pub readout_error: Option<f64>,
    /// Readout duration.
    pub readout_duration: Option<Duration>,
    /// Qubit frequency (Hz).
    pub frequency: Option<f64>,
}

/// Per-gate properties.
#[derive(Debug, Clone, Default)]
pub struct OperationProperties {
    /// Gate name (e.g. "cx", "rz", "h").
    pub name: Option<String>,
    /// Gate execution time.
    pub duration: Option<Duration>,
    /// Gate fidelity (0.0 – 1.0).
    pub fidelity: Option<f64>,
    /// Number of qubits this gate acts on.
    pub num_qubits: Option<usize>,
}

// ---------------------------------------------------------------------------
// Coupling map
// ---------------------------------------------------------------------------

/// Sparse directed graph representing qubit connectivity.
#[derive(Debug, Clone, Default)]
pub struct CouplingMap {
    edges: Vec<(SiteId, SiteId)>,
    adjacency: HashMap<SiteId, Vec<SiteId>>,
}

impl CouplingMap {
    /// Build from QDMI-style flat pairs `[(a0,b0), (a1,b1), ...]`.
    pub fn from_pairs(pairs: Vec<(SiteId, SiteId)>) -> Self {
        let mut adjacency: HashMap<SiteId, Vec<SiteId>> = HashMap::new();
        for &(a, b) in &pairs {
            adjacency.entry(a).or_default().push(b);
        }
        Self {
            edges: pairs,
            adjacency,
        }
    }

    /// All directed edges.
    pub fn edges(&self) -> &[(SiteId, SiteId)] {
        &self.edges
    }

    /// Number of directed edges.
    pub fn num_edges(&self) -> usize {
        self.edges.len()
    }

    /// Whether a directed edge `a → b` exists.
    pub fn is_connected(&self, a: SiteId, b: SiteId) -> bool {
        self.adjacency
            .get(&a)
            .map_or(false, |nbrs| nbrs.contains(&b))
    }

    /// Neighbours reachable from `site` in one hop.
    pub fn neighbors(&self, site: SiteId) -> &[SiteId] {
        self.adjacency.get(&site).map_or(&[], |v| v.as_slice())
    }

    /// BFS shortest-path distance from `from` to `to`. Returns `None` if
    /// unreachable.
    pub fn distance(&self, from: SiteId, to: SiteId) -> Option<usize> {
        if from == to {
            return Some(0);
        }

        let mut visited: HashMap<SiteId, usize> = HashMap::new();
        let mut queue = VecDeque::new();

        visited.insert(from, 0);
        queue.push_back(from);

        while let Some(current) = queue.pop_front() {
            let d = visited[&current];
            for &nbr in self.neighbors(current) {
                if nbr == to {
                    return Some(d + 1);
                }
                if !visited.contains_key(&nbr) {
                    visited.insert(nbr, d + 1);
                    queue.push_back(nbr);
                }
            }
        }

        None
    }

    /// Diameter: the maximum shortest-path distance between any two connected
    /// sites.
    pub fn diameter(&self) -> Option<usize> {
        let sites: Vec<_> = self.adjacency.keys().copied().collect();
        let mut max_d = 0usize;
        for &a in &sites {
            for &b in &sites {
                if let Some(d) = self.distance(a, b) {
                    max_d = max_d.max(d);
                }
            }
        }
        if max_d == 0 && sites.len() > 1 {
            None // disconnected
        } else {
            Some(max_d)
        }
    }
}

// ---------------------------------------------------------------------------
// Capability query orchestrator
// ---------------------------------------------------------------------------

impl DeviceCapabilities {
    /// Query all available capabilities from a QDMI device session.
    ///
    /// Properties that the device does not support are silently set to `None`.
    /// Only truly fatal errors (e.g. session failure) propagate.
    pub fn query(session: &DeviceSession<'_>) -> Result<Self> {
        // -- Device-level properties -----------------------------------------

        let name = session
            .query_device_string(ffi::QDMI_DEVICE_PROPERTY_NAME)
            .unwrap_or_else(|_| "<unnamed>".into());

        let version = session
            .query_device_string(ffi::QDMI_DEVICE_PROPERTY_VERSION)
            .ok();

        let num_qubits = session
            .query_device_usize(ffi::QDMI_DEVICE_PROPERTY_QUBITSNUM)
            .unwrap_or(0);

        // -- Sites -----------------------------------------------------------

        let sites = query_sites(session)?;

        // -- Coupling map ----------------------------------------------------

        let coupling_map = query_coupling_map(session, &sites)?;

        // -- Operations ------------------------------------------------------

        let operations = query_operations(session)?;

        // -- Per-site properties ---------------------------------------------

        let mut site_properties = HashMap::new();
        for &site in &sites {
            let props = query_site_properties(session, site)?;
            site_properties.insert(site, props);
        }

        // -- Per-operation properties ----------------------------------------

        let mut operation_properties = HashMap::new();
        for &op in &operations {
            let props = query_operation_props(session, op)?;
            operation_properties.insert(op, props);
        }

        // -- Supported formats (best-effort) ---------------------------------

        let supported_formats = query_supported_formats(session);

        Ok(Self {
            name,
            version,
            num_qubits,
            sites,
            coupling_map,
            operations,
            site_properties,
            operation_properties,
            supported_formats,
        })
    }
}

// ---------------------------------------------------------------------------
// Internal query helpers
// ---------------------------------------------------------------------------

/// Retrieve the list of site handles from the device.
fn query_sites(session: &DeviceSession<'_>) -> Result<Vec<SiteId>> {
    let buf = match session.raw_query_device_property(ffi::QDMI_DEVICE_PROPERTY_SITES) {
        Ok(b) => b,
        Err(QdmiError::NotSupported) => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };

    let ptr_size = std::mem::size_of::<ffi::QdmiSite>();
    if buf.len() % ptr_size != 0 {
        return Err(QdmiError::ParseError(format!(
            "sites buffer length {} not a multiple of pointer size {}",
            buf.len(),
            ptr_size
        )));
    }

    let count = buf.len() / ptr_size;
    let sites: Vec<SiteId> = (0..count)
        .map(|i| {
            let offset = i * ptr_size;
            let ptr_bytes = &buf[offset..offset + ptr_size];
            let raw = usize::from_ne_bytes(ptr_bytes.try_into().unwrap());
            SiteId(raw)
        })
        .collect();

    log::debug!("queried {} sites from device", sites.len());
    Ok(sites)
}

/// Parse the coupling map from the device.
///
/// The QDMI coupling map is returned as a flat array of `QDMI_Site` pairs:
/// `[a0, b0, a1, b1, ...]` where each `(aᵢ, bᵢ)` is a directed edge.
fn query_coupling_map(
    session: &DeviceSession<'_>,
    _sites: &[SiteId],
) -> Result<CouplingMap> {
    let buf = match session.raw_query_device_property(ffi::QDMI_DEVICE_PROPERTY_COUPLINGMAP) {
        Ok(b) => b,
        Err(QdmiError::NotSupported) => {
            log::warn!("device does not report a coupling map");
            return Ok(CouplingMap::default());
        }
        Err(e) => return Err(e),
    };

    let ptr_size = std::mem::size_of::<ffi::QdmiSite>();
    let pair_size = ptr_size * 2;

    if buf.is_empty() {
        return Ok(CouplingMap::default());
    }

    if buf.len() % pair_size != 0 {
        return Err(QdmiError::ParseError(format!(
            "coupling map buffer length {} not a multiple of pair size {}",
            buf.len(),
            pair_size
        )));
    }

    let num_pairs = buf.len() / pair_size;
    let mut pairs = Vec::with_capacity(num_pairs);

    for i in 0..num_pairs {
        let offset = i * pair_size;
        let a_bytes = &buf[offset..offset + ptr_size];
        let b_bytes = &buf[offset + ptr_size..offset + pair_size];
        let a = SiteId(usize::from_ne_bytes(a_bytes.try_into().unwrap()));
        let b = SiteId(usize::from_ne_bytes(b_bytes.try_into().unwrap()));
        pairs.push((a, b));
    }

    log::debug!("queried coupling map with {} edges", pairs.len());
    Ok(CouplingMap::from_pairs(pairs))
}

/// Retrieve the list of operation handles.
fn query_operations(session: &DeviceSession<'_>) -> Result<Vec<OperationId>> {
    let buf = match session.raw_query_device_property(ffi::QDMI_DEVICE_PROPERTY_OPERATIONS) {
        Ok(b) => b,
        Err(QdmiError::NotSupported) => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };

    let ptr_size = std::mem::size_of::<ffi::QdmiOperation>();
    if buf.len() % ptr_size != 0 {
        return Err(QdmiError::ParseError(format!(
            "operations buffer not aligned to pointer size"
        )));
    }

    let count = buf.len() / ptr_size;
    let ops: Vec<OperationId> = (0..count)
        .map(|i| {
            let offset = i * ptr_size;
            let raw = usize::from_ne_bytes(buf[offset..offset + ptr_size].try_into().unwrap());
            OperationId(raw)
        })
        .collect();

    log::debug!("queried {} operations from device", ops.len());
    Ok(ops)
}

/// Query per-site physical properties, gracefully handling unsupported ones.
fn query_site_properties(session: &DeviceSession<'_>, site: SiteId) -> Result<SiteProperties> {
    let site_ptr = site.0 as ffi::QdmiSite;

    let t1 = session
        .query_site_f64_optional(site_ptr, ffi::QDMI_SITE_PROPERTY_T1)?
        .map(Duration::from_secs_f64);

    let t2 = session
        .query_site_f64_optional(site_ptr, ffi::QDMI_SITE_PROPERTY_T2)?
        .map(Duration::from_secs_f64);

    let readout_error =
        session.query_site_f64_optional(site_ptr, ffi::QDMI_SITE_PROPERTY_READOUTERROR)?;

    let readout_duration = session
        .query_site_f64_optional(site_ptr, ffi::QDMI_SITE_PROPERTY_READOUTDURATION)?
        .map(Duration::from_secs_f64);

    let frequency =
        session.query_site_f64_optional(site_ptr, ffi::QDMI_SITE_PROPERTY_FREQUENCY)?;

    Ok(SiteProperties {
        t1,
        t2,
        readout_error,
        readout_duration,
        frequency,
    })
}

/// Query per-operation properties.
fn query_operation_props(
    session: &DeviceSession<'_>,
    op: OperationId,
) -> Result<OperationProperties> {
    let op_ptr = op.0 as ffi::QdmiOperation;

    // Name (string property)
    let name = {
        match session.raw_query_operation_property(op_ptr, ffi::QDMI_OPERATION_PROPERTY_NAME) {
            Ok(buf) => std::ffi::CStr::from_bytes_until_nul(&buf)
                .ok()
                .and_then(|c| c.to_str().ok())
                .map(|s| s.to_string()),
            Err(_) => None,
        }
    };

    let duration = session
        .query_operation_f64_optional(op_ptr, ffi::QDMI_OPERATION_PROPERTY_DURATION)?
        .map(Duration::from_secs_f64);

    let fidelity =
        session.query_operation_f64_optional(op_ptr, ffi::QDMI_OPERATION_PROPERTY_FIDELITY)?;

    let num_qubits = {
        match session.raw_query_operation_property(op_ptr, ffi::QDMI_OPERATION_PROPERTY_QUBITSNUM)
        {
            Ok(buf) if buf.len() >= std::mem::size_of::<usize>() => {
                Some(usize::from_ne_bytes(
                    buf[..std::mem::size_of::<usize>()].try_into().unwrap(),
                ))
            }
            _ => None,
        }
    };

    Ok(OperationProperties {
        name,
        duration,
        fidelity,
        num_qubits,
    })
}

/// Best-effort query for supported circuit formats.
///
/// If the device doesn't advertise formats, we fall back to `[OpenQasm3]` with
/// a warning—this matches the legacy behaviour while being explicit about it.
fn query_supported_formats(_session: &DeviceSession<'_>) -> Vec<CircuitFormat> {
    // TODO: Once the QDMI spec stabilises a circuit-format property key, query
    // it here. For now, we try a common convention and fall back.
    //
    // Some devices may expose this through a session property or a
    // device-specific extension.

    log::debug!(
        "circuit format query not yet standardised in QDMI; \
         defaulting to OpenQASM 3 (update when spec provides a property key)"
    );

    vec![CircuitFormat::OpenQasm3]
}

// ---------------------------------------------------------------------------
// Unit tests for CouplingMap
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn linear_3q() -> CouplingMap {
        // 0 ↔ 1 ↔ 2
        CouplingMap::from_pairs(vec![
            (SiteId(0), SiteId(1)),
            (SiteId(1), SiteId(0)),
            (SiteId(1), SiteId(2)),
            (SiteId(2), SiteId(1)),
        ])
    }

    #[test]
    fn test_connectivity() {
        let cm = linear_3q();
        assert!(cm.is_connected(SiteId(0), SiteId(1)));
        assert!(cm.is_connected(SiteId(1), SiteId(0)));
        assert!(!cm.is_connected(SiteId(0), SiteId(2)));
    }

    #[test]
    fn test_neighbors() {
        let cm = linear_3q();
        let n = cm.neighbors(SiteId(1));
        assert_eq!(n.len(), 2);
        assert!(n.contains(&SiteId(0)));
        assert!(n.contains(&SiteId(2)));
    }

    #[test]
    fn test_distance() {
        let cm = linear_3q();
        assert_eq!(cm.distance(SiteId(0), SiteId(0)), Some(0));
        assert_eq!(cm.distance(SiteId(0), SiteId(1)), Some(1));
        assert_eq!(cm.distance(SiteId(0), SiteId(2)), Some(2));
    }

    #[test]
    fn test_distance_unreachable() {
        // Directed graph: 0 → 1 (no reverse)
        let cm = CouplingMap::from_pairs(vec![(SiteId(0), SiteId(1))]);
        assert_eq!(cm.distance(SiteId(0), SiteId(1)), Some(1));
        assert_eq!(cm.distance(SiteId(1), SiteId(0)), None);
    }

    #[test]
    fn test_diameter() {
        let cm = linear_3q();
        assert_eq!(cm.diameter(), Some(2));
    }

    #[test]
    fn test_empty_coupling_map() {
        let cm = CouplingMap::default();
        assert_eq!(cm.num_edges(), 0);
    }

    #[test]
    fn test_star_topology() {
        // 0 is the hub connected to 1,2,3
        let cm = CouplingMap::from_pairs(vec![
            (SiteId(0), SiteId(1)),
            (SiteId(1), SiteId(0)),
            (SiteId(0), SiteId(2)),
            (SiteId(2), SiteId(0)),
            (SiteId(0), SiteId(3)),
            (SiteId(3), SiteId(0)),
        ]);
        assert_eq!(cm.distance(SiteId(1), SiteId(2)), Some(2));
        assert_eq!(cm.distance(SiteId(1), SiteId(3)), Some(2));
        assert_eq!(cm.neighbors(SiteId(0)).len(), 3);
    }
}
