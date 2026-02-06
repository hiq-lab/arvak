//! Parsers for SLURM command output.

use crate::error::{SchedError, SchedResult};
use crate::slurm::adapter::{SlurmJobInfo, SlurmState};

/// Parse sbatch output to extract job ID.
///
/// sbatch output format: "Submitted batch job 12345"
pub fn parse_sbatch_output(output: &str) -> SchedResult<String> {
    let trimmed = output.trim();

    // Check for "Submitted batch job <ID>" format
    if let Some(rest) = trimmed.strip_prefix("Submitted batch job ") {
        let job_id = rest.trim();
        if !job_id.is_empty() && job_id.chars().all(|c| c.is_ascii_digit()) {
            return Ok(job_id.to_string());
        }
    }

    Err(SchedError::SlurmCommandError {
        command: "sbatch".to_string(),
        message: format!("Unexpected output format: {}", trimmed),
    })
}

/// Parse squeue output to extract job information.
///
/// Expected format (from `squeue -j <id> -o "%i|%j|%T|%r|%S"`):
/// JOBID|NAME|STATE|REASON|START_TIME
/// 12345|job_name|RUNNING|None|2024-01-15T10:30:00
pub fn parse_squeue_output(output: &str) -> SchedResult<Option<SlurmJobInfo>> {
    let lines: Vec<&str> = output.lines().collect();

    // Skip header line
    if lines.len() < 2 {
        return Ok(None);
    }

    let data_line = lines[1].trim();
    if data_line.is_empty() {
        return Ok(None);
    }

    let parts: Vec<&str> = data_line.split('|').collect();
    if parts.len() < 4 {
        return Err(SchedError::SlurmCommandError {
            command: "squeue".to_string(),
            message: format!("Unexpected output format: {}", data_line),
        });
    }

    let job_id = parts[0].trim().to_string();
    let name = parts[1].trim().to_string();
    let state = parse_slurm_state(parts[2].trim());
    let reason = if parts[3].trim() == "None" {
        None
    } else {
        Some(parts[3].trim().to_string())
    };

    Ok(Some(SlurmJobInfo {
        job_id,
        name,
        state,
        reason,
        exit_code: None,
    }))
}

/// Parse sacct output for completed job information.
///
/// Expected format (from `sacct -j <id> -o JobID,JobName,State,ExitCode -P`):
/// JobID|JobName|State|ExitCode
/// 12345|job_name|COMPLETED|0:0
/// 12345.batch|batch|COMPLETED|0:0
pub fn parse_sacct_output(output: &str) -> SchedResult<Option<SlurmJobInfo>> {
    let lines: Vec<&str> = output.lines().collect();

    // Skip header line
    if lines.len() < 2 {
        return Ok(None);
    }

    // Find the main job line (not .batch or .extern)
    for line in &lines[1..] {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 4 {
            continue;
        }

        let job_id = parts[0].trim();
        // Skip sub-jobs (e.g., "12345.batch", "12345.extern")
        if job_id.contains('.') {
            continue;
        }

        let name = parts[1].trim().to_string();
        let state = parse_slurm_state(parts[2].trim());
        let exit_code = parse_exit_code(parts[3].trim());

        return Ok(Some(SlurmJobInfo {
            job_id: job_id.to_string(),
            name,
            state,
            reason: None,
            exit_code,
        }));
    }

    Ok(None)
}

/// Parse SLURM state string.
fn parse_slurm_state(state: &str) -> SlurmState {
    match state.to_uppercase().as_str() {
        "PENDING" | "PD" => SlurmState::Pending,
        "RUNNING" | "R" => SlurmState::Running,
        "COMPLETING" | "CG" => SlurmState::Completing,
        "COMPLETED" | "CD" => SlurmState::Completed,
        "FAILED" | "F" => SlurmState::Failed,
        "TIMEOUT" | "TO" => SlurmState::Timeout,
        "CANCELLED" | "CA" => SlurmState::Cancelled,
        "NODE_FAIL" | "NF" => SlurmState::NodeFail,
        "PREEMPTED" | "PR" => SlurmState::Preempted,
        "OUT_OF_MEMORY" | "OOM" => SlurmState::OutOfMemory,
        _ => SlurmState::Unknown(state.to_string()),
    }
}

/// Parse exit code from SLURM format "exit_code:signal".
fn parse_exit_code(code: &str) -> Option<i32> {
    let parts: Vec<&str> = code.split(':').collect();
    parts.first().and_then(|s| s.parse().ok())
}

/// Parse scancel output to verify cancellation.
pub fn parse_scancel_output(output: &str, stderr: &str) -> SchedResult<()> {
    // scancel typically produces no output on success
    // Check for error messages in stderr
    if stderr.contains("Invalid job id") || stderr.contains("does not exist") {
        return Err(SchedError::SlurmJobNotFound(
            "Job not found or already completed".to_string(),
        ));
    }

    if !stderr.is_empty() && !stderr.contains("already completing") {
        return Err(SchedError::SlurmCommandError {
            command: "scancel".to_string(),
            message: stderr.to_string(),
        });
    }

    // Empty output or "already completing" is OK
    let _ = output;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sbatch_output() {
        let output = "Submitted batch job 12345\n";
        let job_id = parse_sbatch_output(output).unwrap();
        assert_eq!(job_id, "12345");

        let output = "Submitted batch job 9999999";
        let job_id = parse_sbatch_output(output).unwrap();
        assert_eq!(job_id, "9999999");
    }

    #[test]
    fn test_parse_sbatch_output_error() {
        let output = "Error: some error message";
        assert!(parse_sbatch_output(output).is_err());
    }

    #[test]
    fn test_parse_squeue_output() {
        let output =
            "JOBID|NAME|STATE|REASON|START_TIME\n12345|my_job|RUNNING|None|2024-01-15T10:30:00\n";
        let info = parse_squeue_output(output).unwrap().unwrap();
        assert_eq!(info.job_id, "12345");
        assert_eq!(info.name, "my_job");
        assert!(matches!(info.state, SlurmState::Running));
        assert!(info.reason.is_none());

        let output = "JOBID|NAME|STATE|REASON|START_TIME\n12345|my_job|PENDING|Resources|N/A\n";
        let info = parse_squeue_output(output).unwrap().unwrap();
        assert!(matches!(info.state, SlurmState::Pending));
        assert_eq!(info.reason, Some("Resources".to_string()));
    }

    #[test]
    fn test_parse_squeue_output_empty() {
        let output = "JOBID|NAME|STATE|REASON|START_TIME\n";
        let info = parse_squeue_output(output).unwrap();
        assert!(info.is_none());
    }

    #[test]
    fn test_parse_sacct_output() {
        let output = "JobID|JobName|State|ExitCode\n12345|my_job|COMPLETED|0:0\n12345.batch|batch|COMPLETED|0:0\n";
        let info = parse_sacct_output(output).unwrap().unwrap();
        assert_eq!(info.job_id, "12345");
        assert_eq!(info.name, "my_job");
        assert!(matches!(info.state, SlurmState::Completed));
        assert_eq!(info.exit_code, Some(0));
    }

    #[test]
    fn test_parse_sacct_output_failed() {
        let output = "JobID|JobName|State|ExitCode\n12345|my_job|FAILED|1:0\n";
        let info = parse_sacct_output(output).unwrap().unwrap();
        assert!(matches!(info.state, SlurmState::Failed));
        assert_eq!(info.exit_code, Some(1));
    }

    #[test]
    fn test_parse_slurm_state() {
        assert!(matches!(parse_slurm_state("PENDING"), SlurmState::Pending));
        assert!(matches!(parse_slurm_state("PD"), SlurmState::Pending));
        assert!(matches!(parse_slurm_state("RUNNING"), SlurmState::Running));
        assert!(matches!(parse_slurm_state("R"), SlurmState::Running));
        assert!(matches!(
            parse_slurm_state("COMPLETED"),
            SlurmState::Completed
        ));
        assert!(matches!(parse_slurm_state("FAILED"), SlurmState::Failed));
        assert!(matches!(
            parse_slurm_state("CANCELLED"),
            SlurmState::Cancelled
        ));
        assert!(matches!(
            parse_slurm_state("UNKNOWN_STATE"),
            SlurmState::Unknown(_)
        ));
    }
}
