//! Job types for the HPC scheduler.

use arvak_hal::JobId;
use arvak_ir::Circuit;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a scheduled job.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScheduledJobId(pub Uuid);

impl ScheduledJobId {
    /// Create a new random job ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a job ID from a UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Parse a job ID from a string.
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl Default for ScheduledJobId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ScheduledJobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Job priority. Higher values mean higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Priority(pub u32);

impl Priority {
    /// Default priority (100).
    pub const DEFAULT: Priority = Priority(100);

    /// Low priority (50).
    pub const LOW: Priority = Priority(50);

    /// High priority (150).
    pub const HIGH: Priority = Priority(150);

    /// Critical priority (200).
    pub const CRITICAL: Priority = Priority(200);

    /// Create a new priority with the given value.
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    /// Create a low priority.
    pub fn low() -> Self {
        Self::LOW
    }

    /// Create a default priority.
    pub fn default_priority() -> Self {
        Self::DEFAULT
    }

    /// Create a high priority.
    pub fn high() -> Self {
        Self::HIGH
    }

    /// Create a critical priority.
    pub fn critical() -> Self {
        Self::CRITICAL
    }

    /// Get the numeric value.
    pub fn value(&self) -> u32 {
        self.0
    }
}

impl Default for Priority {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Status of a scheduled job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduledJobStatus {
    /// Job is pending in the scheduler queue.
    Pending,

    /// Job is waiting for dependencies to complete.
    WaitingOnDependencies,

    /// Job has been submitted to SLURM and is queued.
    SlurmQueued { slurm_job_id: String },

    /// Job is running on SLURM.
    SlurmRunning { slurm_job_id: String },

    /// SLURM job completed, quantum job has been submitted.
    QuantumSubmitted {
        slurm_job_id: String,
        quantum_job_id: JobId,
    },

    /// Quantum job is running on the backend.
    QuantumRunning {
        slurm_job_id: String,
        quantum_job_id: JobId,
    },

    /// Job completed successfully.
    Completed {
        slurm_job_id: String,
        quantum_job_id: JobId,
    },

    /// Job failed.
    Failed {
        reason: String,
        slurm_job_id: Option<String>,
        quantum_job_id: Option<JobId>,
    },

    /// Job was cancelled.
    Cancelled,
}

impl ScheduledJobStatus {
    /// Check if the job is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ScheduledJobStatus::Completed { .. }
                | ScheduledJobStatus::Failed { .. }
                | ScheduledJobStatus::Cancelled
        )
    }

    /// Check if the job is running.
    pub fn is_running(&self) -> bool {
        matches!(
            self,
            ScheduledJobStatus::SlurmRunning { .. } | ScheduledJobStatus::QuantumRunning { .. }
        )
    }

    /// Check if the job is pending.
    pub fn is_pending(&self) -> bool {
        matches!(
            self,
            ScheduledJobStatus::Pending | ScheduledJobStatus::WaitingOnDependencies
        )
    }

    /// Check if the job completed successfully.
    pub fn is_success(&self) -> bool {
        matches!(self, ScheduledJobStatus::Completed { .. })
    }

    /// Get a human-readable status name.
    pub fn name(&self) -> &'static str {
        match self {
            ScheduledJobStatus::Pending => "Pending",
            ScheduledJobStatus::WaitingOnDependencies => "WaitingOnDependencies",
            ScheduledJobStatus::SlurmQueued { .. } => "SlurmQueued",
            ScheduledJobStatus::SlurmRunning { .. } => "SlurmRunning",
            ScheduledJobStatus::QuantumSubmitted { .. } => "QuantumSubmitted",
            ScheduledJobStatus::QuantumRunning { .. } => "QuantumRunning",
            ScheduledJobStatus::Completed { .. } => "Completed",
            ScheduledJobStatus::Failed { .. } => "Failed",
            ScheduledJobStatus::Cancelled => "Cancelled",
        }
    }

    /// Get the SLURM job ID if available.
    pub fn slurm_job_id(&self) -> Option<&str> {
        match self {
            ScheduledJobStatus::SlurmQueued { slurm_job_id }
            | ScheduledJobStatus::SlurmRunning { slurm_job_id }
            | ScheduledJobStatus::QuantumSubmitted { slurm_job_id, .. }
            | ScheduledJobStatus::QuantumRunning { slurm_job_id, .. }
            | ScheduledJobStatus::Completed { slurm_job_id, .. } => Some(slurm_job_id),
            ScheduledJobStatus::Failed { slurm_job_id, .. } => slurm_job_id.as_deref(),
            _ => None,
        }
    }

    /// Get the quantum job ID if available.
    pub fn quantum_job_id(&self) -> Option<&JobId> {
        match self {
            ScheduledJobStatus::QuantumSubmitted { quantum_job_id, .. }
            | ScheduledJobStatus::QuantumRunning { quantum_job_id, .. }
            | ScheduledJobStatus::Completed { quantum_job_id, .. } => Some(quantum_job_id),
            ScheduledJobStatus::Failed { quantum_job_id, .. } => quantum_job_id.as_ref(),
            _ => None,
        }
    }
}

impl std::fmt::Display for ScheduledJobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScheduledJobStatus::Pending => write!(f, "Pending"),
            ScheduledJobStatus::WaitingOnDependencies => write!(f, "Waiting on dependencies"),
            ScheduledJobStatus::SlurmQueued { slurm_job_id } => {
                write!(f, "SLURM queued ({slurm_job_id})")
            }
            ScheduledJobStatus::SlurmRunning { slurm_job_id } => {
                write!(f, "SLURM running ({slurm_job_id})")
            }
            ScheduledJobStatus::QuantumSubmitted {
                slurm_job_id,
                quantum_job_id,
            } => {
                write!(
                    f,
                    "Quantum submitted (SLURM: {}, Quantum: {})",
                    slurm_job_id, quantum_job_id.0
                )
            }
            ScheduledJobStatus::QuantumRunning {
                slurm_job_id,
                quantum_job_id,
            } => {
                write!(
                    f,
                    "Quantum running (SLURM: {}, Quantum: {})",
                    slurm_job_id, quantum_job_id.0
                )
            }
            ScheduledJobStatus::Completed {
                slurm_job_id,
                quantum_job_id,
            } => {
                write!(
                    f,
                    "Completed (SLURM: {}, Quantum: {})",
                    slurm_job_id, quantum_job_id.0
                )
            }
            ScheduledJobStatus::Failed { reason, .. } => write!(f, "Failed: {reason}"),
            ScheduledJobStatus::Cancelled => write!(f, "Cancelled"),
        }
    }
}

/// Topology preference for backend matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TopologyPreference {
    /// Prefer linear topology.
    Linear,
    /// Prefer grid topology.
    Grid,
    /// Prefer all-to-all connectivity.
    AllToAll,
    /// Require specific topology.
    Specific(String),
}

/// Resource requirements for a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    /// Minimum number of qubits required.
    pub min_qubits: u32,

    /// Topology preference.
    pub topology_preference: Option<TopologyPreference>,

    /// Allow running on simulator backends.
    pub allow_simulator: bool,

    /// Maximum time to wait in queue (seconds).
    pub max_queue_time: Option<u64>,

    /// Preferred backend names.
    pub preferred_backends: Vec<String>,

    /// Required gate set (gate names that must be supported).
    pub required_gates: Vec<String>,
}

impl Default for ResourceRequirements {
    fn default() -> Self {
        Self {
            min_qubits: 0,
            topology_preference: None,
            allow_simulator: true,
            max_queue_time: None,
            preferred_backends: Vec::new(),
            required_gates: Vec::new(),
        }
    }
}

impl ResourceRequirements {
    /// Create new resource requirements with the specified minimum qubit count.
    pub fn new(min_qubits: u32) -> Self {
        Self {
            min_qubits,
            ..Default::default()
        }
    }

    /// Set topology preference.
    pub fn with_topology(mut self, topology: TopologyPreference) -> Self {
        self.topology_preference = Some(topology);
        self
    }

    /// Disallow simulator backends.
    pub fn require_real_hardware(mut self) -> Self {
        self.allow_simulator = false;
        self
    }

    /// Set maximum queue time.
    pub fn with_max_queue_time(mut self, seconds: u64) -> Self {
        self.max_queue_time = Some(seconds);
        self
    }

    /// Add preferred backend.
    pub fn prefer_backend(mut self, name: impl Into<String>) -> Self {
        self.preferred_backends.push(name.into());
        self
    }

    /// Add required gate.
    pub fn require_gate(mut self, gate: impl Into<String>) -> Self {
        self.required_gates.push(gate.into());
        self
    }
}

/// Specification for a circuit to be executed.
///
/// Circuits are stored as QASM3 strings for serialization compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CircuitSpec {
    /// QASM3 source code.
    Qasm3(String),

    /// Path to QASM file.
    QasmFile(std::path::PathBuf),
}

impl CircuitSpec {
    /// Create a circuit spec from a circuit (converts to QASM3).
    pub fn from_circuit(circuit: &Circuit) -> crate::SchedResult<Self> {
        let qasm = arvak_qasm3::emit(circuit)?;
        Ok(CircuitSpec::Qasm3(qasm))
    }

    /// Create a circuit spec from QASM3 source.
    pub fn from_qasm(qasm: impl Into<String>) -> Self {
        CircuitSpec::Qasm3(qasm.into())
    }

    /// Create a circuit spec from a file path.
    pub fn from_file(path: impl Into<std::path::PathBuf>) -> Self {
        CircuitSpec::QasmFile(path.into())
    }

    /// Resolve the circuit spec to a circuit.
    pub fn resolve(&self) -> crate::SchedResult<Circuit> {
        match self {
            CircuitSpec::Qasm3(qasm) => Ok(arvak_qasm3::parse(qasm)?),
            CircuitSpec::QasmFile(path) => {
                let qasm = std::fs::read_to_string(path)?;
                Ok(arvak_qasm3::parse(&qasm)?)
            }
        }
    }

    /// Get the number of qubits in the circuit.
    // TODO: Cache parsed qubit count to avoid re-parsing on every call
    pub fn num_qubits(&self) -> crate::SchedResult<u32> {
        let circuit = self.resolve()?;
        Ok(circuit.num_qubits() as u32)
    }
}

/// A scheduled job in the HPC scheduler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    /// Unique job identifier.
    pub id: ScheduledJobId,

    /// Human-readable job name.
    pub name: String,

    /// Current job status.
    pub status: ScheduledJobStatus,

    /// Job priority.
    pub priority: Priority,

    /// Resource requirements.
    pub requirements: ResourceRequirements,

    /// Circuits to execute (supports batch).
    pub circuits: Vec<CircuitSpec>,

    /// Number of shots per circuit.
    pub shots: u32,

    /// Job dependencies (must complete before this job can run).
    pub dependencies: Vec<ScheduledJobId>,

    /// Matched backend name (set after resource matching).
    pub matched_backend: Option<String>,

    /// Job creation timestamp.
    pub created_at: DateTime<Utc>,

    /// Job submission timestamp (when submitted to SLURM).
    pub submitted_at: Option<DateTime<Utc>>,

    /// Job completion timestamp.
    pub completed_at: Option<DateTime<Utc>>,

    /// Arbitrary metadata.
    pub metadata: rustc_hash::FxHashMap<String, String>,
}

impl ScheduledJob {
    /// Create a new scheduled job with a single circuit.
    pub fn new(name: impl Into<String>, circuit: CircuitSpec) -> Self {
        Self {
            id: ScheduledJobId::new(),
            name: name.into(),
            status: ScheduledJobStatus::Pending,
            priority: Priority::default(),
            requirements: ResourceRequirements::default(),
            circuits: vec![circuit],
            shots: 1024,
            dependencies: Vec::new(),
            matched_backend: None,
            created_at: Utc::now(),
            submitted_at: None,
            completed_at: None,
            metadata: rustc_hash::FxHashMap::default(),
        }
    }

    /// Create a new batch job with multiple circuits.
    pub fn batch(name: impl Into<String>, circuits: Vec<CircuitSpec>) -> Self {
        Self {
            id: ScheduledJobId::new(),
            name: name.into(),
            status: ScheduledJobStatus::Pending,
            priority: Priority::default(),
            requirements: ResourceRequirements::default(),
            circuits,
            shots: 1024,
            dependencies: Vec::new(),
            matched_backend: None,
            created_at: Utc::now(),
            submitted_at: None,
            completed_at: None,
            metadata: rustc_hash::FxHashMap::default(),
        }
    }

    /// Set the job priority.
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Set the number of shots.
    pub fn with_shots(mut self, shots: u32) -> Self {
        self.shots = shots;
        self
    }

    /// Set resource requirements.
    pub fn with_requirements(mut self, requirements: ResourceRequirements) -> Self {
        self.requirements = requirements;
        self
    }

    /// Add a dependency on another job.
    pub fn depends_on(mut self, job_id: ScheduledJobId) -> Self {
        self.dependencies.push(job_id);
        self
    }

    /// Add multiple dependencies.
    pub fn depends_on_all(mut self, job_ids: impl IntoIterator<Item = ScheduledJobId>) -> Self {
        self.dependencies.extend(job_ids);
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Check if all dependencies are satisfied (given a set of completed job IDs).
    pub fn dependencies_satisfied(
        &self,
        completed: &rustc_hash::FxHashSet<ScheduledJobId>,
    ) -> bool {
        self.dependencies.iter().all(|dep| completed.contains(dep))
    }

    /// Get the maximum qubit count across all circuits.
    pub fn max_qubits(&self) -> crate::SchedResult<u32> {
        let mut max = 0;
        for circuit in &self.circuits {
            max = max.max(circuit.num_qubits()?);
        }
        Ok(max)
    }

    /// Check if this is a batch job.
    pub fn is_batch(&self) -> bool {
        self.circuits.len() > 1
    }
}

/// Filter for listing jobs.
#[derive(Debug, Clone, Default)]
pub struct JobFilter {
    /// Filter by status names.
    pub status: Option<Vec<String>>,

    /// Filter by name pattern.
    pub name_pattern: Option<String>,

    /// Filter by priority range.
    pub min_priority: Option<Priority>,
    pub max_priority: Option<Priority>,

    /// Filter by creation time range.
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,

    /// Include only pending jobs.
    pub pending_only: bool,

    /// Include only running jobs.
    pub running_only: bool,

    /// Maximum number of results.
    pub limit: Option<usize>,
}

impl JobFilter {
    /// Create a filter for pending jobs.
    pub fn pending() -> Self {
        Self {
            pending_only: true,
            ..Default::default()
        }
    }

    /// Create a filter for running jobs.
    pub fn running() -> Self {
        Self {
            running_only: true,
            ..Default::default()
        }
    }

    /// Filter by status.
    pub fn with_status(mut self, status: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.status = Some(status.into_iter().map(Into::into).collect());
        self
    }

    /// Limit results.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Check if a job matches this filter.
    pub fn matches(&self, job: &ScheduledJob) -> bool {
        // Check status filter
        if let Some(ref statuses) = self.status {
            if !statuses.contains(&job.status.name().to_string()) {
                return false;
            }
        }

        // Check pending only
        if self.pending_only && !job.status.is_pending() {
            return false;
        }

        // Check running only
        if self.running_only && !job.status.is_running() {
            return false;
        }

        // Check priority range
        if let Some(min) = self.min_priority {
            if job.priority < min {
                return false;
            }
        }
        if let Some(max) = self.max_priority {
            if job.priority > max {
                return false;
            }
        }

        // Check time range
        if let Some(after) = self.created_after {
            if job.created_at < after {
                return false;
            }
        }
        if let Some(before) = self.created_before {
            if job.created_at > before {
                return false;
            }
        }

        // Check name pattern
        if let Some(ref pattern) = self.name_pattern {
            if !job.name.contains(pattern) {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduled_job_id() {
        let id1 = ScheduledJobId::new();
        let id2 = ScheduledJobId::new();
        assert_ne!(id1, id2);

        let id_str = id1.to_string();
        let parsed = ScheduledJobId::parse(&id_str).unwrap();
        assert_eq!(id1, parsed);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::LOW < Priority::DEFAULT);
        assert!(Priority::DEFAULT < Priority::HIGH);
        assert!(Priority::HIGH < Priority::CRITICAL);
    }

    #[test]
    fn test_scheduled_job_status() {
        let pending = ScheduledJobStatus::Pending;
        assert!(pending.is_pending());
        assert!(!pending.is_terminal());
        assert!(!pending.is_running());

        let running = ScheduledJobStatus::SlurmRunning {
            slurm_job_id: "123".to_string(),
        };
        assert!(running.is_running());
        assert!(!running.is_terminal());
        assert_eq!(running.slurm_job_id(), Some("123"));

        let completed = ScheduledJobStatus::Completed {
            slurm_job_id: "123".to_string(),
            quantum_job_id: JobId("q-456".to_string()),
        };
        assert!(completed.is_terminal());
        assert!(completed.is_success());
    }

    #[test]
    fn test_scheduled_job_builder() {
        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q; h q[0]; cx q[0], q[1];");

        let job = ScheduledJob::new("test_job", circuit)
            .with_priority(Priority::high())
            .with_shots(2000)
            .with_metadata("user", "alice");

        assert_eq!(job.name, "test_job");
        assert_eq!(job.priority, Priority::HIGH);
        assert_eq!(job.shots, 2000);
        assert_eq!(job.metadata.get("user"), Some(&"alice".to_string()));
    }

    #[test]
    fn test_resource_requirements_builder() {
        let req = ResourceRequirements::new(5)
            .with_topology(TopologyPreference::Linear)
            .require_real_hardware()
            .prefer_backend("ibm_backend")
            .with_max_queue_time(3600);

        assert_eq!(req.min_qubits, 5);
        assert_eq!(req.topology_preference, Some(TopologyPreference::Linear));
        assert!(!req.allow_simulator);
        assert!(req.preferred_backends.contains(&"ibm_backend".to_string()));
        assert_eq!(req.max_queue_time, Some(3600));
    }

    #[test]
    fn test_job_filter() {
        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q;");
        let job = ScheduledJob::new("test", circuit);

        let filter = JobFilter::pending();
        assert!(filter.matches(&job));

        let filter = JobFilter::running();
        assert!(!filter.matches(&job));
    }
}
