// SPDX-License-Identifier: Apache-2.0
//! Integration tests against the real MQT Core DDSIM QDMI device.
//!
//! These tests only run when the `DDSIM_QDMI_DEVICE_PATH` environment
//! variable points to a compiled `libmqt-core-qdmi-ddsim-device.so`.
//!
//! In CI, this is built by the nightly `ddsim-compat` job using the `CMake`
//! wrapper at `ci/ddsim_shim/` (mqt-core v3.12.0 → QDMI 1.4; pass
//! `-DMQT_CORE_VERSION=v3.7.0` for a QDMI 1.3 device). The tests are
//! ABI-agnostic. Locally, you can build the `.so` yourself:
//!
//! ```bash
//! cmake -S crates/arvak-qdmi/ci/ddsim_shim -B build-ddsim \
//!       -G Ninja -DCMAKE_BUILD_TYPE=Release
//! cmake --build build-ddsim --target mqt-core-qdmi-ddsim-device
//! export DDSIM_QDMI_DEVICE_PATH=$(find build-ddsim -name 'libmqt-core-qdmi-ddsim-device.so')
//! cargo test -p arvak-qdmi --test ddsim_integration
//! ```

use std::path::Path;

use arvak_qdmi::capabilities::DeviceCapabilities;
use arvak_qdmi::device_loader::{QdmiAbi, QdmiDevice};
use arvak_qdmi::ffi;
use arvak_qdmi::format::CircuitFormat;
use arvak_qdmi::session::DeviceSession;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ddsim_device_path() -> Option<String> {
    std::env::var("DDSIM_QDMI_DEVICE_PATH").ok()
}

fn load_ddsim() -> Option<QdmiDevice> {
    let path = ddsim_device_path()?;
    Some(
        QdmiDevice::load(Path::new(&path), "MQT_DDSIM")
            .expect("failed to load MQT DDSIM QDMI device"),
    )
}

/// Skip the test gracefully when DDSIM is not available.
macro_rules! require_ddsim {
    () => {
        match load_ddsim() {
            Some(d) => d,
            None => {
                eprintln!("DDSIM_QDMI_DEVICE_PATH not set — skipping DDSIM integration test");
                return;
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Device loading & lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_ddsim_load() {
    let device = require_ddsim!();
    assert_eq!(device.prefix(), "MQT_DDSIM");
    assert!(
        device.supports_jobs(),
        "DDSIM should support the full job interface"
    );
}

#[test]
fn test_ddsim_device_debug() {
    let device = require_ddsim!();
    let debug = format!("{device:?}");
    assert!(debug.contains("MQT_DDSIM"), "debug = {debug}");
    assert!(debug.contains("supports_jobs: true"), "debug = {debug}");
}

// ---------------------------------------------------------------------------
// Session lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_ddsim_session_open_close() {
    let device = require_ddsim!();
    let session = DeviceSession::open(&device).expect("session open failed");
    assert!(session.is_active());
    // session is dropped here — RAII calls session_free
}

// ---------------------------------------------------------------------------
// Device property queries
// ---------------------------------------------------------------------------

#[test]
fn test_ddsim_device_name() {
    let device = require_ddsim!();
    let session = DeviceSession::open(&device).unwrap();
    let name = session
        .query_device_string(ffi::QDMI_DEVICE_PROPERTY_NAME)
        .unwrap();
    // MQT DDSIM reports its name as something containing "DDSIM"
    assert!(
        name.to_ascii_uppercase().contains("DDSIM"),
        "expected device name to contain 'DDSIM', got '{name}'"
    );
}

#[test]
fn test_ddsim_num_qubits() {
    let device = require_ddsim!();
    let session = DeviceSession::open(&device).unwrap();
    let n = session
        .query_device_usize(ffi::QDMI_DEVICE_PROPERTY_QUBITSNUM)
        .unwrap();
    // DDSIM supports many qubits (typically 128+ based on dd::Qubit max)
    assert!(n > 0, "expected non-zero qubits, got {n}");
    // Should be significantly more than a toy device
    assert!(n >= 10, "expected at least 10 qubits, got {n}");
}

#[test]
fn test_ddsim_version() {
    let device = require_ddsim!();
    let session = DeviceSession::open(&device).unwrap();
    // Version may or may not be reported — just verify no panic
    let _version = session
        .query_device_string(ffi::QDMI_DEVICE_PROPERTY_VERSION)
        .ok();
}

// ---------------------------------------------------------------------------
// Supported formats
// ---------------------------------------------------------------------------

#[test]
fn test_ddsim_supported_formats() {
    let device = require_ddsim!();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).expect("capability query failed");

    assert!(
        caps.supported_formats.contains(&CircuitFormat::OpenQasm2),
        "DDSIM should support OpenQASM 2; formats = {:?}",
        caps.supported_formats
    );
    assert!(
        caps.supported_formats.contains(&CircuitFormat::OpenQasm3),
        "DDSIM should support OpenQASM 3; formats = {:?}",
        caps.supported_formats
    );
}

// ---------------------------------------------------------------------------
// Full capability query
// ---------------------------------------------------------------------------

#[test]
fn test_ddsim_full_capabilities() {
    let device = require_ddsim!();
    let session = DeviceSession::open(&device).unwrap();
    let caps = DeviceCapabilities::query(&session).expect("capability query failed");

    // Basic device info
    assert!(
        caps.name.to_ascii_uppercase().contains("DDSIM"),
        "name = {}",
        caps.name
    );
    assert!(caps.num_qubits > 0, "num_qubits = {}", caps.num_qubits);

    // DDSIM should have many operations (57+ in the full gate set)
    assert!(
        caps.operations.len() >= 10,
        "expected at least 10 operations, got {}",
        caps.operations.len()
    );

    // Sites should be reported
    assert!(
        !caps.sites.is_empty(),
        "DDSIM should report sites (one per qubit)"
    );

    // Check for well-known gates in operation names
    let op_names: Vec<String> = caps
        .operations
        .iter()
        .filter_map(|op| {
            caps.operation_properties
                .get(op)
                .and_then(|p| p.name.clone())
        })
        .collect();

    // H gate should be present
    assert!(
        op_names.iter().any(|n| n == "h"),
        "H gate not found in operations: {op_names:?}"
    );

    // CX (CNOT) gate should be present
    assert!(
        op_names.iter().any(|n| n == "cx"),
        "CX gate not found in operations: {op_names:?}"
    );

    // DDSIM is a simulator — coupling map may be empty (all-to-all implied)
    // but should not cause an error
    let _ = caps.coupling_map.num_edges();
}

// ---------------------------------------------------------------------------
// Job submission: Bell state (QASM2)
// ---------------------------------------------------------------------------

#[test]
fn test_ddsim_bell_state_qasm2() {
    let device = require_ddsim!();
    let session = DeviceSession::open(&device).unwrap();

    let job = session.create_job().expect("create_job failed");

    // Bell state circuit in OpenQASM 2.0
    let program = b"OPENQASM 2.0;\n\
                    include \"qelib1.inc\";\n\
                    qreg q[2];\n\
                    creg c[2];\n\
                    h q[0];\n\
                    cx q[0],q[1];\n\
                    measure q -> c;\n";
    job.set_program(ffi::QDMI_PROGRAM_FORMAT_QASM2, program)
        .expect("set program failed");

    // 1024 shots
    let shots: usize = 1024;
    job.set_parameter(
        ffi::QDMI_DEVICE_JOB_PARAMETER_SHOTSNUM,
        &shots.to_ne_bytes(),
    )
    .expect("set shots failed");

    // Submit and wait
    job.submit().expect("submit failed");
    // DDSIM runs asynchronously; timeout is in seconds (not ms!)
    job.wait(30).expect("wait failed or timed out");

    // Verify job is done
    let status = job.check().expect("check failed");
    assert_eq!(
        status,
        ffi::QDMI_JOB_STATUS_DONE,
        "expected DONE ({}), got {}",
        ffi::QDMI_JOB_STATUS_DONE,
        status
    );

    // Retrieve histogram values (array of usize counts)
    let hist_values_buf = job
        .get_results(ffi::QDMI_JOB_RESULT_HISTVALUES)
        .expect("get histogram values failed");
    assert!(
        !hist_values_buf.is_empty(),
        "histogram values should not be empty"
    );

    // Parse the counts
    let value_size = std::mem::size_of::<usize>();
    assert_eq!(
        hist_values_buf.len() % value_size,
        0,
        "hist values buffer length {} not a multiple of usize size {}",
        hist_values_buf.len(),
        value_size
    );

    let num_entries = hist_values_buf.len() / value_size;
    let counts: Vec<usize> = (0..num_entries)
        .map(|i| {
            let start = i * value_size;
            usize::from_ne_bytes(
                hist_values_buf[start..start + value_size]
                    .try_into()
                    .unwrap(),
            )
        })
        .collect();

    // Total counts should equal number of shots
    let total: usize = counts.iter().sum();
    assert_eq!(
        total, 1024,
        "total counts should be 1024, got {total} (counts = {counts:?})"
    );

    // Bell state |Phi+> = (|00> + |11>) / sqrt(2)
    // We expect exactly 2 histogram entries (00 and 11)
    assert_eq!(
        num_entries, 2,
        "Bell state should produce 2 outcomes, got {num_entries} (counts = {counts:?})"
    );

    // Each outcome should have a significant number of counts
    for (i, &c) in counts.iter().enumerate() {
        assert!(
            c > 100,
            "entry {i} has only {c} counts; expected ~512 for Bell state"
        );
    }

    // Retrieve histogram keys to verify they're returned
    let hist_keys_buf = job
        .get_results(ffi::QDMI_JOB_RESULT_HISTKEYS)
        .expect("get histogram keys failed");
    assert!(
        !hist_keys_buf.is_empty(),
        "histogram keys should not be empty"
    );
}

// ---------------------------------------------------------------------------
// Job submission: Bell state (QASM3)
// ---------------------------------------------------------------------------

#[test]
fn test_ddsim_bell_state_qasm3() {
    let device = require_ddsim!();
    let session = DeviceSession::open(&device).unwrap();

    let job = session.create_job().expect("create_job failed");

    // Bell state in OpenQASM 3
    let program = b"OPENQASM 3;\n\
                    include \"stdgates.inc\";\n\
                    qubit[2] q;\n\
                    bit[2] c;\n\
                    h q[0];\n\
                    cx q[0], q[1];\n\
                    c = measure q;\n";
    job.set_program(ffi::QDMI_PROGRAM_FORMAT_QASM3, program)
        .expect("set program failed");

    let shots: usize = 512;
    job.set_parameter(
        ffi::QDMI_DEVICE_JOB_PARAMETER_SHOTSNUM,
        &shots.to_ne_bytes(),
    )
    .expect("set shots failed");

    job.submit().expect("submit failed");
    job.wait(30).expect("wait failed");

    let status = job.check().expect("check failed");
    assert_eq!(
        status,
        ffi::QDMI_JOB_STATUS_DONE,
        "expected DONE, got {status}"
    );

    // Verify total counts
    let hist_values_buf = job
        .get_results(ffi::QDMI_JOB_RESULT_HISTVALUES)
        .expect("get histogram values failed");
    let value_size = std::mem::size_of::<usize>();
    let num_entries = hist_values_buf.len() / value_size;
    let total: usize = (0..num_entries)
        .map(|i| {
            let start = i * value_size;
            usize::from_ne_bytes(
                hist_values_buf[start..start + value_size]
                    .try_into()
                    .unwrap(),
            )
        })
        .sum();

    assert_eq!(
        total, 512,
        "total counts for QASM3 submission should be 512, got {total}"
    );
    assert_eq!(
        num_entries, 2,
        "Bell state via QASM3 should produce 2 outcomes, got {num_entries}"
    );
}

// ---------------------------------------------------------------------------
// QDMI 1.4: ABI detection and program round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_ddsim_abi_detected() {
    let device = require_ddsim!();
    eprintln!("DDSIM device ABI: {}", device.abi());
    if let Ok(expected) = std::env::var("DDSIM_EXPECTED_ABI") {
        let expected = match expected.as_str() {
            "1.3" => QdmiAbi::V1_3,
            "1.4" => QdmiAbi::V1_4,
            other => panic!("DDSIM_EXPECTED_ABI must be 1.3 or 1.4, got {other}"),
        };
        assert_eq!(device.abi(), expected);
    }
}

#[test]
fn test_ddsim_v14_program_roundtrip() {
    let device = require_ddsim!();
    if device.abi() != QdmiAbi::V1_4 {
        eprintln!("DDSIM device is {} — skipping 1.4-only test", device.abi());
        return;
    }
    let session = DeviceSession::open(&device).unwrap();
    let job = session.create_job().unwrap();

    // No trailing NUL here: set_program must add exactly one for text formats.
    let program = b"OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[1];\ncreg c[1];\nx q[0];\nmeasure q -> c;\n";
    job.set_program(ffi::QDMI_PROGRAM_FORMAT_QASM2, program)
        .expect("set_programs failed");

    let stored = job.get_program(0).expect("get_program failed");
    assert_eq!(stored.last(), Some(&0), "text program must end with NUL");
    assert_eq!(&stored[..stored.len() - 1], &program[..]);

    let num = job
        .query_property(ffi::QDMI_DEVICE_JOB_PROPERTY_PROGRAMSNUM)
        .expect("PROGRAMSNUM query failed");
    assert_eq!(usize::from_ne_bytes(num[..8].try_into().unwrap()), 1);

    let shots: usize = 100;
    job.set_parameter(
        ffi::QDMI_DEVICE_JOB_PARAMETER_SHOTSNUM,
        &shots.to_ne_bytes(),
    )
    .unwrap();
    job.submit().unwrap();
    job.wait(30).unwrap();
    assert_eq!(job.check().unwrap(), ffi::QDMI_JOB_STATUS_DONE);

    // |1> deterministically: one histogram entry with all shots.
    let values = job
        .get_program_results(0, ffi::QDMI_JOB_RESULT_HISTVALUES)
        .unwrap();
    assert_eq!(values.len(), std::mem::size_of::<usize>());
    assert_eq!(usize::from_ne_bytes(values[..8].try_into().unwrap()), 100);
    let keys = job
        .get_program_results(0, ffi::QDMI_JOB_RESULT_HISTKEYS)
        .unwrap();
    eprintln!("HISTKEYS raw = {:?}", String::from_utf8_lossy(&keys));
    assert!(String::from_utf8_lossy(&keys).contains('1'));

    // Index 1 does not exist in a single-program job.
    let err = job
        .get_program_results(1, ffi::QDMI_JOB_RESULT_HISTVALUES)
        .unwrap_err();
    eprintln!("index 1 → {err:?}");
    assert!(matches!(
        err,
        arvak_qdmi::QdmiError::DeviceError(ffi::QDMI_ERROR_OUTOFRANGE)
    ));
}
