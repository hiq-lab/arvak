// SPDX-License-Identifier: Apache-2.0
//! Integration tests using the compiled mock QDMI v1.2.1 device.
//!
//! The build.rs compiles `examples/mock_device/mock_device.c` into
//! `libmock_qdmi_device.so` and exports its path via the
//! `MOCK_QDMI_DEVICE_PATH` env var.

use std::path::Path;

use arvak_qdmi::capabilities::DeviceCapabilities;
use arvak_qdmi::device_loader::QdmiDevice;
use arvak_qdmi::format::CircuitFormat;
use arvak_qdmi::session::DeviceSession;
use arvak_qdmi::{QdmiError, ffi};

/// Path to the compiled mock device .so (set by build.rs).
fn mock_device_path() -> &'static str {
    env!("MOCK_QDMI_DEVICE_PATH")
}

fn load_mock() -> QdmiDevice {
    QdmiDevice::load(Path::new(mock_device_path()), "MOCK")
        .expect("failed to load mock QDMI device")
}

// ---------------------------------------------------------------------------
// Device loading & lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_load_mock_device() {
    let device = load_mock();
    assert_eq!(device.prefix(), "MOCK");
    // The mock device now implements all 18 functions including jobs
    assert!(device.supports_jobs());
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

#[test]
fn test_device_initialize_finalize_via_load_drop() {
    // device_initialize is called during load(), device_finalize during Drop.
    // Verify the lifecycle works by loading and dropping twice.
    {
        let _device = load_mock();
        // device_initialize was called
    }
    // device_finalize was called on drop
    {
        let _device = load_mock();
        // A second load+init should succeed
    }
}

// ---------------------------------------------------------------------------
// Session management
// ---------------------------------------------------------------------------

#[test]
fn test_open_session() {
    let device = load_mock();
    let session = DeviceSession::open(&device).expect("session open failed");
    assert!(session.is_active());
}

#[test]
fn test_session_drop_is_safe() {
    let device = load_mock();
    {
        let _session = DeviceSession::open(&device).expect("session open failed");
        // session drops here (session_free called)
    }
    // Opening a second session should work fine after the first was freed.
    let _session2 = DeviceSession::open(&device).expect("second session open failed");
}

#[test]
fn test_session_with_parameters() {
    use std::collections::HashMap;

    let device = load_mock();

    let mut params = HashMap::new();
    params.insert(
        ffi::QDMI_DEVICE_SESSION_PARAMETER_TOKEN,
        b"my-test-token".to_vec(),
    );
    params.insert(
        ffi::QDMI_DEVICE_SESSION_PARAMETER_BASEURL,
        b"https://example.com".to_vec(),
    );

    let session =
        DeviceSession::open_with_params(&device, &params).expect("session with params failed");
    assert!(session.is_active());
}

// ---------------------------------------------------------------------------
// Device-level property queries
// ---------------------------------------------------------------------------

#[test]
fn test_query_device_name() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let name = session
        .query_device_string(ffi::QDMI_DEVICE_PROPERTY_NAME)
        .unwrap();
    assert_eq!(name, "Arvak Mock Device (5Q Linear)");
}

#[test]
fn test_query_device_version() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let version = session
        .query_device_string(ffi::QDMI_DEVICE_PROPERTY_VERSION)
        .unwrap();
    assert_eq!(version, "0.1.0");
}

#[test]
fn test_query_num_qubits() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let n = session
        .query_device_usize(ffi::QDMI_DEVICE_PROPERTY_QUBITSNUM)
        .unwrap();
    assert_eq!(n, 5);
}

#[test]
fn test_query_device_status() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let buf = session
        .raw_query_device_property(ffi::QDMI_DEVICE_PROPERTY_STATUS)
        .unwrap();
    assert!(buf.len() >= std::mem::size_of::<i32>());
    let status = i32::from_ne_bytes(buf[..4].try_into().unwrap());
    assert_eq!(status, ffi::QDMI_DEVICE_STATUS_IDLE);
}

#[test]
fn test_query_duration_scale_factor() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let sf = session
        .query_device_f64(ffi::QDMI_DEVICE_PROPERTY_DURATIONSCALEFACTOR)
        .unwrap();
    assert!(
        (sf - 1e-9).abs() < 1e-20,
        "duration scale factor = {sf}, expected 1e-9"
    );
}

#[test]
fn test_query_unsupported_property() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    // Property 999 doesn't exist in the mock device → QDMI_ERROR_NOTSUPPORTED (-9)
    let result = session.raw_query_device_property(999);
    assert!(matches!(result, Err(QdmiError::NotSupported)));
}

// ---------------------------------------------------------------------------
// Supported formats
// ---------------------------------------------------------------------------

#[test]
fn test_query_supported_formats() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).unwrap();

    // Mock device reports QASM2 + QASM3
    assert!(
        caps.supported_formats.contains(&CircuitFormat::OpenQasm2),
        "expected OpenQasm2 in {:?}",
        caps.supported_formats
    );
    assert!(
        caps.supported_formats.contains(&CircuitFormat::OpenQasm3),
        "expected OpenQasm3 in {:?}",
        caps.supported_formats
    );
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
    assert_eq!(caps.status, Some(ffi::QDMI_DEVICE_STATUS_IDLE));
    assert!((caps.duration_scale_factor - 1e-9).abs() < 1e-20);

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
    let props = caps
        .site_properties
        .get(&site0)
        .expect("site0 properties missing");

    // T1 = 100000 ns * 1e-9 = 100 μs
    let t1 = props.t1.expect("T1 missing for site 0");
    assert!(
        (t1.as_secs_f64() - 100e-6).abs() < 1e-10,
        "T1 = {t1:?}, expected ~100μs"
    );

    // T2 = 50000 ns * 1e-9 = 50 μs
    let t2 = props.t2.expect("T2 missing for site 0");
    assert!(
        (t2.as_secs_f64() - 50e-6).abs() < 1e-10,
        "T2 = {t2:?}, expected ~50μs"
    );
}

#[test]
fn test_site_index_property() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).unwrap();

    // Each site should have an index matching its position
    for (i, site) in caps.sites.iter().enumerate() {
        let props = caps.site_properties.get(site).unwrap();
        assert_eq!(
            props.index,
            Some(i),
            "site {} index mismatch: {:?}",
            i,
            props.index
        );
    }
}

#[test]
fn test_all_sites_have_properties() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).unwrap();

    for site in &caps.sites {
        let props = caps.site_properties.get(site);
        assert!(props.is_some(), "missing properties for site {site:?}");

        let props = props.unwrap();
        assert!(props.t1.is_some(), "missing T1 for {site:?}");
        assert!(props.t2.is_some(), "missing T2 for {site:?}");

        // Quantum decoherence: 1/T2 = 1/(2*T1) + 1/T_phi, so T2 <= 2*T1 always.
        let t1 = props.t1.unwrap();
        let t2 = props.t2.unwrap();
        assert!(
            t2 <= t1 * 2,
            "T2 ({t2:?}) > 2*T1 ({:?}) for {site:?} — violates decoherence bound",
            t1 * 2
        );
    }
}

#[test]
fn test_duration_scale_factor_applied() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).unwrap();

    // Verify that T1 values are in the microsecond range (not nanoseconds or seconds)
    // Raw T1 = 100000 (ns), scale = 1e-9, so physical = 100μs
    let site0 = caps.sites[0];
    let props = caps.site_properties.get(&site0).unwrap();
    let t1_secs = props.t1.unwrap().as_secs_f64();

    // Should be in range 1e-5 to 1e-3 (10μs to 1ms)
    assert!(
        t1_secs > 1e-5 && t1_secs < 1e-3,
        "T1 = {t1_secs}s not in microsecond range; scale factor may not be applied"
    );
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

    for op in &caps.operations {
        let props = caps.operation_properties.get(op);
        assert!(props.is_some(), "missing properties for op {op:?}");

        let props = props.unwrap();
        assert!(props.name.is_some(), "missing name for op {op:?}");
        assert!(props.fidelity.is_some(), "missing fidelity for op {op:?}");
        assert!(props.duration.is_some(), "missing duration for op {op:?}");

        // Fidelity should be between 0 and 1
        let fid = props.fidelity.unwrap();
        assert!(
            (0.0..=1.0).contains(&fid),
            "fidelity {fid} out of range for op {op:?}"
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

#[test]
fn test_rz_gate_has_one_parameter() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).unwrap();

    let rz_op = caps.operations.iter().find(|op| {
        caps.operation_properties
            .get(op)
            .and_then(|p| p.name.as_deref())
            == Some("rz")
    });
    assert!(rz_op.is_some(), "RZ gate not found");

    let rz_props = caps.operation_properties.get(rz_op.unwrap()).unwrap();
    assert_eq!(rz_props.num_parameters, Some(1));
    assert_eq!(rz_props.num_qubits, Some(1));
}

#[test]
fn test_operation_durations_scaled() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).unwrap();

    // H gate: raw = 30 ns, scale = 1e-9, physical = 30e-9 s = 30 ns
    let h_op = caps.operations.iter().find(|op| {
        caps.operation_properties
            .get(op)
            .and_then(|p| p.name.as_deref())
            == Some("h")
    });
    assert!(h_op.is_some(), "H gate not found");

    let h_dur = caps
        .operation_properties
        .get(h_op.unwrap())
        .unwrap()
        .duration
        .unwrap();
    let h_secs = h_dur.as_secs_f64();
    assert!(
        (h_secs - 30e-9).abs() < 1e-15,
        "H duration = {h_secs}s, expected ~30ns"
    );
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

    for &a in &caps.sites {
        for &b in &caps.sites {
            let d_ab = caps.coupling_map.distance(a, b);
            let d_ba = caps.coupling_map.distance(b, a);
            assert_eq!(
                d_ab, d_ba,
                "asymmetric distance between {a:?} and {b:?}: {d_ab:?} vs {d_ba:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Job lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_job_lifecycle() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();

    // Create job
    let job = session.create_job().expect("create_job failed");

    // Set program format (QASM2)
    let fmt = ffi::QDMI_PROGRAM_FORMAT_QASM2;
    job.set_parameter(
        ffi::QDMI_DEVICE_JOB_PARAMETER_PROGRAMFORMAT,
        &fmt.to_ne_bytes(),
    )
    .expect("set format failed");

    // Set program
    let program = b"OPENQASM 2.0;\nqreg q[2];\nh q[0];\ncx q[0],q[1];";
    job.set_parameter(ffi::QDMI_DEVICE_JOB_PARAMETER_PROGRAM, program)
        .expect("set program failed");

    // Set shots
    let shots: usize = 1024;
    job.set_parameter(
        ffi::QDMI_DEVICE_JOB_PARAMETER_SHOTSNUM,
        &shots.to_ne_bytes(),
    )
    .expect("set shots failed");

    // Submit
    job.submit().expect("submit failed");

    // Check status — mock immediately goes to DONE
    let status = job.check().expect("check failed");
    assert_eq!(
        status,
        ffi::QDMI_JOB_STATUS_DONE,
        "expected DONE, got {status}"
    );

    // Wait (should return immediately for mock)
    job.wait(5000).expect("wait failed");

    // Get results: histogram keys
    let hist_keys = job
        .get_results(ffi::QDMI_JOB_RESULT_HISTKEYS)
        .expect("get hist keys failed");
    assert!(!hist_keys.is_empty(), "hist keys should not be empty");

    // Get results: histogram values
    let hist_values = job
        .get_results(ffi::QDMI_JOB_RESULT_HISTVALUES)
        .expect("get hist values failed");
    assert!(!hist_values.is_empty(), "hist values should not be empty");

    // Parse the histogram values (2 x usize)
    let value_size = std::mem::size_of::<usize>();
    assert_eq!(hist_values.len(), 2 * value_size);
    let count0 = usize::from_ne_bytes(hist_values[..value_size].try_into().unwrap());
    let count1 = usize::from_ne_bytes(hist_values[value_size..2 * value_size].try_into().unwrap());
    assert_eq!(count0 + count1, 1024, "total counts should be 1024");

    // Job is freed when dropped
}

#[test]
fn test_job_wait_then_check() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();

    let job = session.create_job().unwrap();
    job.submit().unwrap();
    job.wait(0).unwrap(); // 0 = infinite wait (mock returns immediately)

    let status = job.check().unwrap();
    assert_eq!(status, ffi::QDMI_JOB_STATUS_DONE);
}

#[test]
fn test_unsupported_result_type() {
    let device = load_mock();
    let session = DeviceSession::open(&device).unwrap();

    let job = session.create_job().unwrap();
    job.submit().unwrap();

    // STATEVECTOR_DENSE is not supported by the mock
    let result = job.get_results(ffi::QDMI_JOB_RESULT_STATEVECTORDENSE);
    assert!(
        matches!(result, Err(QdmiError::NotSupported)),
        "expected NotSupported, got {result:?}"
    );
}
