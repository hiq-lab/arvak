//! A test backend that rejects circuits containing the `h` (Hadamard) gate.
//!
//! This catches missing compilation: the `h` gate is never a native gate on any
//! real backend (IQM uses prx, IBM uses sx+rz). If `h` survives to submission,
//! compilation was skipped. The Optimize1qGates pass uses ZYZ decomposition
//! (Rz·Ry·Rz) which means the compiler's actual output contains ry/rz/cx/cz/prx —
//! NOT just the target basis gates. This backend accepts all those but rejects `h`.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use arvak_hal::backend::{Backend, BackendAvailability, ValidationResult};
use arvak_hal::capability::Capabilities;
use arvak_hal::error::{HalError, HalResult};
use arvak_hal::job::{JobId, JobStatus};
use arvak_hal::result::{Counts, ExecutionResult};
use arvak_ir::circuit::Circuit;

use async_trait::async_trait;

/// Test backend that rejects `h` gates.
///
/// The `h` gate is always decomposed by the compiler (into prx for IQM, or
/// sx+rz for IBM). If `h` appears in a submitted circuit, it means compilation
/// was skipped — exactly the class of bug this backend exists to catch.
pub struct StrictBackend {
    rejected_gates: HashSet<String>,
    capabilities: Capabilities,
    next_job_id: AtomicU64,
}

impl Default for StrictBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl StrictBackend {
    pub fn new() -> Self {
        // Gates that should NEVER appear after compilation
        let rejected_gates: HashSet<String> = ["h"].iter().map(|s| (*s).to_string()).collect();

        let capabilities = Capabilities::iqm("strict-test", 5);

        Self {
            rejected_gates,
            capabilities,
            next_job_id: AtomicU64::new(0),
        }
    }

    /// Check that no rejected gates are present in the circuit.
    fn validate_gates(&self, circuit: &Circuit) -> Result<(), Vec<String>> {
        let dag = circuit.dag();
        let mut rejected = Vec::new();

        for (_idx, inst) in dag.topological_ops() {
            let name = inst.name();
            if self.rejected_gates.contains(name) {
                rejected.push(name.to_string());
            }
        }

        if rejected.is_empty() {
            Ok(())
        } else {
            rejected.sort();
            rejected.dedup();
            Err(rejected)
        }
    }
}

#[async_trait]
impl Backend for StrictBackend {
    #[allow(clippy::unnecessary_literal_bound)]
    fn name(&self) -> &str {
        "strict-test"
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    async fn availability(&self) -> HalResult<BackendAvailability> {
        Ok(BackendAvailability::always_available())
    }

    async fn validate(&self, circuit: &Circuit) -> HalResult<ValidationResult> {
        match self.validate_gates(circuit) {
            Ok(()) => Ok(ValidationResult::Valid),
            Err(rejected) => Ok(ValidationResult::Invalid {
                reasons: rejected
                    .iter()
                    .map(|g| format!("Unsupported gate: {g}"))
                    .collect(),
            }),
        }
    }

    async fn submit(&self, circuit: &Circuit, shots: u32) -> HalResult<JobId> {
        // Validate before accepting
        if let Err(rejected) = self.validate_gates(circuit) {
            return Err(HalError::InvalidCircuit(format!(
                "Unsupported gates: {}",
                rejected.join(", ")
            )));
        }

        let id = self.next_job_id.fetch_add(1, Ordering::Relaxed);
        let _ = shots;
        Ok(JobId::new(format!("strict-{id}")))
    }

    async fn status(&self, _job_id: &JobId) -> HalResult<JobStatus> {
        Ok(JobStatus::Completed)
    }

    async fn result(&self, _job_id: &JobId) -> HalResult<ExecutionResult> {
        let counts = Counts::from_pairs(vec![("00000", 1024)]);
        Ok(ExecutionResult::new(counts, 1024))
    }

    async fn cancel(&self, _job_id: &JobId) -> HalResult<()> {
        Ok(())
    }

    async fn wait(&self, _job_id: &JobId) -> HalResult<ExecutionResult> {
        self.result(_job_id).await
    }
}
