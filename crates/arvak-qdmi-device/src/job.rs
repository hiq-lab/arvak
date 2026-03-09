// SPDX-License-Identifier: Apache-2.0
//! Job state for a QDMI device job.

/// A QDMI job backed by an Arvak gRPC job.
pub struct ArvakJob {
    /// Session this job belongs to.
    pub session_id: usize,
    /// QASM3 program text (set via `job_set_parameter(PROGRAM)`).
    pub program: Option<String>,
    /// Number of shots (set via `job_set_parameter(SHOTSNUM)`, default 1024).
    pub shots: u32,
    /// Remote job ID from Arvak gRPC (set after `job_submit`).
    pub grpc_job_id: Option<String>,
    /// Cached results: sorted `(bitstring, count)` pairs.
    pub cached_results: Option<Vec<(String, u64)>>,
}

impl ArvakJob {
    pub fn new(session_id: usize) -> Self {
        Self {
            session_id,
            program: None,
            shots: 1024,
            grpc_job_id: None,
            cached_results: None,
        }
    }
}
