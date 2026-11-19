// SPDX-License-Identifier: Apache-2.0
//! Job state for a QDMI device job.

use std::ffi::c_int;

/// A QDMI job backed by an Arvak gRPC job.
///
/// Arvak maps one QDMI job to one gRPC job, so a job holds at most one
/// program; QDMI 1.4 multi-program lists are rejected with `NOTSUPPORTED`.
pub struct ArvakJob {
    /// Session this job belongs to.
    pub session_id: usize,
    /// Program format (QDMI 1.4 `set_programs`, or 1.3 `PROGRAMFORMAT`).
    pub format: Option<c_int>,
    /// QASM3 program text, without the trailing NUL (QDMI 1.4
    /// `set_programs`, or `job_set_parameter(PROGRAM)` on 1.3).
    pub program: Option<String>,
    /// Number of shots (set via `job_set_parameter(SHOTSNUM)`, default 1024).
    pub shots: u32,
    /// Remote job ID from Arvak gRPC (set after `job_submit`).
    pub grpc_job_id: Option<String>,
    /// Re-attached via `retrieve_device_job_by_id`: read-only, program unknown.
    pub retrieved: bool,
    /// Cached results: sorted `(bitstring, count)` pairs.
    pub cached_results: Option<Vec<(String, u64)>>,
}

impl ArvakJob {
    pub fn new(session_id: usize) -> Self {
        Self {
            session_id,
            format: None,
            program: None,
            shots: 1024,
            grpc_job_id: None,
            retrieved: false,
            cached_results: None,
        }
    }

    /// A handle for an already submitted remote job.
    #[cfg(not(feature = "qdmi-v1-3"))]
    pub fn retrieved(session_id: usize, grpc_job_id: String) -> Self {
        Self {
            grpc_job_id: Some(grpc_job_id),
            retrieved: true,
            ..Self::new(session_id)
        }
    }

    /// Number of programs: one once set (or for a retrieved job), else zero.
    pub fn num_programs(&self) -> usize {
        usize::from(self.program.is_some() || self.retrieved)
    }
}
