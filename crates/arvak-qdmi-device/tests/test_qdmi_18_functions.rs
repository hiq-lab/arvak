// SPDX-License-Identifier: Apache-2.0
//! Integration test for all 18 QDMI device interface functions.
//!
//! Connects to the live Arvak gRPC server and exercises every function
//! in the QDMI v1.2.1 device interface with the `ARVAK_` prefix.
//!
//! Requires: `ARVAK_QDMI_TEST_URL` env var (e.g. `http://87.106.219.154:50051`).
//! Skip with: `cargo test -p arvak-qdmi-device` (no env var → test skipped).

#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    clippy::let_and_return,
    clippy::manual_range_contains
)]

use std::ffi::{c_int, c_void};

use arvak_qdmi::ffi;
use arvak_qdmi_device::*;

/// Resolve the gRPC server URL, or skip the test.
fn server_url() -> String {
    std::env::var("ARVAK_QDMI_TEST_URL")
        .unwrap_or_else(|_| "http://87.106.219.154:50051".to_string())
}

/// Helper: two-phase query returning raw bytes.
unsafe fn query_bytes(f: impl Fn(usize, *mut c_void, *mut usize) -> c_int) -> Vec<u8> {
    let mut size: usize = 0;
    let ret = f(0, std::ptr::null_mut(), &raw mut size);
    assert_eq!(ret, ffi::QDMI_SUCCESS, "size probe failed");
    assert!(size > 0, "size probe returned 0");
    let mut buf = vec![0u8; size];
    let ret = f(size, buf.as_mut_ptr().cast(), &raw mut size);
    assert_eq!(ret, ffi::QDMI_SUCCESS, "data read failed");
    buf
}

/// Helper: two-phase query returning a nul-terminated string.
unsafe fn query_string(f: impl Fn(usize, *mut c_void, *mut usize) -> c_int) -> String {
    let buf = query_bytes(f);
    let s = std::ffi::CStr::from_bytes_until_nul(&buf)
        .expect("not nul-terminated")
        .to_string_lossy()
        .into_owned();
    s
}

/// Helper: two-phase query returning a native-endian usize.
unsafe fn query_usize(f: impl Fn(usize, *mut c_void, *mut usize) -> c_int) -> usize {
    let buf = query_bytes(f);
    assert!(buf.len() >= std::mem::size_of::<usize>());
    usize::from_ne_bytes(buf[..std::mem::size_of::<usize>()].try_into().unwrap())
}

/// Helper: two-phase query returning a native-endian c_int.
unsafe fn query_cint(f: impl Fn(usize, *mut c_void, *mut usize) -> c_int) -> c_int {
    let buf = query_bytes(f);
    assert!(buf.len() >= std::mem::size_of::<c_int>());
    c_int::from_ne_bytes(buf[..std::mem::size_of::<c_int>()].try_into().unwrap())
}

// All 18 functions tested in a single sequential test because they share
// global state (OnceLock) and must run in lifecycle order.
#[test]
fn test_all_18_qdmi_functions() {
    let url = server_url();
    eprintln!("QDMI integration test against {url}");

    unsafe {
        // ── Function 1: device_initialize ────────────────────────────
        eprintln!("[1/18] device_initialize");
        let ret = ARVAK_QDMI_device_initialize();
        assert_eq!(ret, ffi::QDMI_SUCCESS);

        // Double init should also succeed (idempotent per QDMI spec)
        let ret = ARVAK_QDMI_device_initialize();
        assert_eq!(ret, ffi::QDMI_SUCCESS);

        // ── Function 3: session_alloc ────────────────────────────────
        eprintln!("[3/18] session_alloc");
        let mut session: *mut c_void = std::ptr::null_mut();
        let ret = ARVAK_QDMI_device_session_alloc(&raw mut session);
        assert_eq!(ret, ffi::QDMI_SUCCESS);
        assert!(!session.is_null());

        // ── Function 4: session_set_parameter (BASEURL) ──────────────
        eprintln!("[4/18] session_set_parameter(BASEURL)");
        let url_bytes = url.as_bytes();
        let ret = ARVAK_QDMI_device_session_set_parameter(
            session,
            ffi::QDMI_DEVICE_SESSION_PARAMETER_BASEURL,
            url_bytes.len(),
            url_bytes.as_ptr().cast(),
        );
        assert_eq!(ret, ffi::QDMI_SUCCESS);

        // Also test TOKEN parameter (empty string is fine)
        eprintln!("       session_set_parameter(TOKEN)");
        let token = b"test-token\0";
        let ret = ARVAK_QDMI_device_session_set_parameter(
            session,
            ffi::QDMI_DEVICE_SESSION_PARAMETER_TOKEN,
            token.len(),
            token.as_ptr().cast(),
        );
        assert_eq!(ret, ffi::QDMI_SUCCESS);

        // Invalid parameter should fail
        let ret = ARVAK_QDMI_device_session_set_parameter(
            session,
            99, // invalid
            0,
            std::ptr::null(),
        );
        assert_eq!(ret, ffi::QDMI_ERROR_INVALIDARGUMENT);

        // ── Function 5: session_init ─────────────────────────────────
        eprintln!("[5/18] session_init (connects to gRPC server)");
        let ret = ARVAK_QDMI_device_session_init(session);
        assert_eq!(
            ret,
            ffi::QDMI_SUCCESS,
            "session_init failed — is the gRPC server running at {url}?"
        );

        // ── Function 7: query_device_property ────────────────────────
        eprintln!("[7/18] query_device_property");

        // NAME
        let name = query_string(|sz, val, szr| {
            ARVAK_QDMI_device_session_query_device_property(
                session,
                ffi::QDMI_DEVICE_PROPERTY_NAME,
                sz,
                val,
                szr,
            )
        });
        eprintln!("       NAME = {name}");
        assert_eq!(name, "arvak");

        // VERSION
        let version = query_string(|sz, val, szr| {
            ARVAK_QDMI_device_session_query_device_property(
                session,
                ffi::QDMI_DEVICE_PROPERTY_VERSION,
                sz,
                val,
                szr,
            )
        });
        eprintln!("       VERSION = {version}");
        assert!(!version.is_empty());

        // STATUS
        let status = query_cint(|sz, val, szr| {
            ARVAK_QDMI_device_session_query_device_property(
                session,
                ffi::QDMI_DEVICE_PROPERTY_STATUS,
                sz,
                val,
                szr,
            )
        });
        eprintln!(
            "       STATUS = {status} (IDLE={})",
            ffi::QDMI_DEVICE_STATUS_IDLE
        );
        assert_eq!(status, ffi::QDMI_DEVICE_STATUS_IDLE);

        // QUBITSNUM
        let num_qubits = query_usize(|sz, val, szr| {
            ARVAK_QDMI_device_session_query_device_property(
                session,
                ffi::QDMI_DEVICE_PROPERTY_QUBITSNUM,
                sz,
                val,
                szr,
            )
        });
        eprintln!("       QUBITSNUM = {num_qubits}");
        assert!(num_qubits > 0, "expected at least 1 qubit");

        // SITES (array of opaque handles)
        let sites_buf = query_bytes(|sz, val, szr| {
            ARVAK_QDMI_device_session_query_device_property(
                session,
                ffi::QDMI_DEVICE_PROPERTY_SITES,
                sz,
                val,
                szr,
            )
        });
        let num_sites = sites_buf.len() / std::mem::size_of::<*mut c_void>();
        eprintln!("       SITES = {num_sites} site handles");
        assert_eq!(num_sites, num_qubits);

        // Parse site handles
        let site_handles: Vec<*mut c_void> = (0..num_sites)
            .map(|i| {
                let offset = i * std::mem::size_of::<*mut c_void>();
                let bytes = &sites_buf[offset..offset + std::mem::size_of::<*mut c_void>()];
                usize::from_ne_bytes(bytes.try_into().unwrap()) as *mut c_void
            })
            .collect();

        // OPERATIONS (array of operation handles)
        let ops_buf = query_bytes(|sz, val, szr| {
            ARVAK_QDMI_device_session_query_device_property(
                session,
                ffi::QDMI_DEVICE_PROPERTY_OPERATIONS,
                sz,
                val,
                szr,
            )
        });
        let num_ops = ops_buf.len() / std::mem::size_of::<*mut c_void>();
        eprintln!("       OPERATIONS = {num_ops} operation handles");
        assert!(num_ops > 0, "expected at least 1 operation");

        let op_handles: Vec<*mut c_void> = (0..num_ops)
            .map(|i| {
                let offset = i * std::mem::size_of::<*mut c_void>();
                let bytes = &ops_buf[offset..offset + std::mem::size_of::<*mut c_void>()];
                usize::from_ne_bytes(bytes.try_into().unwrap()) as *mut c_void
            })
            .collect();

        // COUPLINGMAP
        let coupling_buf = query_bytes(|sz, val, szr| {
            ARVAK_QDMI_device_session_query_device_property(
                session,
                ffi::QDMI_DEVICE_PROPERTY_COUPLINGMAP,
                sz,
                val,
                szr,
            )
        });
        let num_edges = coupling_buf.len() / std::mem::size_of::<*mut c_void>() / 2;
        eprintln!("       COUPLINGMAP = {num_edges} edges");
        assert!(num_edges > 0, "expected at least 1 coupling edge");

        // SUPPORTEDPROGRAMFORMATS
        let formats_buf = query_bytes(|sz, val, szr| {
            ARVAK_QDMI_device_session_query_device_property(
                session,
                ffi::QDMI_DEVICE_PROPERTY_SUPPORTEDPROGRAMFORMATS,
                sz,
                val,
                szr,
            )
        });
        let format = c_int::from_ne_bytes(formats_buf[..4].try_into().unwrap());
        eprintln!(
            "       SUPPORTEDPROGRAMFORMATS = [{}] (QASM3={})",
            format,
            ffi::QDMI_PROGRAM_FORMAT_QASM3
        );
        assert_eq!(format, ffi::QDMI_PROGRAM_FORMAT_QASM3);

        // Unsupported property
        let mut sz_ret: usize = 0;
        let ret = ARVAK_QDMI_device_session_query_device_property(
            session,
            999,
            0,
            std::ptr::null_mut(),
            &raw mut sz_ret,
        );
        assert_eq!(ret, ffi::QDMI_ERROR_NOTSUPPORTED);

        // ── Function 8: query_site_property ──────────────────────────
        eprintln!("[8/18] query_site_property");
        let site0 = site_handles[0];

        // INDEX
        let idx = query_usize(|sz, val, szr| {
            ARVAK_QDMI_device_session_query_site_property(
                session,
                site0,
                ffi::QDMI_SITE_PROPERTY_INDEX,
                sz,
                val,
                szr,
            )
        });
        eprintln!("       site[0] INDEX = {idx}");
        assert_eq!(idx, 0);

        // NAME
        let site_name = query_string(|sz, val, szr| {
            ARVAK_QDMI_device_session_query_site_property(
                session,
                site0,
                ffi::QDMI_SITE_PROPERTY_NAME,
                sz,
                val,
                szr,
            )
        });
        eprintln!("       site[0] NAME = {site_name}");
        assert_eq!(site_name, "q0");

        // ── Function 9: query_operation_property ─────────────────────
        eprintln!("[9/18] query_operation_property");
        let op0 = op_handles[0];

        // NAME
        let op_name = query_string(|sz, val, szr| {
            ARVAK_QDMI_device_session_query_operation_property(
                session,
                op0,
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
                ffi::QDMI_OPERATION_PROPERTY_NAME,
                sz,
                val,
                szr,
            )
        });
        eprintln!("       op[0] NAME = {op_name}");
        assert!(!op_name.is_empty());

        // QUBITSNUM
        let op_qubits = query_usize(|sz, val, szr| {
            ARVAK_QDMI_device_session_query_operation_property(
                session,
                op0,
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
                ffi::QDMI_OPERATION_PROPERTY_QUBITSNUM,
                sz,
                val,
                szr,
            )
        });
        eprintln!("       op[0] QUBITSNUM = {op_qubits}");
        assert!(op_qubits >= 1 && op_qubits <= 3);

        // PARAMETERSNUM
        let op_params = query_usize(|sz, val, szr| {
            ARVAK_QDMI_device_session_query_operation_property(
                session,
                op0,
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
                ffi::QDMI_OPERATION_PROPERTY_PARAMETERSNUM,
                sz,
                val,
                szr,
            )
        });
        eprintln!("       op[0] PARAMETERSNUM = {op_params}");

        // Print all operations
        eprintln!("       All operations:");
        for (i, &handle) in op_handles.iter().enumerate() {
            let name = query_string(|sz, val, szr| {
                ARVAK_QDMI_device_session_query_operation_property(
                    session,
                    handle,
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    ffi::QDMI_OPERATION_PROPERTY_NAME,
                    sz,
                    val,
                    szr,
                )
            });
            let qubits = query_usize(|sz, val, szr| {
                ARVAK_QDMI_device_session_query_operation_property(
                    session,
                    handle,
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    ffi::QDMI_OPERATION_PROPERTY_QUBITSNUM,
                    sz,
                    val,
                    szr,
                )
            });
            let params = query_usize(|sz, val, szr| {
                ARVAK_QDMI_device_session_query_operation_property(
                    session,
                    handle,
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    ffi::QDMI_OPERATION_PROPERTY_PARAMETERSNUM,
                    sz,
                    val,
                    szr,
                )
            });
            eprintln!("         [{i:2}] {name:6} {qubits}q {params}p");
        }

        // ── Function 10: create_device_job ───────────────────────────
        eprintln!("[10/18] create_device_job");
        let mut job: *mut c_void = std::ptr::null_mut();
        let ret = ARVAK_QDMI_device_session_create_device_job(session, &raw mut job);
        assert_eq!(ret, ffi::QDMI_SUCCESS);
        assert!(!job.is_null());

        // ── Function 11: job_set_parameter ───────────────────────────
        eprintln!("[11/18] job_set_parameter");

        // PROGRAMFORMAT = QASM3
        let format = ffi::QDMI_PROGRAM_FORMAT_QASM3;
        let ret = ARVAK_QDMI_device_job_set_parameter(
            job,
            ffi::QDMI_DEVICE_JOB_PARAMETER_PROGRAMFORMAT,
            std::mem::size_of::<c_int>(),
            (&raw const format).cast(),
        );
        assert_eq!(ret, ffi::QDMI_SUCCESS);

        // PROGRAM = Bell state QASM3
        let qasm = b"OPENQASM 3.0;\ninclude \"stdgates.inc\";\nqubit[2] q;\nbit[2] c;\nh q[0];\ncx q[0], q[1];\nc = measure q;\n\0";
        let ret = ARVAK_QDMI_device_job_set_parameter(
            job,
            ffi::QDMI_DEVICE_JOB_PARAMETER_PROGRAM,
            qasm.len(),
            qasm.as_ptr().cast(),
        );
        assert_eq!(ret, ffi::QDMI_SUCCESS);

        // SHOTSNUM = 512
        let shots: c_int = 512;
        let ret = ARVAK_QDMI_device_job_set_parameter(
            job,
            ffi::QDMI_DEVICE_JOB_PARAMETER_SHOTSNUM,
            std::mem::size_of::<c_int>(),
            (&raw const shots).cast(),
        );
        assert_eq!(ret, ffi::QDMI_SUCCESS);

        // Invalid parameter
        let ret = ARVAK_QDMI_device_job_set_parameter(job, 99, 0, std::ptr::null());
        assert_eq!(ret, ffi::QDMI_ERROR_INVALIDARGUMENT);

        // ── Function 12: job_submit ──────────────────────────────────
        eprintln!("[12/18] job_submit");
        let ret = ARVAK_QDMI_device_job_submit(job);
        assert_eq!(ret, ffi::QDMI_SUCCESS);

        // ── Function 13: job_check ───────────────────────────────────
        eprintln!("[13/18] job_check");
        let mut status: c_int = -1;
        let ret = ARVAK_QDMI_device_job_check(job, &raw mut status);
        assert_eq!(ret, ffi::QDMI_SUCCESS);
        eprintln!(
            "       status = {status} (DONE={}, QUEUED={}, RUNNING={})",
            ffi::QDMI_JOB_STATUS_DONE,
            ffi::QDMI_JOB_STATUS_QUEUED,
            ffi::QDMI_JOB_STATUS_RUNNING
        );

        // ── Function 14: job_wait ────────────────────────────────────
        eprintln!("[14/18] job_wait (timeout=30s)");
        let ret = ARVAK_QDMI_device_job_wait(job, 30_000);
        assert_eq!(ret, ffi::QDMI_SUCCESS, "job_wait timed out");

        // Verify status is DONE after wait
        let ret = ARVAK_QDMI_device_job_check(job, &raw mut status);
        assert_eq!(ret, ffi::QDMI_SUCCESS);
        assert_eq!(status, ffi::QDMI_JOB_STATUS_DONE, "job not DONE after wait");
        eprintln!("       job completed successfully");

        // ── Function 16: job_get_results ─────────────────────────────
        eprintln!("[16/18] job_get_results");

        // HISTKEYS (null-separated bitstrings)
        let keys_buf = query_bytes(|sz, val, szr| {
            ARVAK_QDMI_device_job_get_results(job, ffi::QDMI_JOB_RESULT_HISTKEYS, sz, val, szr)
        });
        let keys: Vec<&str> = std::str::from_utf8(&keys_buf)
            .unwrap()
            .split('\0')
            .filter(|s| !s.is_empty())
            .collect();
        eprintln!("       HISTKEYS = {:?}", keys);
        assert!(!keys.is_empty(), "no histogram keys");

        // HISTVALUES (array of u64)
        let vals_buf = query_bytes(|sz, val, szr| {
            ARVAK_QDMI_device_job_get_results(job, ffi::QDMI_JOB_RESULT_HISTVALUES, sz, val, szr)
        });
        let num_vals = vals_buf.len() / std::mem::size_of::<u64>();
        let values: Vec<u64> = (0..num_vals)
            .map(|i| {
                let off = i * 8;
                u64::from_ne_bytes(vals_buf[off..off + 8].try_into().unwrap())
            })
            .collect();
        eprintln!("       HISTVALUES = {:?}", values);
        assert_eq!(keys.len(), values.len());

        // SHOTS
        let shots_buf = query_bytes(|sz, val, szr| {
            ARVAK_QDMI_device_job_get_results(job, ffi::QDMI_JOB_RESULT_SHOTS, sz, val, szr)
        });
        let total_shots = u64::from_ne_bytes(shots_buf[..8].try_into().unwrap());
        eprintln!("       SHOTS = {total_shots}");
        assert_eq!(total_shots, 512);
        assert_eq!(values.iter().sum::<u64>(), total_shots);

        // Verify Bell state: should be ~50/50 |00⟩ and |11⟩
        eprintln!("       Histogram:");
        for (k, v) in keys.iter().zip(&values) {
            let pct = *v as f64 / total_shots as f64 * 100.0;
            eprintln!("         |{k}⟩ = {v} ({pct:.1}%)");
        }
        let bell_counts: u64 = keys
            .iter()
            .zip(&values)
            .filter(|(k, _)| **k == "00" || **k == "11")
            .map(|(_, v)| *v)
            .sum();
        let bell_frac = bell_counts as f64 / total_shots as f64;
        eprintln!("       Bell fidelity: {:.1}%", bell_frac * 100.0);
        assert!(bell_frac > 0.95, "Bell fidelity too low: {bell_frac:.3}");

        // ── Function 17: job_query_property (stub) ───────────────────
        eprintln!("[17/18] job_query_property (expected: NOTSUPPORTED)");
        let mut sz_ret: usize = 0;
        let ret = ARVAK_QDMI_device_job_query_property(
            job,
            ffi::QDMI_DEVICE_JOB_PROPERTY_ID,
            0,
            std::ptr::null_mut(),
            &raw mut sz_ret,
        );
        assert_eq!(ret, ffi::QDMI_ERROR_NOTSUPPORTED);

        // ── Function 15: job_cancel (test with a second job) ─────────
        eprintln!("[15/18] job_cancel");
        let mut job2: *mut c_void = std::ptr::null_mut();
        let ret = ARVAK_QDMI_device_session_create_device_job(session, &raw mut job2);
        assert_eq!(ret, ffi::QDMI_SUCCESS);

        // Cancel before submit — should succeed silently
        let ret = ARVAK_QDMI_device_job_cancel(job2);
        assert_eq!(ret, ffi::QDMI_SUCCESS);

        // Submit and then cancel
        let ret = ARVAK_QDMI_device_job_set_parameter(
            job2,
            ffi::QDMI_DEVICE_JOB_PARAMETER_PROGRAM,
            qasm.len(),
            qasm.as_ptr().cast(),
        );
        assert_eq!(ret, ffi::QDMI_SUCCESS);
        let ret = ARVAK_QDMI_device_job_submit(job2);
        assert_eq!(ret, ffi::QDMI_SUCCESS);
        let ret = ARVAK_QDMI_device_job_cancel(job2);
        // Cancel may succeed or fail (job may have completed already)
        eprintln!("       cancel after submit: ret={ret}");
        assert!(ret == ffi::QDMI_SUCCESS || ret == ffi::QDMI_ERROR_FATAL);

        // ── Function 18: job_free ────────────────────────────────────
        eprintln!("[18/18] job_free");
        let ret = ARVAK_QDMI_device_job_free(job);
        assert_eq!(ret, ffi::QDMI_SUCCESS);
        let ret = ARVAK_QDMI_device_job_free(job2);
        assert_eq!(ret, ffi::QDMI_SUCCESS);

        // ── Function 6: session_free ─────────────────────────────────
        eprintln!("[6/18] session_free");
        let ret = ARVAK_QDMI_device_session_free(session);
        assert_eq!(ret, ffi::QDMI_SUCCESS);

        // ── Function 2: device_finalize ──────────────────────────────
        eprintln!("[2/18] device_finalize");
        let ret = ARVAK_QDMI_device_finalize();
        assert_eq!(ret, ffi::QDMI_SUCCESS);

        // After finalize, all functions should return BADSTATE
        let ret = ARVAK_QDMI_device_session_alloc(&raw mut session);
        assert_eq!(ret, ffi::QDMI_ERROR_BADSTATE);

        eprintln!("\n=== ALL 18 QDMI FUNCTIONS TESTED SUCCESSFULLY ===");
    }
}
