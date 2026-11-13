// SPDX-License-Identifier: Apache-2.0
#![allow(unsafe_op_in_unsafe_fn, clippy::missing_safety_doc)]
//! QDMI device library exposing Arvak as a composite quantum device.
//!
//! This crate produces `libqdmi_arvak.so` (or `.dylib` on macOS). When loaded
//! by a QDMI driver (e.g. MQT Core), it connects to a remote Arvak server via
//! gRPC and exposes Arvak's 13 backend adapters through the standard QDMI
//! device interface.
//!
//! All functions use the `ARVAK_` prefix for QDMI name-shifting.
//!
//! # QDMI ABI
//!
//! By default the library implements the **QDMI 1.4** device interface: 22
//! functions, programs via `ARVAK_QDMI_device_job_set_programs`, and
//! `get_results` with a program index. Building with the `qdmi-v1-3` feature
//! produces the legacy **QDMI 1.2/1.3** interface (18 functions, programs via
//! the `PROGRAMFORMAT`/`PROGRAM` job parameters) for older drivers. The 1.4-only
//! symbols are not exported in that build, so loaders that detect the ABI by
//! `set_programs` (MQT Core, `arvak-qdmi`) pick the right path.
//!
//! # Configuration
//!
//! The server URL is determined in order of precedence:
//! 1. `session_set_parameter(BASEURL, "https://...")` (per-session)
//! 2. `ARVAK_QDMI_URL` environment variable
//! 3. Default: `https://arvak.io/qdmi`

mod job;
mod session;
mod state;

#[cfg(not(feature = "qdmi-v1-3"))]
use std::ffi::c_char;
use std::ffi::{CStr, c_int, c_void};
use std::slice;

use arvak_grpc::proto::{
    CancelJobRequest, CircuitPayload, GetJobResultRequest, GetJobStatusRequest, JobState,
    SubmitJobRequest, circuit_payload,
};
use arvak_qdmi::ffi;
use tonic::Request;

// ---------------------------------------------------------------------------
// Helper: catch panics at the FFI boundary
// ---------------------------------------------------------------------------

macro_rules! qdmi_guard {
    ($body:expr) => {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
            Ok(code) => code,
            Err(_) => ffi::QDMI_ERROR_FATAL,
        }
    };
}

// ---------------------------------------------------------------------------
// Two-phase query helper
// ---------------------------------------------------------------------------

/// Handle the two-phase buffer query pattern.
///
/// If `value` is null, write the required size to `size_ret` and return SUCCESS.
/// If `value` is non-null, copy the data into the buffer.
unsafe fn write_query_result(
    data: &[u8],
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int {
    if value.is_null() {
        // Phase 1: size probe
        if !size_ret.is_null() {
            *size_ret = data.len();
        }
        return ffi::QDMI_SUCCESS;
    }
    // Phase 2: data read
    if size < data.len() {
        return ffi::QDMI_ERROR_OUTOFRANGE;
    }
    std::ptr::copy_nonoverlapping(data.as_ptr(), value.cast::<u8>(), data.len());
    if !size_ret.is_null() {
        *size_ret = data.len();
    }
    ffi::QDMI_SUCCESS
}

// ===========================================================================
// Device lifecycle
// ===========================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_initialize() -> c_int {
    qdmi_guard!({
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
            )
            .try_init();

        if state::initialize() {
            tracing::info!("arvak-qdmi-device initialized");
            ffi::QDMI_SUCCESS
        } else {
            // Already initialized — that's fine per QDMI spec
            ffi::QDMI_SUCCESS
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_finalize() -> c_int {
    qdmi_guard!({
        if let Some(gs) = state::get() {
            gs.finalized
                .store(true, std::sync::atomic::Ordering::Relaxed);
            tracing::info!("arvak-qdmi-device finalized");
            ffi::QDMI_SUCCESS
        } else {
            ffi::QDMI_ERROR_BADSTATE
        }
    })
}

// ===========================================================================
// Session lifecycle
// ===========================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_session_alloc(session_out: *mut *mut c_void) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(id) = gs.alloc_session_id() else {
            return ffi::QDMI_ERROR_OUTOFMEM;
        };
        gs.runtime.block_on(async {
            gs.sessions
                .write()
                .await
                .insert(id, session::ArvakSession::new());
        });
        *session_out = state::id_to_handle(id);
        ffi::QDMI_SUCCESS
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_session_set_parameter(
    session: *mut c_void,
    param: c_int,
    size: usize,
    value: *const c_void,
) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(id) = state::handle_to_id(session) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };

        if value.is_null() || size == 0 {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        }
        let bytes = slice::from_raw_parts(value.cast::<u8>(), size);
        let Ok(s) = CStr::from_bytes_until_nul(bytes).map(|c| c.to_string_lossy().into_owned())
        else {
            // Try as raw string without nul terminator
            let Ok(s) = std::str::from_utf8(bytes) else {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            };
            let val = s.to_string();
            return gs.runtime.block_on(async {
                let mut sessions = gs.sessions.write().await;
                let Some(sess) = sessions.get_mut(&id) else {
                    return ffi::QDMI_ERROR_INVALIDARGUMENT;
                };
                match param {
                    ffi::QDMI_DEVICE_SESSION_PARAMETER_BASEURL => {
                        sess.server_url = Some(val);
                        ffi::QDMI_SUCCESS
                    }
                    ffi::QDMI_DEVICE_SESSION_PARAMETER_TOKEN => {
                        sess.token = Some(val);
                        ffi::QDMI_SUCCESS
                    }
                    _ => ffi::QDMI_ERROR_INVALIDARGUMENT,
                }
            });
        };

        gs.runtime.block_on(async {
            let mut sessions = gs.sessions.write().await;
            let Some(sess) = sessions.get_mut(&id) else {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            };
            match param {
                ffi::QDMI_DEVICE_SESSION_PARAMETER_BASEURL => {
                    sess.server_url = Some(s);
                    ffi::QDMI_SUCCESS
                }
                ffi::QDMI_DEVICE_SESSION_PARAMETER_TOKEN => {
                    sess.token = Some(s);
                    ffi::QDMI_SUCCESS
                }
                _ => ffi::QDMI_ERROR_INVALIDARGUMENT,
            }
        })
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_session_init(session: *mut c_void) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(id) = state::handle_to_id(session) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };

        gs.runtime.block_on(async {
            let mut sessions = gs.sessions.write().await;
            let Some(sess) = sessions.get_mut(&id) else {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            };
            match sess.connect().await {
                Ok(()) => ffi::QDMI_SUCCESS,
                Err(e) => {
                    tracing::error!("session_init failed: {e}");
                    ffi::QDMI_ERROR_FATAL
                }
            }
        })
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_session_free(session: *mut c_void) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(id) = state::handle_to_id(session) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };

        gs.runtime.block_on(async {
            // Remove session
            gs.sessions.write().await.remove(&id);
            // Remove all jobs belonging to this session
            gs.jobs.write().await.retain(|_, j| j.session_id != id);
        });
        ffi::QDMI_SUCCESS
    })
}

// ===========================================================================
// Property queries
// ===========================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_session_query_device_property(
    session: *mut c_void,
    prop: c_int,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(id) = state::handle_to_id(session) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };

        gs.runtime.block_on(async {
            let sessions = gs.sessions.read().await;
            let Some(sess) = sessions.get(&id) else {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            };

            match prop {
                ffi::QDMI_DEVICE_PROPERTY_NAME => {
                    let data = b"arvak\0";
                    write_query_result(data, size, value, size_ret)
                }
                ffi::QDMI_DEVICE_PROPERTY_VERSION => {
                    let data = b"2.0.0\0";
                    write_query_result(data, size, value, size_ret)
                }
                ffi::QDMI_DEVICE_PROPERTY_STATUS => {
                    let status: c_int = if sess.client.is_some() {
                        ffi::QDMI_DEVICE_STATUS_IDLE
                    } else {
                        ffi::QDMI_DEVICE_STATUS_OFFLINE
                    };
                    let data = status.to_ne_bytes();
                    write_query_result(&data, size, value, size_ret)
                }
                ffi::QDMI_DEVICE_PROPERTY_QUBITSNUM => {
                    let n = sess
                        .active_backend
                        .as_ref()
                        .map_or(0usize, |b| b.max_qubits as usize);
                    let data = n.to_ne_bytes();
                    write_query_result(&data, size, value, size_ret)
                }
                ffi::QDMI_DEVICE_PROPERTY_SITES => {
                    let n = sess.active_backend.as_ref().map_or(0u32, |b| b.max_qubits);
                    // Return array of opaque site handles (1-based IDs as pointers)
                    let handles: Vec<*mut c_void> =
                        (1..=n as usize).map(state::id_to_handle).collect();
                    let data = slice::from_raw_parts(
                        handles.as_ptr().cast::<u8>(),
                        handles.len() * std::mem::size_of::<*mut c_void>(),
                    );
                    write_query_result(data, size, value, size_ret)
                }
                ffi::QDMI_DEVICE_PROPERTY_OPERATIONS => {
                    let gates = sess
                        .active_backend
                        .as_ref()
                        .map_or(&[][..], |b| &b.supported_gates);
                    // One handle per gate, starting at 0x1000 to avoid collision with sites
                    let handles: Vec<*mut c_void> = (0..gates.len())
                        .map(|i| state::id_to_handle(0x1000 + i))
                        .collect();
                    let data = slice::from_raw_parts(
                        handles.as_ptr().cast::<u8>(),
                        handles.len() * std::mem::size_of::<*mut c_void>(),
                    );
                    write_query_result(data, size, value, size_ret)
                }
                ffi::QDMI_DEVICE_PROPERTY_COUPLINGMAP => {
                    let topo_json = sess
                        .active_backend
                        .as_ref()
                        .map_or("", |b| &b.topology_json);
                    // Parse topology JSON: {"kind":"...", "edges":[[0,1],[1,2],...]}
                    // or bare edge list [[0,1],[1,2],...]
                    let edges: Vec<Vec<usize>> =
                        serde_json::from_str::<serde_json::Value>(topo_json)
                            .ok()
                            .and_then(|v| {
                                // Try object with "edges" field first, then bare array
                                let arr = v.get("edges").unwrap_or(&v);
                                serde_json::from_value(arr.clone()).ok()
                            })
                            .unwrap_or_default();
                    let mut handles: Vec<*mut c_void> = Vec::new();
                    for edge in &edges {
                        if edge.len() == 2 {
                            handles.push(state::id_to_handle(edge[0] + 1)); // 1-based
                            handles.push(state::id_to_handle(edge[1] + 1));
                        }
                    }
                    let data = slice::from_raw_parts(
                        handles.as_ptr().cast::<u8>(),
                        handles.len() * std::mem::size_of::<*mut c_void>(),
                    );
                    write_query_result(data, size, value, size_ret)
                }
                ffi::QDMI_DEVICE_PROPERTY_SUPPORTEDPROGRAMFORMATS => {
                    let formats: [c_int; 1] = [ffi::QDMI_PROGRAM_FORMAT_QASM3];
                    let data = slice::from_raw_parts(
                        formats.as_ptr().cast::<u8>(),
                        std::mem::size_of_val(&formats),
                    );
                    write_query_result(data, size, value, size_ret)
                }
                _ => ffi::QDMI_ERROR_NOTSUPPORTED,
            }
        })
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_session_query_site_property(
    session: *mut c_void,
    site: *mut c_void,
    prop: c_int,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int {
    qdmi_guard!({
        let Some(_gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let site_idx = (site as usize).wrapping_sub(1); // 1-based → 0-based

        match prop {
            ffi::QDMI_SITE_PROPERTY_INDEX => {
                let data = site_idx.to_ne_bytes();
                write_query_result(&data, size, value, size_ret)
            }
            ffi::QDMI_SITE_PROPERTY_NAME => {
                let name = format!("q{site_idx}\0");
                write_query_result(name.as_bytes(), size, value, size_ret)
            }
            _ => {
                let _ = session;
                ffi::QDMI_ERROR_NOTSUPPORTED
            }
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_session_query_operation_property(
    session: *mut c_void,
    operation: *mut c_void,
    _num_sites: usize,
    _sites: *const *mut c_void,
    _num_params: usize,
    _params: *const f64,
    prop: c_int,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(sess_id) = state::handle_to_id(session) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };
        let op_idx = (operation as usize).wrapping_sub(0x1000);

        gs.runtime.block_on(async {
            let sessions = gs.sessions.read().await;
            let Some(sess) = sessions.get(&sess_id) else {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            };
            let gates = sess
                .active_backend
                .as_ref()
                .map_or(&[][..], |b| &b.supported_gates);
            let Some(gate_name) = gates.get(op_idx) else {
                return ffi::QDMI_ERROR_OUTOFRANGE;
            };

            match prop {
                ffi::QDMI_OPERATION_PROPERTY_NAME => {
                    let mut name = gate_name.clone();
                    name.push('\0');
                    write_query_result(name.as_bytes(), size, value, size_ret)
                }
                ffi::QDMI_OPERATION_PROPERTY_QUBITSNUM => {
                    let n: usize = match gate_name.as_str() {
                        "cx" | "cz" | "ecr" | "rxx" | "rzz" | "iswap" | "xx" | "swap" => 2,
                        "ccx" | "cswap" => 3,
                        _ => 1,
                    };
                    let data = n.to_ne_bytes();
                    write_query_result(&data, size, value, size_ret)
                }
                ffi::QDMI_OPERATION_PROPERTY_PARAMETERSNUM => {
                    let n: usize = match gate_name.as_str() {
                        "rz" | "rx" | "ry" | "p" | "u1" => 1,
                        "u2" | "rxx" | "rzz" => 2,
                        "u3" | "u" => 3,
                        _ => 0,
                    };
                    let data = n.to_ne_bytes();
                    write_query_result(&data, size, value, size_ret)
                }
                _ => ffi::QDMI_ERROR_NOTSUPPORTED,
            }
        })
    })
}

// ===========================================================================
// Job interface
// ===========================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_session_create_device_job(
    session: *mut c_void,
    job_out: *mut *mut c_void,
) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(sess_id) = state::handle_to_id(session) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };
        let Some(job_id) = gs.alloc_job_id() else {
            return ffi::QDMI_ERROR_OUTOFMEM;
        };

        gs.runtime.block_on(async {
            gs.jobs
                .write()
                .await
                .insert(job_id, job::ArvakJob::new(sess_id));
        });
        *job_out = state::id_to_handle(job_id);
        ffi::QDMI_SUCCESS
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_job_set_parameter(
    job_handle: *mut c_void,
    param: c_int,
    size: usize,
    value: *const c_void,
) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(jid) = state::handle_to_id(job_handle) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };

        gs.runtime.block_on(async {
            let mut jobs = gs.jobs.write().await;
            let Some(job) = jobs.get_mut(&jid) else {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            };

            if job.retrieved || job.grpc_job_id.is_some() {
                return ffi::QDMI_ERROR_BADSTATE;
            }

            match param {
                #[cfg(feature = "qdmi-v1-3")]
                ffi::QDMI_DEVICE_JOB_PARAMETER_PROGRAMFORMAT => {
                    // Recorded but not enforced: 1.3 clients historically
                    // sent QASM3 text regardless of the declared format.
                    if !value.is_null() && size >= std::mem::size_of::<c_int>() {
                        job.format = Some(value.cast::<c_int>().read_unaligned());
                    }
                    ffi::QDMI_SUCCESS
                }
                #[cfg(feature = "qdmi-v1-3")]
                ffi::QDMI_DEVICE_JOB_PARAMETER_PROGRAM => {
                    if value.is_null() || size == 0 {
                        return ffi::QDMI_ERROR_INVALIDARGUMENT;
                    }
                    let bytes = slice::from_raw_parts(value.cast::<u8>(), size);
                    let program = CStr::from_bytes_until_nul(bytes).map_or_else(
                        |_| String::from_utf8_lossy(bytes).into_owned(),
                        |c| c.to_string_lossy().into_owned(),
                    );
                    job.program = Some(program);
                    ffi::QDMI_SUCCESS
                }
                // Removed in QDMI 1.4; the values stay reserved.
                #[cfg(not(feature = "qdmi-v1-3"))]
                ffi::QDMI_DEVICE_JOB_PARAMETER_PROGRAMFORMAT
                | ffi::QDMI_DEVICE_JOB_PARAMETER_PROGRAM => ffi::QDMI_ERROR_NOTSUPPORTED,
                ffi::QDMI_DEVICE_JOB_PARAMETER_SHOTSNUM => match read_shots(value, size) {
                    Some(shots) if shots > 0 => {
                        job.shots = u32::try_from(shots).unwrap_or(u32::MAX);
                        ffi::QDMI_SUCCESS
                    }
                    _ => ffi::QDMI_ERROR_INVALIDARGUMENT,
                },
                _ => ffi::QDMI_ERROR_INVALIDARGUMENT,
            }
        })
    })
}

/// Read the `SHOTSNUM` value: `size_t` per the QDMI spec. Older Arvak
/// clients passed a C `int`, so a 4-byte value is accepted as well.
unsafe fn read_shots(value: *const c_void, size: usize) -> Option<u64> {
    if value.is_null() {
        return None;
    }
    if size == std::mem::size_of::<usize>() {
        u64::try_from(value.cast::<usize>().read_unaligned()).ok()
    } else if size == std::mem::size_of::<c_int>() {
        u64::try_from(value.cast::<c_int>().read_unaligned()).ok()
    } else {
        None
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_job_submit(job_handle: *mut c_void) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(jid) = state::handle_to_id(job_handle) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };

        gs.runtime.block_on(async {
            // Read job data
            let (sess_id, program, shots, backend_id) = {
                let jobs = gs.jobs.read().await;
                let Some(job) = jobs.get(&jid) else {
                    return ffi::QDMI_ERROR_INVALIDARGUMENT;
                };
                if job.retrieved || job.grpc_job_id.is_some() {
                    return ffi::QDMI_ERROR_BADSTATE;
                }
                let Some(program) = job.program.clone() else {
                    tracing::error!("job_submit: no program set");
                    return ffi::QDMI_ERROR_BADSTATE;
                };
                let sessions = gs.sessions.read().await;
                let backend_id = sessions
                    .get(&job.session_id)
                    .map_or("simulator".into(), |s| s.backend_id.clone());
                (job.session_id, program, job.shots, backend_id)
            };

            // Submit via gRPC
            let mut sessions = gs.sessions.write().await;
            let Some(sess) = sessions.get_mut(&sess_id) else {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            };
            let Ok(client) = sess.client_mut() else {
                return ffi::QDMI_ERROR_BADSTATE;
            };

            let req = Request::new(SubmitJobRequest {
                circuit: Some(CircuitPayload {
                    format: Some(circuit_payload::Format::Qasm3(program)),
                }),
                backend_id,
                shots,
                optimization_level: 1,
            });

            match client.submit_job(req).await {
                Ok(resp) => {
                    let grpc_id = resp.into_inner().job_id;
                    tracing::info!("submitted job {jid} → gRPC {grpc_id}");
                    drop(sessions);
                    gs.jobs.write().await.get_mut(&jid).unwrap().grpc_job_id = Some(grpc_id);
                    ffi::QDMI_SUCCESS
                }
                Err(e) => {
                    tracing::error!("job_submit gRPC error: {e}");
                    ffi::QDMI_ERROR_FATAL
                }
            }
        })
    })
}

/// Async helper: check job status via gRPC without calling `block_on`.
/// Used by both `job_check` and `job_wait` to avoid nested runtime panics.
async unsafe fn check_job_status_async(
    gs: &state::GlobalState,
    job_handle: *mut c_void,
    status_out: *mut c_int,
) -> c_int {
    let Some(jid) = state::handle_to_id(job_handle) else {
        return ffi::QDMI_ERROR_INVALIDARGUMENT;
    };

    let (sess_id, grpc_id) = {
        let jobs = gs.jobs.read().await;
        let Some(job) = jobs.get(&jid) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };
        let Some(grpc_id) = job.grpc_job_id.clone() else {
            *status_out = ffi::QDMI_JOB_STATUS_CREATED;
            return ffi::QDMI_SUCCESS;
        };
        (job.session_id, grpc_id)
    };

    let mut sessions = gs.sessions.write().await;
    let Some(sess) = sessions.get_mut(&sess_id) else {
        return ffi::QDMI_ERROR_INVALIDARGUMENT;
    };
    let Ok(client) = sess.client_mut() else {
        return ffi::QDMI_ERROR_BADSTATE;
    };

    match client
        .get_job_status(Request::new(GetJobStatusRequest { job_id: grpc_id }))
        .await
    {
        Ok(resp) => {
            let job_state = resp.into_inner().job.map_or(JobState::Unspecified, |j| {
                JobState::try_from(j.state).unwrap_or(JobState::Unspecified)
            });
            *status_out = match job_state {
                JobState::Queued => ffi::QDMI_JOB_STATUS_QUEUED,
                JobState::Running => ffi::QDMI_JOB_STATUS_RUNNING,
                JobState::Completed => ffi::QDMI_JOB_STATUS_DONE,
                JobState::Failed => ffi::QDMI_JOB_STATUS_FAILED,
                JobState::Canceled => ffi::QDMI_JOB_STATUS_CANCELED,
                // QDMI has no expired-result state; execution did complete.
                JobState::ResultExpired => ffi::QDMI_JOB_STATUS_DONE,
                JobState::Unspecified => ffi::QDMI_JOB_STATUS_SUBMITTED,
            };
            ffi::QDMI_SUCCESS
        }
        Err(e) => {
            tracing::error!("job_check gRPC error: {e}");
            ffi::QDMI_ERROR_FATAL
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_job_check(
    job_handle: *mut c_void,
    status_out: *mut c_int,
) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        gs.runtime
            .block_on(check_job_status_async(gs, job_handle, status_out))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_job_wait(
    job_handle: *mut c_void,
    timeout_ms: usize,
) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };

        gs.runtime.block_on(async {
            let deadline = if timeout_ms == 0 {
                None
            } else {
                Some(
                    tokio::time::Instant::now()
                        + std::time::Duration::from_millis(
                            u64::try_from(timeout_ms).unwrap_or(u64::MAX),
                        ),
                )
            };

            loop {
                let mut status: c_int = 0;
                let ret = check_job_status_async(gs, job_handle, &raw mut status).await;
                if ret != ffi::QDMI_SUCCESS {
                    return ret;
                }
                match status {
                    ffi::QDMI_JOB_STATUS_DONE
                    | ffi::QDMI_JOB_STATUS_FAILED
                    | ffi::QDMI_JOB_STATUS_CANCELED => return ffi::QDMI_SUCCESS,
                    _ => {}
                }
                if let Some(dl) = deadline {
                    if tokio::time::Instant::now() >= dl {
                        return ffi::QDMI_ERROR_TIMEOUT;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        })
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_job_cancel(job_handle: *mut c_void) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(jid) = state::handle_to_id(job_handle) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };

        gs.runtime.block_on(async {
            let (sess_id, grpc_id) = {
                let jobs = gs.jobs.read().await;
                let Some(job) = jobs.get(&jid) else {
                    return ffi::QDMI_ERROR_INVALIDARGUMENT;
                };
                let Some(grpc_id) = job.grpc_job_id.clone() else {
                    return ffi::QDMI_SUCCESS; // Not submitted yet
                };
                (job.session_id, grpc_id)
            };

            let mut sessions = gs.sessions.write().await;
            let Some(sess) = sessions.get_mut(&sess_id) else {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            };
            let Ok(client) = sess.client_mut() else {
                return ffi::QDMI_ERROR_BADSTATE;
            };

            match client
                .cancel_job(Request::new(CancelJobRequest { job_id: grpc_id }))
                .await
            {
                Ok(_) => ffi::QDMI_SUCCESS,
                Err(e) => {
                    tracing::error!("job_cancel gRPC error: {e}");
                    ffi::QDMI_ERROR_FATAL
                }
            }
        })
    })
}

/// QDMI 1.4: `get_results(job, program_index, result, size, data, size_ret)`.
#[cfg(not(feature = "qdmi-v1-3"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_job_get_results(
    job_handle: *mut c_void,
    program_index: usize,
    result_type: c_int,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int {
    qdmi_guard!(get_results_impl(
        job_handle,
        program_index,
        result_type,
        size,
        value,
        size_ret
    ))
}

/// QDMI 1.2/1.3: `get_results(job, result, size, data, size_ret)`.
#[cfg(feature = "qdmi-v1-3")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_job_get_results(
    job_handle: *mut c_void,
    result_type: c_int,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int {
    qdmi_guard!(get_results_impl(
        job_handle,
        0,
        result_type,
        size,
        value,
        size_ret
    ))
}

#[allow(clippy::too_many_lines)]
unsafe fn get_results_impl(
    job_handle: *mut c_void,
    program_index: usize,
    result_type: c_int,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int {
    {
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(jid) = state::handle_to_id(job_handle) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };

        gs.runtime.block_on(async {
            // Fetch results if not cached
            {
                let jobs = gs.jobs.read().await;
                let Some(job) = jobs.get(&jid) else {
                    return ffi::QDMI_ERROR_INVALIDARGUMENT;
                };
                if program_index >= job.num_programs().max(1) {
                    return ffi::QDMI_ERROR_OUTOFRANGE;
                }
                if job.cached_results.is_none() {
                    let Some(grpc_id) = job.grpc_job_id.clone() else {
                        return ffi::QDMI_ERROR_BADSTATE;
                    };
                    let sess_id = job.session_id;
                    drop(jobs);

                    let mut sessions = gs.sessions.write().await;
                    let Some(sess) = sessions.get_mut(&sess_id) else {
                        return ffi::QDMI_ERROR_INVALIDARGUMENT;
                    };
                    let Ok(client) = sess.client_mut() else {
                        return ffi::QDMI_ERROR_BADSTATE;
                    };

                    match client
                        .get_job_result(Request::new(GetJobResultRequest { job_id: grpc_id }))
                        .await
                    {
                        Ok(resp) => {
                            let result = resp.into_inner().result;
                            let mut counts: Vec<(String, u64)> =
                                result.map_or(Vec::new(), |r| r.counts.into_iter().collect());
                            counts.sort_by(|a, b| a.0.cmp(&b.0));
                            drop(sessions);
                            gs.jobs.write().await.get_mut(&jid).unwrap().cached_results =
                                Some(counts);
                        }
                        Err(e) => {
                            tracing::error!("get_results gRPC error: {e}");
                            return ffi::QDMI_ERROR_FATAL;
                        }
                    }
                }
            }

            let jobs = gs.jobs.read().await;
            let job = jobs.get(&jid).unwrap();
            let results = job.cached_results.as_ref().unwrap();

            match result_type {
                ffi::QDMI_JOB_RESULT_HISTKEYS => {
                    // Null-separated bitstrings, double-null terminated
                    let mut data = Vec::new();
                    for (bs, _) in results {
                        data.extend_from_slice(bs.as_bytes());
                        data.push(0);
                    }
                    data.push(0); // double-null terminator
                    write_query_result(&data, size, value, size_ret)
                }
                ffi::QDMI_JOB_RESULT_HISTVALUES => {
                    // Array of u64 counts, same order as HISTKEYS
                    let vals: Vec<u64> = results.iter().map(|(_, c)| *c).collect();
                    let data = slice::from_raw_parts(
                        vals.as_ptr().cast::<u8>(),
                        vals.len() * std::mem::size_of::<u64>(),
                    );
                    write_query_result(data, size, value, size_ret)
                }
                ffi::QDMI_JOB_RESULT_SHOTS => {
                    let total: u64 = results.iter().map(|(_, c)| *c).sum();
                    let data = total.to_ne_bytes();
                    write_query_result(&data, size, value, size_ret)
                }
                _ => ffi::QDMI_ERROR_NOTSUPPORTED,
            }
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_job_query_property(
    job_handle: *mut c_void,
    prop: c_int,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(jid) = state::handle_to_id(job_handle) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };

        gs.runtime.block_on(async {
            let jobs = gs.jobs.read().await;
            let Some(job) = jobs.get(&jid) else {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            };

            match prop {
                // The gRPC job ID: unique per server and accepted by
                // `retrieve_device_job_by_id`. Assigned on submission.
                ffi::QDMI_DEVICE_JOB_PROPERTY_ID => match &job.grpc_job_id {
                    Some(id) => {
                        let data = [id.as_bytes(), b"\0"].concat();
                        write_query_result(&data, size, value, size_ret)
                    }
                    None => ffi::QDMI_ERROR_BADSTATE,
                },
                ffi::QDMI_DEVICE_JOB_PROPERTY_SHOTSNUM if !job.retrieved => {
                    let data = (job.shots as usize).to_ne_bytes();
                    write_query_result(&data, size, value, size_ret)
                }
                #[cfg(not(feature = "qdmi-v1-3"))]
                ffi::QDMI_DEVICE_JOB_PROPERTY_PROGRAMFORMAT => match job.format {
                    Some(format) => {
                        write_query_result(&format.to_ne_bytes(), size, value, size_ret)
                    }
                    None if job.retrieved => ffi::QDMI_ERROR_NOTSUPPORTED,
                    None => ffi::QDMI_ERROR_BADSTATE,
                },
                #[cfg(not(feature = "qdmi-v1-3"))]
                ffi::QDMI_DEVICE_JOB_PROPERTY_PROGRAMSNUM => match job.num_programs() {
                    0 => ffi::QDMI_ERROR_BADSTATE,
                    n => write_query_result(&n.to_ne_bytes(), size, value, size_ret),
                },
                _ => ffi::QDMI_ERROR_NOTSUPPORTED,
            }
        })
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_job_free(job_handle: *mut c_void) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(jid) = state::handle_to_id(job_handle) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };

        gs.runtime.block_on(async {
            gs.jobs.write().await.remove(&jid);
        });
        ffi::QDMI_SUCCESS
    })
}

// ===========================================================================
// QDMI 1.4 job interface
// ===========================================================================

/// Set the job's program list. Arvak runs one gRPC job per QDMI job, so only
/// a single QASM3 program is accepted; longer lists return `NOTSUPPORTED`
/// (the spec's signal for clients to submit separate jobs).
#[cfg(not(feature = "qdmi-v1-3"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_job_set_programs(
    job_handle: *mut c_void,
    format: c_int,
    count: usize,
    sizes: *const usize,
    programs: *const *const c_void,
) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(jid) = state::handle_to_id(job_handle) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };
        if count == 0 || !(0..ffi::QDMI_PROGRAM_FORMAT_BATCHJOB + 1).contains(&format) {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        }
        if format != ffi::QDMI_PROGRAM_FORMAT_QASM3 || count != 1 {
            return ffi::QDMI_ERROR_NOTSUPPORTED;
        }

        gs.runtime.block_on(async {
            let mut jobs = gs.jobs.write().await;
            let Some(job) = jobs.get_mut(&jid) else {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            };
            if programs.is_null() {
                return ffi::QDMI_SUCCESS; // support check only
            }
            if job.retrieved || job.grpc_job_id.is_some() {
                return ffi::QDMI_ERROR_BADSTATE;
            }
            if sizes.is_null() {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            }
            let (ptr, len) = (*programs, *sizes);
            if ptr.is_null() || len == 0 {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            }

            // Text programs carry exactly one '\0', as the final byte.
            let bytes = slice::from_raw_parts(ptr.cast::<u8>(), len);
            let Some((&0, text)) = bytes.split_last() else {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            };
            if text.contains(&0) {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            }
            let Ok(text) = std::str::from_utf8(text) else {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            };

            job.format = Some(format);
            job.program = Some(text.to_owned());
            ffi::QDMI_SUCCESS
        })
    })
}

#[cfg(not(feature = "qdmi-v1-3"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_job_get_program(
    job_handle: *mut c_void,
    program_index: usize,
    size: usize,
    data: *mut c_void,
    size_ret: *mut usize,
) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(jid) = state::handle_to_id(job_handle) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };

        gs.runtime.block_on(async {
            let jobs = gs.jobs.read().await;
            let Some(job) = jobs.get(&jid) else {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            };
            if job.num_programs() == 0 {
                return ffi::QDMI_ERROR_BADSTATE;
            }
            if program_index >= job.num_programs() {
                return ffi::QDMI_ERROR_OUTOFRANGE;
            }
            // Retrieved jobs do not carry the original program bytes.
            let Some(program) = &job.program else {
                return ffi::QDMI_ERROR_NOTSUPPORTED;
            };
            let bytes = [program.as_bytes(), b"\0"].concat();
            if !data.is_null() && size < bytes.len() {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            }
            write_query_result(&bytes, size, data, size_ret)
        })
    })
}

/// Per-program status. A QDMI job here is exactly one gRPC job, so the
/// single program's status is the job status.
#[cfg(not(feature = "qdmi-v1-3"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_job_get_program_status(
    job_handle: *mut c_void,
    program_index: usize,
    status: *mut c_int,
) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(jid) = state::handle_to_id(job_handle) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };

        gs.runtime.block_on(async {
            let num_programs = {
                let jobs = gs.jobs.read().await;
                let Some(job) = jobs.get(&jid) else {
                    return ffi::QDMI_ERROR_INVALIDARGUMENT;
                };
                job.num_programs()
            };
            if num_programs == 0 {
                return ffi::QDMI_ERROR_BADSTATE;
            }
            if program_index >= num_programs {
                return ffi::QDMI_ERROR_OUTOFRANGE;
            }
            if status.is_null() {
                return ffi::QDMI_ERROR_INVALIDARGUMENT;
            }
            check_job_status_async(gs, job_handle, status).await
        })
    })
}

/// Re-attach to a submitted job by its gRPC job ID (the value of
/// `QDMI_DEVICE_JOB_PROPERTY_ID`).
#[cfg(not(feature = "qdmi-v1-3"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ARVAK_QDMI_device_session_retrieve_device_job_by_id(
    session: *mut c_void,
    job_id: *const c_char,
    job_out: *mut *mut c_void,
) -> c_int {
    qdmi_guard!({
        let Some(gs) = state::get() else {
            return ffi::QDMI_ERROR_BADSTATE;
        };
        let Some(sess_id) = state::handle_to_id(session) else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };
        if job_id.is_null() || job_out.is_null() {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        }
        let Ok(job_id) = CStr::from_ptr(job_id).to_str() else {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        };
        if job_id.is_empty() {
            return ffi::QDMI_ERROR_INVALIDARGUMENT;
        }

        gs.runtime.block_on(async {
            {
                let mut sessions = gs.sessions.write().await;
                let Some(sess) = sessions.get_mut(&sess_id) else {
                    return ffi::QDMI_ERROR_INVALIDARGUMENT;
                };
                let Ok(client) = sess.client_mut() else {
                    return ffi::QDMI_ERROR_BADSTATE;
                };
                let req = Request::new(GetJobStatusRequest {
                    job_id: job_id.to_owned(),
                });
                if let Err(e) = client.get_job_status(req).await {
                    return match e.code() {
                        tonic::Code::NotFound => ffi::QDMI_ERROR_NOTFOUND,
                        tonic::Code::PermissionDenied | tonic::Code::Unauthenticated => {
                            ffi::QDMI_ERROR_PERMISSIONDENIED
                        }
                        _ => {
                            tracing::error!("retrieve_device_job_by_id gRPC error: {e}");
                            ffi::QDMI_ERROR_FATAL
                        }
                    };
                }
            }

            let Some(jid) = gs.alloc_job_id() else {
                return ffi::QDMI_ERROR_OUTOFMEM;
            };
            gs.jobs
                .write()
                .await
                .insert(jid, job::ArvakJob::retrieved(sess_id, job_id.to_owned()));
            *job_out = state::id_to_handle(jid);
            ffi::QDMI_SUCCESS
        })
    })
}
