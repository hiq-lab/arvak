//! SLURM adapter for job submission and tracking.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::fs;
use tokio::process::Command;

use crate::error::{SchedError, SchedResult};
use crate::job::ScheduledJob;
use crate::slurm::parser;
use crate::slurm::templates;

/// SLURM job state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlurmState {
    Pending,
    Running,
    Completing,
    Completed,
    Failed,
    Timeout,
    Cancelled,
    NodeFail,
    Preempted,
    OutOfMemory,
    Unknown(String),
}

impl SlurmState {
    /// Check if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            SlurmState::Completed
                | SlurmState::Failed
                | SlurmState::Timeout
                | SlurmState::Cancelled
                | SlurmState::NodeFail
                | SlurmState::OutOfMemory
        )
    }

    /// Check if this represents a successful completion.
    pub fn is_success(&self) -> bool {
        matches!(self, SlurmState::Completed)
    }
}

/// Information about a SLURM job.
#[derive(Debug, Clone)]
pub struct SlurmJobInfo {
    /// SLURM job ID.
    pub job_id: String,

    /// Job name.
    pub name: String,

    /// Current state.
    pub state: SlurmState,

    /// Reason for current state (e.g., "Resources" for pending).
    pub reason: Option<String>,

    /// Exit code (for completed jobs).
    pub exit_code: Option<i32>,
}

/// Configuration for SLURM adapter.
#[derive(Debug, Clone)]
pub struct SlurmConfig {
    /// SLURM partition to submit to.
    pub partition: String,

    /// SLURM account for billing.
    pub account: Option<String>,

    /// Time limit in minutes.
    pub time_limit: u32,

    /// Memory limit in MB.
    pub memory_mb: u32,

    /// Number of CPUs per task.
    pub cpus_per_task: u32,

    /// Working directory for job files.
    pub work_dir: PathBuf,

    /// Path to the HIQ binary.
    pub hiq_binary: PathBuf,

    /// Modules to load before running.
    pub modules: Vec<String>,

    /// Python virtual environment path.
    pub python_venv: Option<PathBuf>,

    /// Mapping from priority value to SLURM QOS.
    pub priority_qos_mapping: Option<rustc_hash::FxHashMap<u32, String>>,
}

impl Default for SlurmConfig {
    fn default() -> Self {
        Self {
            partition: "compute".to_string(),
            account: None,
            time_limit: 60,
            memory_mb: 4096,
            cpus_per_task: 1,
            work_dir: PathBuf::from("/tmp/hiq-jobs"),
            hiq_binary: PathBuf::from("hiq"),
            modules: Vec::new(),
            python_venv: None,
            priority_qos_mapping: None,
        }
    }
}

/// Adapter for SLURM HPC scheduler.
pub struct SlurmAdapter {
    config: SlurmConfig,
    /// Whether to use mock mode (for testing).
    mock_mode: bool,
    /// Mock job counter for generating fake job IDs.
    mock_counter: std::sync::atomic::AtomicU64,
}

impl SlurmAdapter {
    /// Create a new SLURM adapter with the given configuration.
    pub async fn new(config: SlurmConfig) -> SchedResult<Self> {
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

    /// Create a new SLURM adapter in mock mode (for testing).
    pub fn mock(config: SlurmConfig) -> Self {
        Self {
            config,
            mock_mode: true,
            mock_counter: std::sync::atomic::AtomicU64::new(1000),
        }
    }

    /// Submit a job to SLURM.
    pub async fn submit(&self, job: &ScheduledJob) -> SchedResult<String> {
        // In mock mode, skip file I/O
        if self.mock_mode {
            let job_id = self
                .mock_counter
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Ok(job_id.to_string());
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
            templates::generate_batch_script(job, &self.config, &circuit_files[0], &result_file)
        } else {
            let result_dir = self
                .config
                .work_dir
                .join("results")
                .join(job.id.to_string());
            let circuit_refs: Vec<&Path> = circuit_files.iter().map(|p| p.as_path()).collect();
            templates::generate_batch_script_multi(job, &self.config, &circuit_refs, &result_dir)
        };

        // Write batch script
        let script_path = self
            .config
            .work_dir
            .join("scripts")
            .join(format!("{}.sh", job.id));
        fs::write(&script_path, &script).await?;

        // Submit via sbatch
        self.run_sbatch(&script_path).await
    }

    /// Get the status of a SLURM job.
    pub async fn status(&self, slurm_job_id: &str) -> SchedResult<SlurmJobInfo> {
        if self.mock_mode {
            return Ok(SlurmJobInfo {
                job_id: slurm_job_id.to_string(),
                name: "mock_job".to_string(),
                state: SlurmState::Completed,
                reason: None,
                exit_code: Some(0),
            });
        }

        // First try squeue (for pending/running jobs)
        if let Some(info) = self.run_squeue(slurm_job_id).await? {
            return Ok(info);
        }

        // If not found in squeue, check sacct (for completed jobs)
        if let Some(info) = self.run_sacct(slurm_job_id).await? {
            return Ok(info);
        }

        Err(SchedError::SlurmJobNotFound(slurm_job_id.to_string()))
    }

    /// Cancel a SLURM job.
    pub async fn cancel(&self, slurm_job_id: &str) -> SchedResult<()> {
        if self.mock_mode {
            return Ok(());
        }

        let output = Command::new("scancel")
            .arg(slurm_job_id)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| SchedError::SlurmCommandError {
                command: "scancel".to_string(),
                message: e.to_string(),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        parser::parse_scancel_output(&stdout, &stderr)
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
            let qasm = hiq_qasm3::emit(&circuit)?;

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

    /// Run sbatch command.
    async fn run_sbatch(&self, script_path: &Path) -> SchedResult<String> {
        let output = Command::new("sbatch")
            .arg(script_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| SchedError::SlurmCommandError {
                command: "sbatch".to_string(),
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SchedError::SlurmSubmitError(stderr.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parser::parse_sbatch_output(&stdout)
    }

    /// Run squeue command to get job status.
    async fn run_squeue(&self, slurm_job_id: &str) -> SchedResult<Option<SlurmJobInfo>> {
        let output = Command::new("squeue")
            .args(["-j", slurm_job_id, "-o", "%i|%j|%T|%r|%S"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| SchedError::SlurmCommandError {
                command: "squeue".to_string(),
                message: e.to_string(),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        parser::parse_squeue_output(&stdout)
    }

    /// Run sacct command to get completed job status.
    async fn run_sacct(&self, slurm_job_id: &str) -> SchedResult<Option<SlurmJobInfo>> {
        let output = Command::new("sacct")
            .args([
                "-j",
                slurm_job_id,
                "-o",
                "JobID,JobName,State,ExitCode",
                "-P",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| SchedError::SlurmCommandError {
                command: "sacct".to_string(),
                message: e.to_string(),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        parser::parse_sacct_output(&stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{CircuitSpec, Priority};

    #[tokio::test]
    async fn test_mock_slurm_adapter() {
        let config = SlurmConfig::default();
        let adapter = SlurmAdapter::mock(config);

        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q; h q[0]; cx q[0], q[1];");
        let job = ScheduledJob::new("test_job", circuit).with_priority(Priority::default());

        // Submit
        let slurm_job_id = adapter.submit(&job).await.unwrap();
        assert!(slurm_job_id.parse::<u64>().is_ok());

        // Status
        let info = adapter.status(&slurm_job_id).await.unwrap();
        assert_eq!(info.job_id, slurm_job_id);
        assert!(info.state.is_success());

        // Cancel
        adapter.cancel(&slurm_job_id).await.unwrap();
    }

    #[test]
    fn test_slurm_state() {
        assert!(SlurmState::Completed.is_terminal());
        assert!(SlurmState::Failed.is_terminal());
        assert!(SlurmState::Cancelled.is_terminal());
        assert!(!SlurmState::Pending.is_terminal());
        assert!(!SlurmState::Running.is_terminal());

        assert!(SlurmState::Completed.is_success());
        assert!(!SlurmState::Failed.is_success());
    }
}
