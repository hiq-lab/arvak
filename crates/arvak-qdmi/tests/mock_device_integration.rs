// SPDX-License-Identifier: Apache-2.0
//! Integration tests using the compiled mock QDMI device.
//!
//! The build.rs compiles `examples/mock_device/mock_device.c` into
//! `libmock_qdmi_device.so` and exports its path via the
//! `MOCK_QDMI_DEVICE_PATH` env var.

use std::path::Path;
use arvak_qdmi::capabilities::DeviceCapabilities;
use arvak_qdmi::device_loader::QdmiDevice;
use arvak_qdmi::session::DeviceSession;

/// Path to the compiled mock device .so (set by build.rs).
fn mock_device_path() -> &'static str {
    env!("MOCK_QDMI_DEVICE_PATH")
}

fn load_mock() -> QdmiDevice {
    QdmiDevice::load(Path::new(mock_device_path()), "MOCK")
        .expect("failed to load mock QDMI device")
}

// ---------------------------------------------------------------------------
// Device loading
// ---------------------------------------------------------------------------

#[test]
fn test_load_mock_device() {
    let device = load_mock();
    assert_eq!(device.prefix(), "MOCK");
    assert!(!device.supports_jobs()); // mock device doesn't implement job interface
}

#[test]
fn test_load_nonexistent_device() {
    let result = QdmiDevice::load(Path::new("/nonexistent/libfoo.so"), "FOO");
    assert!(result.is_err());
}

#[test]
fn test_load_wrong_prefix() {
    // Library exists but the "WRONG" prefix won't resolve any symbols.
    let result = QdmiDevice::load(Path::new(mock_device_path()), "WRONG");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Session management
// ---------------------------------------------------------------------------

#[test]
fn test_open_session() {
    let device = load_mock();
    let session = DeviceSession::open(&device).expect("session open failed");
    // Session should have a valid (non-null) handle
    assert!(session.is_active());
    // Drop closes the session (RAII)
}

#[test]
fn test_session_drop_is_safe() {
    let device = load_mock();
    {
        let _session = DeviceSession::open(&device).expect("session open failed");
        // session drops here
    }
    // Opening a second session should work fine after the first was closed.
    let _session2 = DeviceSession::open(&device).expect("second session open failed");
}

// ---------------------------------------------------------------------------
// Device-level property queries
// ---------------------------------------------------------------------------

#[test]
fn test_query_device_name() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let name = session
        .query_device_string(arvak_qdmi::ffi::QDMI_DEVICE_PROPERTY_NAME)
        .unwrap();
    assert_eq!(name, "Arvak Mock Device (5Q Linear)");
}

#[test]
fn test_query_device_version() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let version = session
        .query_device_string(arvak_qdmi::ffi::QDMI_DEVICE_PROPERTY_VERSION)
        .unwrap();
    assert_eq!(version, "0.1.0");
}

#[test]
fn test_query_num_qubits() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let n = session
        .query_device_usize(arvak_qdmi::ffi::QDMI_DEVICE_PROPERTY_QUBITSNUM)
        .unwrap();
    assert_eq!(n, 5);
}

#[test]
fn test_query_unsupported_property() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    // Property 999 doesn't exist in the mock device.
    let result = session.raw_query_device_property(999);
    assert!(matches!(result, Err(arvak_qdmi::QdmiError::NotSupported)));
}

// ---------------------------------------------------------------------------
// Full capability query
// ---------------------------------------------------------------------------

#[test]
fn test_full_capability_query() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).expect("capability query failed");

    // Device-level
    assert_eq!(caps.name, "Arvak Mock Device (5Q Linear)");
    assert_eq!(caps.version.as_deref(), Some("0.1.0"));
    assert_eq!(caps.num_qubits, 5);

    // Sites
    assert_eq!(caps.sites.len(), 5);

    // Coupling map: 5-qubit linear → 8 directed edges (4 bidirectional)
    assert_eq!(caps.coupling_map.num_edges(), 8);

    // Operations
    assert_eq!(caps.operations.len(), 3);
}

// ---------------------------------------------------------------------------
// Coupling map
// ---------------------------------------------------------------------------

#[test]
fn test_coupling_map_topology() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).unwrap();

    let cm = &caps.coupling_map;

    // Linear topology: each qubit connects to its neighbour(s)
    let sites = &caps.sites;

    // site[0] ↔ site[1]
    assert!(cm.is_connected(sites[0], sites[1]));
    assert!(cm.is_connected(sites[1], sites[0]));

    // site[0] is NOT directly connected to site[2]
    assert!(!cm.is_connected(sites[0], sites[2]));

    // Distance 0→4 should be 4 (linear chain)
    assert_eq!(cm.distance(sites[0], sites[4]), Some(4));

    // Neighbours of middle qubit (site[2]) should be [site[1], site[3]]
    let nbrs = cm.neighbors(sites[2]);
    assert_eq!(nbrs.len(), 2);
    assert!(nbrs.contains(&sites[1]));
    assert!(nbrs.contains(&sites[3]));
}

// ---------------------------------------------------------------------------
// Per-site properties
// ---------------------------------------------------------------------------

#[test]
fn test_site_properties() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).unwrap();

    // Check first qubit's properties
    let site0 = caps.sites[0];
    let props = caps.site_properties.get(&site0).expect("site0 properties missing");

    // T1 = 100 μs
    let t1 = props.t1.expect("T1 missing for site 0");
    assert!(
        (t1.as_secs_f64() - 100e-6).abs() < 1e-10,
        "T1 = {:?}, expected ~100μs",
        t1
    );

    // T2 = 50 μs
    let t2 = props.t2.expect("T2 missing for site 0");
    assert!(
        (t2.as_secs_f64() - 50e-6).abs() < 1e-10,
        "T2 = {:?}, expected ~50μs",
        t2
    );

    // Readout error = 0.02
    let re = props.readout_error.expect("readout error missing for site 0");
    assert!((re - 0.02).abs() < 1e-10, "readout error = {re}");

    // Frequency = 5.1 GHz
    let freq = props.frequency.expect("frequency missing for site 0");
    assert!((freq - 5.1e9).abs() < 1.0, "frequency = {freq}");
}

#[test]
fn test_all_sites_have_properties() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).unwrap();

    for site in &caps.sites {
        let props = caps.site_properties.get(site);
        assert!(props.is_some(), "missing properties for site {:?}", site);

        let props = props.unwrap();
        assert!(props.t1.is_some(), "missing T1 for {:?}", site);
        assert!(props.t2.is_some(), "missing T2 for {:?}", site);
        assert!(props.readout_error.is_some(), "missing readout_error for {:?}", site);

        // T1 should always be >= T2
        let t1 = props.t1.unwrap();
        let t2 = props.t2.unwrap();
        assert!(t1 >= t2, "T1 ({:?}) < T2 ({:?}) for {:?}", t1, t2, site);
    }
}

// ---------------------------------------------------------------------------
// Operation properties
// ---------------------------------------------------------------------------

#[test]
fn test_operation_properties() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).unwrap();

    // We should have 3 operations (H, CX, RZ)
    assert_eq!(caps.operations.len(), 3);

    // Check that we got properties for each operation
    for op in &caps.operations {
        let props = caps.operation_properties.get(op);
        assert!(props.is_some(), "missing properties for op {:?}", op);

        let props = props.unwrap();
        assert!(props.name.is_some(), "missing name for op {:?}", op);
        assert!(props.fidelity.is_some(), "missing fidelity for op {:?}", op);
        assert!(props.duration.is_some(), "missing duration for op {:?}", op);

        // Fidelity should be between 0 and 1
        let fid = props.fidelity.unwrap();
        assert!(
            (0.0..=1.0).contains(&fid),
            "fidelity {fid} out of range for op {:?}",
            op
        );
    }
}

#[test]
fn test_operation_names() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).unwrap();

    let names: Vec<String> = caps
        .operations
        .iter()
        .filter_map(|op| {
            caps.operation_properties
                .get(op)
                .and_then(|p| p.name.clone())
        })
        .collect();

    assert!(names.contains(&"h".to_string()), "missing H gate");
    assert!(names.contains(&"cx".to_string()), "missing CX gate");
    assert!(names.contains(&"rz".to_string()), "missing RZ gate");
}

#[test]
fn test_cx_gate_is_two_qubit() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).unwrap();

    // Find the CX operation
    let cx_op = caps.operations.iter().find(|op| {
        caps.operation_properties
            .get(op)
            .and_then(|p| p.name.as_deref())
            == Some("cx")
    });
    assert!(cx_op.is_some(), "CX gate not found");

    let cx_props = caps.operation_properties.get(cx_op.unwrap()).unwrap();
    assert_eq!(cx_props.num_qubits, Some(2));
}

// ---------------------------------------------------------------------------
// Coupling map graph algorithms
// ---------------------------------------------------------------------------

#[test]
fn test_coupling_map_diameter() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).unwrap();

    // 5-qubit linear chain has diameter 4
    assert_eq!(caps.coupling_map.diameter(), Some(4));
}

#[test]
fn test_coupling_map_distances_are_symmetric() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).unwrap();

    // In a bidirectional linear chain, distance(a,b) == distance(b,a)
    for &a in &caps.sites {
        for &b in &caps.sites {
            let d_ab = caps.coupling_map.distance(a, b);
            let d_ba = caps.coupling_map.distance(b, a);
            assert_eq!(
                d_ab, d_ba,
                "asymmetric distance between {:?} and {:?}: {:?} vs {:?}",
                a, b, d_ab, d_ba
            );
        }
    }
}
