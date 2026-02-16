//! PBS adapter for job submission and tracking.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::fs;
use tokio::process::Command;

use crate::error::{SchedError, SchedResult};
use crate::job::ScheduledJob;
use crate::pbs::parser;
use crate::pbs::templates;

/// PBS job state.
///
/// PBS uses single-letter state codes:
/// - Q: Queued (waiting in queue)
/// - R: Running
/// - E: Exiting (job completing)
/// - C: Completed
/// - H: Held
/// - W: Waiting (delayed start)
/// - S: Suspended
/// - T: Being moved to new location
/// - B: Array job has at least one subjob running
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PbsState {
    /// Job is queued and waiting for resources.
    Queued,
    /// Job is currently running.
    Running,
    /// Job is exiting (finishing up).
    Exiting,
    /// Job has completed.
    Completed,
    /// Job is held and will not run until released.
    Held,
    /// Job is waiting for scheduled start time.
    Waiting,
    /// Job has been suspended.
    Suspended,
    /// Job is being moved to another location.
    Transit,
    /// Array job with subjobs running.
    ArrayRunning,
    /// Job failed.
    Failed,
    /// Unknown state.
    Unknown(String),
}

impl PbsState {
    /// Check if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, PbsState::Completed | PbsState::Failed)
    }

    /// Check if this represents a successful completion.
    pub fn is_success(&self) -> bool {
        matches!(self, PbsState::Completed)
    }

    /// Convert to state code character.
    pub fn to_code(&self) -> &'static str {
        match self {
            PbsState::Queued => "Q",
            PbsState::Running => "R",
            PbsState::Exiting => "E",
            PbsState::Completed => "C",
            PbsState::Held => "H",
            PbsState::Waiting => "W",
            PbsState::Suspended => "S",
            PbsState::Transit => "T",
            PbsState::ArrayRunning => "B",
            PbsState::Failed => "F",
            PbsState::Unknown(_) => "?",
        }
    }
}

/// Information about a PBS job.
#[derive(Debug, Clone)]
pub struct PbsJobInfo {
    /// PBS job ID (e.g., "12345.pbs-server").
    pub job_id: String,

    /// Job name.
    pub name: String,

    /// Current state.
    pub state: PbsState,

    /// Queue the job is in.
    pub queue: Option<String>,

    /// Exit status (for completed jobs).
    pub exit_status: Option<i32>,

    /// Wall time used.
    pub walltime_used: Option<String>,

    /// Resources used (CPU time, memory, etc.).
    pub resources_used: Option<PbsResourcesUsed>,
}

/// Resources used by a PBS job.
#[derive(Debug, Clone, Default)]
pub struct PbsResourcesUsed {
    /// CPU time used.
    pub cput: Option<String>,
    /// Memory used.
    pub mem: Option<String>,
    /// Virtual memory used.
    pub vmem: Option<String>,
    /// Wall time used.
    pub walltime: Option<String>,
}

/// Configuration for PBS adapter.
#[derive(Debug, Clone)]
pub struct PbsConfig {
    /// PBS queue to submit to.
    pub queue: String,

    /// Account string for job accounting.
    pub account: Option<String>,

    /// Walltime limit (format: HH:MM:SS).
    pub walltime: String,

    /// Memory limit (e.g., "4gb", "4096mb").
    pub memory: String,

    /// Number of nodes.
    pub nodes: u32,

    /// Processors per node (ppn).
    pub ppn: u32,

    /// Working directory for job files.
    pub work_dir: PathBuf,

    /// Path to the Arvak binary.
    pub arvak_binary: PathBuf,

    /// Modules to load before running.
    pub modules: Vec<String>,

    /// Python virtual environment path.
    pub python_venv: Option<PathBuf>,

    /// PBS server hostname (optional, for job ID parsing).
    pub server: Option<String>,

    /// Additional PBS directives.
    pub extra_directives: Vec<String>,

    /// Mapping from priority value to PBS queue names.
    pub priority_queue_mapping: Option<rustc_hash::FxHashMap<u32, String>>,
}

impl Default for PbsConfig {
    fn default() -> Self {
        Self {
            queue: "batch".to_string(),
            account: None,
            walltime: "01:00:00".to_string(),
            memory: "4gb".to_string(),
            nodes: 1,
            ppn: 1,
            work_dir: PathBuf::from("/tmp/arvak-jobs"),
            arvak_binary: PathBuf::from("arvak"),
            modules: Vec::new(),
            python_venv: None,
            server: None,
            extra_directives: Vec::new(),
            priority_queue_mapping: None,
        }
    }
}

/// Adapter for PBS HPC scheduler.
pub struct PbsAdapter {
    config: PbsConfig,
    /// Whether to use mock mode (for testing).
    mock_mode: bool,
    /// Mock job counter for generating fake job IDs.
    mock_counter: std::sync::atomic::AtomicU64,
}

impl PbsAdapter {
    /// Create a new PBS adapter with the given configuration.
    pub async fn new(config: PbsConfig) -> SchedResult<Self> {
        // Ensure work directory exists
        fs::create_dir_all(&config.work_dir).await?;
        fs::create_dir_all(config.work_dir.join("scripts")).await?;
        fs::create_dir_all(config.work_dir.join("circuits")).await?;
        fs::create_dir_all(config.work_dir.join("results")).await?;

        Ok(Self {
            config,
            mock_mode: false,
            mock_counter: std::sync::atomic::AtomicU64::new(1000),
        })
    }

    /// Create a new PBS adapter in mock mode (for testing).
    pub fn mock(config: PbsConfig) -> Self {
        Self {
            config,
            mock_mode: true,
            mock_counter: std::sync::atomic::AtomicU64::new(1000),
        }
    }

    /// Get the configuration.
    pub fn config(&self) -> &PbsConfig {
        &self.config
    }

    /// Submit a job to PBS.
    pub async fn submit(&self, job: &ScheduledJob) -> SchedResult<String> {
        // In mock mode, skip file I/O
        if self.mock_mode {
            let job_id = self
                .mock_counter
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let server = self.config.server.as_deref().unwrap_or("pbs-server");
            return Ok(format!("{job_id}.{server}"));
        }

        // Write circuit(s) to file
        let circuit_files = self.write_circuits(job).await?;

        // Generate batch script
        let script = if circuit_files.len() == 1 {
            let result_file = self
                .config
                .work_dir
                .join("results")
                .join(format!("{}.json", job.id));
            templates::generate_pbs_script(job, &self.config, &circuit_files[0], &result_file)?
        } else {
            let result_dir = self
                .config
                .work_dir
                .join("results")
                .join(job.id.to_string());
            let circuit_refs: Vec<&Path> = circuit_files
                .iter()
                .map(std::path::PathBuf::as_path)
                .collect();
            templates::generate_pbs_script_multi(job, &self.config, &circuit_refs, &result_dir)?
        };

        // Write batch script
        let script_path = self
            .config
            .work_dir
            .join("scripts")
            .join(format!("{}.pbs", job.id));
        fs::write(&script_path, &script).await?;

        // Submit via qsub
        self.run_qsub(&script_path).await
    }

    /// Get the status of a PBS job.
    pub async fn status(&self, pbs_job_id: &str) -> SchedResult<PbsJobInfo> {
        if self.mock_mode {
            return Ok(PbsJobInfo {
                job_id: pbs_job_id.to_string(),
                name: "mock_job".to_string(),
                state: PbsState::Completed,
                queue: Some(self.config.queue.clone()),
                exit_status: Some(0),
                walltime_used: None,
                resources_used: None,
            });
        }

        // First try qstat (for pending/running jobs)
        if let Some(info) = self.run_qstat(pbs_job_id).await? {
            // Job found in qstat (still active)
            return Ok(info);
        }

        // If not found in qstat, check qstat -xf for finished jobs
        if let Some(info) = self.run_qstat_finished(pbs_job_id).await? {
            return Ok(info);
        }

        Err(SchedError::PbsJobNotFound(pbs_job_id.to_string()))
    }

    /// Cancel a PBS job.
    pub async fn cancel(&self, pbs_job_id: &str) -> SchedResult<()> {
        if self.mock_mode {
            return Ok(());
        }

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Command::new("qdel")
                .arg(pbs_job_id)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .map_err(|_| SchedError::Timeout("qdel timed out after 30s".into()))?
        .map_err(|e| SchedError::PbsCommandError {
            command: "qdel".to_string(),
            message: e.to_string(),
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        parser::parse_qdel_output(&stdout, &stderr)
    }

    /// Get the result file path for a job.
    pub fn result_path(&self, job: &ScheduledJob) -> PathBuf {
        if job.is_batch() {
            self.config
                .work_dir
                .join("results")
                .join(job.id.to_string())
        } else {
            self.config
                .work_dir
                .join("results")
                .join(format!("{}.json", job.id))
        }
    }

    /// Write circuit files for a job.
    async fn write_circuits(&self, job: &ScheduledJob) -> SchedResult<Vec<PathBuf>> {
        let mut paths = Vec::with_capacity(job.circuits.len());

        for (i, spec) in job.circuits.iter().enumerate() {
            let circuit = spec.resolve()?;
            let qasm = arvak_qasm3::emit(&circuit)?;

            let filename = if job.circuits.len() == 1 {
                format!("{}.qasm", job.id)
            } else {
                format!("{}_{}.qasm", job.id, i)
            };

            let path = self.config.work_dir.join("circuits").join(filename);
            fs::write(&path, qasm).await?;
            paths.push(path);
        }

        Ok(paths)
    }

    /// Run qsub command.
    async fn run_qsub(&self, script_path: &Path) -> SchedResult<String> {
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            Command::new("qsub")
                .arg(script_path)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .map_err(|_| SchedError::Timeout("qsub timed out after 60s".into()))?
        .map_err(|e| SchedError::PbsCommandError {
            command: "qsub".to_string(),
            message: e.to_string(),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SchedError::PbsSubmitError(stderr.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parser::parse_qsub_output(&stdout)
    }

    /// Run qstat command to get job status.
    async fn run_qstat(&self, pbs_job_id: &str) -> SchedResult<Option<PbsJobInfo>> {
        // Use qstat -f for full output in parseable format
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Command::new("qstat")
                .args(["-f", pbs_job_id])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .map_err(|_| SchedError::Timeout("qstat timed out after 30s".into()))?
        .map_err(|e| SchedError::PbsCommandError {
            command: "qstat".to_string(),
            message: e.to_string(),
        })?;

        // Check for "Unknown Job Id" error
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("Unknown Job Id") || stderr.contains("does not exist") {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parser::parse_qstat_full_output(&stdout)
    }

    /// Run qstat -x to get finished job status.
    async fn run_qstat_finished(&self, pbs_job_id: &str) -> SchedResult<Option<PbsJobInfo>> {
        // qstat -xf shows finished jobs too (PBS Pro feature)
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Command::new("qstat")
                .args(["-xf", pbs_job_id])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .map_err(|_| SchedError::Timeout("qstat -xf timed out after 30s".into()))?
        .map_err(|e| SchedError::PbsCommandError {
            command: "qstat".to_string(),
            message: e.to_string(),
        })?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("Unknown Job Id") || stderr.contains("does not exist") {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parser::parse_qstat_full_output(&stdout)
    }

    /// Hold a job (prevent it from running).
    pub async fn hold(&self, pbs_job_id: &str) -> SchedResult<()> {
        if self.mock_mode {
            return Ok(());
        }

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Command::new("qhold")
                .arg(pbs_job_id)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .map_err(|_| SchedError::Timeout("qhold timed out after 30s".into()))?
        .map_err(|e| SchedError::PbsCommandError {
            command: "qhold".to_string(),
            message: e.to_string(),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SchedError::PbsCommandError {
                command: "qhold".to_string(),
                message: stderr.to_string(),
            });
        }

        Ok(())
    }

    /// Release a held job.
    pub async fn release(&self, pbs_job_id: &str) -> SchedResult<()> {
        if self.mock_mode {
            return Ok(());
        }

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Command::new("qrls")
                .arg(pbs_job_id)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .map_err(|_| SchedError::Timeout("qrls timed out after 30s".into()))?
        .map_err(|e| SchedError::PbsCommandError {
            command: "qrls".to_string(),
            message: e.to_string(),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SchedError::PbsCommandError {
                command: "qrls".to_string(),
                message: stderr.to_string(),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{CircuitSpec, Priority};

    #[tokio::test]
    async fn test_mock_pbs_adapter() {
        let config = PbsConfig::default();
        let adapter = PbsAdapter::mock(config);

        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q; h q[0]; cx q[0], q[1];");
        let job = ScheduledJob::new("test_job", circuit).with_priority(Priority::default());

        // Submit
        let pbs_job_id = adapter.submit(&job).await.unwrap();
        assert!(pbs_job_id.contains(".pbs-server"));

        // Status
        let info = adapter.status(&pbs_job_id).await.unwrap();
        assert_eq!(info.job_id, pbs_job_id);
        assert!(info.state.is_success());

        // Cancel
        adapter.cancel(&pbs_job_id).await.unwrap();
    }

    #[test]
    fn test_pbs_state() {
        assert!(PbsState::Completed.is_terminal());
        assert!(PbsState::Failed.is_terminal());
        assert!(!PbsState::Queued.is_terminal());
        assert!(!PbsState::Running.is_terminal());
        assert!(!PbsState::Held.is_terminal());

        assert!(PbsState::Completed.is_success());
        assert!(!PbsState::Failed.is_success());
        assert!(!PbsState::Queued.is_success());
    }

    #[test]
    fn test_pbs_state_codes() {
        assert_eq!(PbsState::Queued.to_code(), "Q");
        assert_eq!(PbsState::Running.to_code(), "R");
        assert_eq!(PbsState::Completed.to_code(), "C");
        assert_eq!(PbsState::Held.to_code(), "H");
    }

    #[test]
    fn test_pbs_config_default() {
        let config = PbsConfig::default();
        assert_eq!(config.queue, "batch");
        assert_eq!(config.walltime, "01:00:00");
        assert_eq!(config.memory, "4gb");
        assert_eq!(config.nodes, 1);
        assert_eq!(config.ppn, 1);
    }
}
