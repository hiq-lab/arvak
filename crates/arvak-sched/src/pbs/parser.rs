//! Parsers for PBS command output.

use crate::error::{SchedError, SchedResult};
use crate::pbs::adapter::{PbsJobInfo, PbsResourcesUsed, PbsState};

/// Parse qsub output to extract job ID.
///
/// qsub output format varies by PBS implementation:
/// - PBS Pro: "12345.pbs-server"
/// - Torque: "12345.server.domain.com"
/// - OpenPBS: "12345.hostname"
pub fn parse_qsub_output(output: &str) -> SchedResult<String> {
    let trimmed = output.trim();

    // PBS job IDs typically contain a number followed by a server name
    // Format: <number>.<server>
    if trimmed.contains('.') && !trimmed.is_empty() {
        // Validate it starts with a number
        let parts: Vec<&str> = trimmed.split('.').collect();
        if !parts.is_empty() && parts[0].chars().all(|c| c.is_ascii_digit()) {
            return Ok(trimmed.to_string());
        }
    }

    // Some PBS systems might just return the numeric ID
    if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Ok(trimmed.to_string());
    }

    Err(SchedError::PbsCommandError {
        command: "qsub".to_string(),
        message: format!("Unexpected output format: {}", trimmed),
    })
}

/// Parse qstat full output (-f flag) to extract job information.
///
/// qstat -f output format:
/// ```text
/// Job Id: 12345.pbs-server
///     Job_Name = my_job
///     job_state = R
///     queue = batch
///     Exit_status = 0
///     resources_used.walltime = 00:05:23
///     resources_used.cput = 00:04:50
///     resources_used.mem = 1024kb
/// ```
pub fn parse_qstat_full_output(output: &str) -> SchedResult<Option<PbsJobInfo>> {
    if output.trim().is_empty() {
        return Ok(None);
    }

    let mut job_id = String::new();
    let mut name = String::new();
    let mut state = PbsState::Unknown("".to_string());
    let mut queue = None;
    let mut exit_status = None;
    let mut resources_used = PbsResourcesUsed::default();

    for line in output.lines() {
        let line = line.trim();

        // Parse Job Id line
        if let Some(id) = line.strip_prefix("Job Id:") {
            job_id = id.trim().to_string();
            continue;
        }

        // Parse key = value lines
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            match key {
                "Job_Name" => name = value.to_string(),
                "job_state" => state = parse_pbs_state(value),
                "queue" => queue = Some(value.to_string()),
                "Exit_status" => exit_status = value.parse().ok(),
                "resources_used.walltime" => resources_used.walltime = Some(value.to_string()),
                "resources_used.cput" => resources_used.cput = Some(value.to_string()),
                "resources_used.mem" => resources_used.mem = Some(value.to_string()),
                "resources_used.vmem" => resources_used.vmem = Some(value.to_string()),
                _ => {}
            }
        }
    }

    if job_id.is_empty() {
        return Ok(None);
    }

    let walltime_used = resources_used.walltime.clone();

    Ok(Some(PbsJobInfo {
        job_id,
        name,
        state,
        queue,
        exit_status,
        walltime_used,
        resources_used: Some(resources_used),
    }))
}

/// Parse qstat brief output (default format).
///
/// Default qstat output:
/// ```text
/// Job id            Name             User              Time Use S Queue
/// ----------------  ---------------- ----------------  -------- - -----
/// 12345.pbs-server  my_job           user              00:05:23 R batch
/// ```
pub fn parse_qstat_brief_output(output: &str) -> SchedResult<Option<PbsJobInfo>> {
    let lines: Vec<&str> = output.lines().collect();

    // Need at least header + separator + data line
    if lines.len() < 3 {
        return Ok(None);
    }

    // Find the data line (skip header and separator)
    for line in &lines[2..] {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse space-separated fields
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 6 {
            continue;
        }

        let job_id = parts[0].to_string();
        let name = parts[1].to_string();
        // parts[2] is user
        // parts[3] is time use
        let state = parse_pbs_state(parts[4]);
        let queue = Some(parts[5].to_string());

        return Ok(Some(PbsJobInfo {
            job_id,
            name,
            state,
            queue,
            exit_status: None,
            walltime_used: Some(parts[3].to_string()),
            resources_used: None,
        }));
    }

    Ok(None)
}

/// Parse PBS state string to PbsState enum.
pub fn parse_pbs_state(state: &str) -> PbsState {
    match state.to_uppercase().as_str() {
        "Q" | "QUEUED" => PbsState::Queued,
        "R" | "RUNNING" => PbsState::Running,
        "E" | "EXITING" => PbsState::Exiting,
        "C" | "COMPLETED" => PbsState::Completed,
        "H" | "HELD" => PbsState::Held,
        "W" | "WAITING" => PbsState::Waiting,
        "S" | "SUSPENDED" => PbsState::Suspended,
        "T" | "TRANSIT" => PbsState::Transit,
        "B" | "BEGUN" => PbsState::ArrayRunning,
        "F" | "FAILED" | "FINISHED" => {
            // PBS Pro uses F for finished jobs - need to check exit status
            // to determine if it was success or failure
            PbsState::Completed // Default to completed, caller should check exit_status
        }
        "X" | "EXPIRED" => PbsState::Failed,
        _ => PbsState::Unknown(state.to_string()),
    }
}

/// Parse qdel output to verify deletion.
pub fn parse_qdel_output(output: &str, stderr: &str) -> SchedResult<()> {
    // qdel typically produces no output on success
    // Check for error messages in stderr
    if stderr.contains("Unknown Job Id") || stderr.contains("does not exist") {
        return Err(SchedError::PbsJobNotFound(
            "Job not found or already completed".to_string(),
        ));
    }

    if stderr.contains("Unauthorized") || stderr.contains("permission denied") {
        return Err(SchedError::PbsCommandError {
            command: "qdel".to_string(),
            message: "Permission denied".to_string(),
        });
    }

    if !stderr.is_empty()
        && !stderr.contains("being deleted")
        && !stderr.contains("has already finished")
    {
        return Err(SchedError::PbsCommandError {
            command: "qdel".to_string(),
            message: stderr.to_string(),
        });
    }

    // Empty output or deletion messages are OK
    let _ = output;
    Ok(())
}

/// Parse tracejob output for detailed job history.
///
/// tracejob shows the complete history of a job including:
/// - Submission time and queue
/// - Resource requests
/// - Execution node
/// - Exit status and resource usage
#[allow(dead_code)]
pub fn parse_tracejob_output(output: &str) -> SchedResult<Option<PbsJobInfo>> {
    if output.trim().is_empty() {
        return Ok(None);
    }

    let mut job_id = String::new();
    let mut name = String::new();
    let mut state = PbsState::Unknown("".to_string());
    let mut queue = None;
    let mut exit_status = None;

    for line in output.lines() {
        let line = line.trim();

        // Look for job ID in header
        if line.starts_with("Job:") {
            if let Some(id) = line.strip_prefix("Job:") {
                job_id = id.trim().to_string();
            }
            continue;
        }

        // Look for exit status
        if line.contains("Exit_status=") {
            if let Some(pos) = line.find("Exit_status=") {
                let rest = &line[pos + 12..];
                if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
                    exit_status = rest[..end].parse().ok();
                } else {
                    exit_status = rest.parse().ok();
                }
            }
        }

        // Look for job name
        if line.contains("Job_Name=") {
            if let Some(pos) = line.find("Job_Name=") {
                let rest = &line[pos + 9..];
                if let Some(end) = rest.find(char::is_whitespace) {
                    name = rest[..end].to_string();
                } else {
                    name = rest.to_string();
                }
            }
        }

        // Look for queue
        if line.contains("queue=") {
            if let Some(pos) = line.find("queue=") {
                let rest = &line[pos + 6..];
                if let Some(end) = rest.find(char::is_whitespace) {
                    queue = Some(rest[..end].to_string());
                } else {
                    queue = Some(rest.to_string());
                }
            }
        }

        // Determine final state based on exit status
        if line.contains("job ended") {
            state = if exit_status == Some(0) {
                PbsState::Completed
            } else {
                PbsState::Failed
            };
        }
    }

    if job_id.is_empty() {
        return Ok(None);
    }

    Ok(Some(PbsJobInfo {
        job_id,
        name,
        state,
        queue,
        exit_status,
        walltime_used: None,
        resources_used: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_qsub_output() {
        let output = "12345.pbs-server\n";
        let job_id = parse_qsub_output(output).unwrap();
        assert_eq!(job_id, "12345.pbs-server");

        let output = "999999.cluster.local";
        let job_id = parse_qsub_output(output).unwrap();
        assert_eq!(job_id, "999999.cluster.local");

        let output = "12345";
        let job_id = parse_qsub_output(output).unwrap();
        assert_eq!(job_id, "12345");
    }

    #[test]
    fn test_parse_qsub_output_error() {
        let output = "qsub: Unknown queue";
        assert!(parse_qsub_output(output).is_err());
    }

    #[test]
    fn test_parse_qstat_full_output() {
        let output = r#"Job Id: 12345.pbs-server
    Job_Name = my_quantum_job
    job_state = R
    queue = quantum
    resources_used.walltime = 00:05:23
    resources_used.cput = 00:04:50
    resources_used.mem = 2048kb
"#;
        let info = parse_qstat_full_output(output).unwrap().unwrap();
        assert_eq!(info.job_id, "12345.pbs-server");
        assert_eq!(info.name, "my_quantum_job");
        assert!(matches!(info.state, PbsState::Running));
        assert_eq!(info.queue, Some("quantum".to_string()));
        assert_eq!(info.walltime_used, Some("00:05:23".to_string()));

        let resources = info.resources_used.unwrap();
        assert_eq!(resources.cput, Some("00:04:50".to_string()));
        assert_eq!(resources.mem, Some("2048kb".to_string()));
    }

    #[test]
    fn test_parse_qstat_full_output_completed() {
        let output = r#"Job Id: 12345.pbs-server
    Job_Name = completed_job
    job_state = C
    queue = batch
    Exit_status = 0
"#;
        let info = parse_qstat_full_output(output).unwrap().unwrap();
        assert!(matches!(info.state, PbsState::Completed));
        assert_eq!(info.exit_status, Some(0));
    }

    #[test]
    fn test_parse_qstat_full_output_empty() {
        let output = "";
        let info = parse_qstat_full_output(output).unwrap();
        assert!(info.is_none());
    }

    #[test]
    fn test_parse_qstat_brief_output() {
        let output = r#"Job id            Name             User              Time Use S Queue
----------------  ---------------- ----------------  -------- - -----
12345.pbs-server  my_job           testuser          00:05:23 R batch
"#;
        let info = parse_qstat_brief_output(output).unwrap().unwrap();
        assert_eq!(info.job_id, "12345.pbs-server");
        assert_eq!(info.name, "my_job");
        assert!(matches!(info.state, PbsState::Running));
        assert_eq!(info.queue, Some("batch".to_string()));
    }

    #[test]
    fn test_parse_pbs_state() {
        assert!(matches!(parse_pbs_state("Q"), PbsState::Queued));
        assert!(matches!(parse_pbs_state("R"), PbsState::Running));
        assert!(matches!(parse_pbs_state("E"), PbsState::Exiting));
        assert!(matches!(parse_pbs_state("C"), PbsState::Completed));
        assert!(matches!(parse_pbs_state("H"), PbsState::Held));
        assert!(matches!(parse_pbs_state("W"), PbsState::Waiting));
        assert!(matches!(parse_pbs_state("S"), PbsState::Suspended));
        assert!(matches!(parse_pbs_state("QUEUED"), PbsState::Queued));
        assert!(matches!(parse_pbs_state("RUNNING"), PbsState::Running));
        assert!(matches!(parse_pbs_state("unknown"), PbsState::Unknown(_)));
    }

    #[test]
    fn test_parse_qdel_output_success() {
        // Empty output is success
        assert!(parse_qdel_output("", "").is_ok());

        // Deletion in progress is success
        assert!(parse_qdel_output("", "Job being deleted").is_ok());
    }

    #[test]
    fn test_parse_qdel_output_not_found() {
        let result = parse_qdel_output("", "Unknown Job Id 12345.server");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SchedError::PbsJobNotFound(_)));
    }

    #[test]
    fn test_parse_qdel_output_permission() {
        let result = parse_qdel_output("", "Unauthorized request");
        assert!(result.is_err());
    }
}
