//! SLURM batch script templates.

use std::path::Path;

use crate::job::ScheduledJob;
use crate::slurm::adapter::SlurmConfig;

/// Generate a SLURM batch script for a quantum job.
pub fn generate_batch_script(
    job: &ScheduledJob,
    config: &SlurmConfig,
    circuit_file: &Path,
    result_file: &Path,
) -> String {
    let mut script = String::new();

    // Shebang
    script.push_str("#!/bin/bash\n");

    // SLURM directives
    script.push_str(&format!(
        "#SBATCH --job-name={}\n",
        sanitize_name(&job.name)
    ));
    script.push_str(&format!(
        "#SBATCH --output={}/slurm-%j.out\n",
        config.work_dir.display()
    ));
    script.push_str(&format!(
        "#SBATCH --error={}/slurm-%j.err\n",
        config.work_dir.display()
    ));
    script.push_str(&format!("#SBATCH --partition={}\n", config.partition));

    if let Some(ref account) = config.account {
        script.push_str(&format!("#SBATCH --account={}\n", account));
    }

    script.push_str(&format!(
        "#SBATCH --time={}\n",
        format_time(config.time_limit)
    ));
    script.push_str(&format!("#SBATCH --mem={}M\n", config.memory_mb));
    script.push_str(&format!(
        "#SBATCH --cpus-per-task={}\n",
        config.cpus_per_task
    ));

    // Optional QOS based on priority
    if let Some(ref qos_mapping) = config.priority_qos_mapping {
        if let Some(qos) = qos_mapping.get(&job.priority.value()) {
            script.push_str(&format!("#SBATCH --qos={}\n", qos));
        }
    }

    // Environment setup
    script.push_str("\n# Environment setup\n");
    script.push_str("set -e\n");
    script.push_str("set -o pipefail\n\n");

    // Load modules if configured
    if !config.modules.is_empty() {
        script.push_str("# Load required modules\n");
        for module in &config.modules {
            script.push_str(&format!("module load {}\n", module));
        }
        script.push('\n');
    }

    // Activate virtual environment if configured
    if let Some(ref venv) = config.python_venv {
        script.push_str("# Activate Python environment\n");
        script.push_str(&format!("source {}/bin/activate\n\n", venv.display()));
    }

    // Job information
    script.push_str("# Job information\n");
    script.push_str("echo \"Job ID: $SLURM_JOB_ID\"\n");
    script.push_str("echo \"Job Name: $SLURM_JOB_NAME\"\n");
    script.push_str("echo \"Node: $SLURM_NODELIST\"\n");
    script.push_str("echo \"Start Time: $(date)\"\n\n");

    // Execute HIQ command
    script.push_str("# Execute quantum job\n");

    let backend_flag = if let Some(ref backend) = job.matched_backend {
        format!("--backend {}", backend)
    } else {
        String::new()
    };

    script.push_str(&format!(
        "{} run {} --shots {} {} --output {}\n",
        config.hiq_binary.display(),
        circuit_file.display(),
        job.shots,
        backend_flag,
        result_file.display(),
    ));

    // Completion message
    script.push_str("\necho \"Job completed at: $(date)\"\n");
    script.push_str("echo \"Exit code: $?\"\n");

    script
}

/// Generate a batch script for multiple circuits (batch job).
pub fn generate_batch_script_multi(
    job: &ScheduledJob,
    config: &SlurmConfig,
    circuit_files: &[&Path],
    result_dir: &Path,
) -> String {
    let mut script = String::new();

    // Shebang
    script.push_str("#!/bin/bash\n");

    // SLURM directives
    script.push_str(&format!(
        "#SBATCH --job-name={}\n",
        sanitize_name(&job.name)
    ));
    script.push_str(&format!(
        "#SBATCH --output={}/slurm-%j.out\n",
        config.work_dir.display()
    ));
    script.push_str(&format!(
        "#SBATCH --error={}/slurm-%j.err\n",
        config.work_dir.display()
    ));
    script.push_str(&format!("#SBATCH --partition={}\n", config.partition));

    if let Some(ref account) = config.account {
        script.push_str(&format!("#SBATCH --account={}\n", account));
    }

    // Scale time based on number of circuits
    let scaled_time = config.time_limit * circuit_files.len() as u32;
    script.push_str(&format!("#SBATCH --time={}\n", format_time(scaled_time)));
    script.push_str(&format!("#SBATCH --mem={}M\n", config.memory_mb));
    script.push_str(&format!(
        "#SBATCH --cpus-per-task={}\n",
        config.cpus_per_task
    ));

    // Environment setup
    script.push_str("\n# Environment setup\n");
    script.push_str("set -e\n");
    script.push_str("set -o pipefail\n\n");

    // Load modules if configured
    if !config.modules.is_empty() {
        script.push_str("# Load required modules\n");
        for module in &config.modules {
            script.push_str(&format!("module load {}\n", module));
        }
        script.push('\n');
    }

    // Activate virtual environment if configured
    if let Some(ref venv) = config.python_venv {
        script.push_str("# Activate Python environment\n");
        script.push_str(&format!("source {}/bin/activate\n\n", venv.display()));
    }

    // Job information
    script.push_str("# Job information\n");
    script.push_str("echo \"Job ID: $SLURM_JOB_ID\"\n");
    script.push_str(&format!("echo \"Batch size: {}\"\n", circuit_files.len()));
    script.push_str("echo \"Start Time: $(date)\"\n\n");

    // Create result directory
    script.push_str(&format!("mkdir -p {}\n\n", result_dir.display()));

    // Execute each circuit
    script.push_str("# Execute quantum jobs\n");
    script.push_str("FAILED=0\n\n");

    let backend_flag = if let Some(ref backend) = job.matched_backend {
        format!("--backend {}", backend)
    } else {
        String::new()
    };

    for (i, circuit_file) in circuit_files.iter().enumerate() {
        let result_file = result_dir.join(format!("result_{}.json", i));
        script.push_str(&format!(
            "echo \"Running circuit {} of {}\"\n",
            i + 1,
            circuit_files.len()
        ));
        script.push_str(&format!(
            "if ! {} run {} --shots {} {} --output {}; then\n",
            config.hiq_binary.display(),
            circuit_file.display(),
            job.shots,
            backend_flag,
            result_file.display(),
        ));
        script.push_str("    echo \"Circuit failed\"\n");
        script.push_str("    FAILED=$((FAILED + 1))\n");
        script.push_str("fi\n\n");
    }

    // Summary
    script.push_str("echo \"Job completed at: $(date)\"\n");
    script.push_str(&format!(
        "echo \"Total circuits: {}\"\n",
        circuit_files.len()
    ));
    script.push_str("echo \"Failed circuits: $FAILED\"\n");
    script.push_str("exit $FAILED\n");

    script
}

/// Sanitize a job name for SLURM.
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(64) // SLURM has a 64 character limit for job names
        .collect()
}

/// Format time in minutes to SLURM time format (D-HH:MM:SS or HH:MM:SS).
fn format_time(minutes: u32) -> String {
    let hours = minutes / 60;
    let mins = minutes % 60;

    if hours >= 24 {
        let days = hours / 24;
        let remaining_hours = hours % 24;
        format!("{}-{:02}:{:02}:00", days, remaining_hours, mins)
    } else {
        format!("{:02}:{:02}:00", hours, mins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{CircuitSpec, Priority};
    use std::path::PathBuf;

    fn test_config() -> SlurmConfig {
        SlurmConfig {
            partition: "quantum".to_string(),
            account: Some("project123".to_string()),
            time_limit: 60,
            memory_mb: 4096,
            cpus_per_task: 1,
            work_dir: PathBuf::from("/scratch/jobs"),
            hiq_binary: PathBuf::from("/opt/hiq/bin/hiq"),
            modules: vec!["python/3.11".to_string()],
            python_venv: Some(PathBuf::from("/opt/hiq/venv")),
            priority_qos_mapping: None,
        }
    }

    #[test]
    fn test_generate_batch_script() {
        let config = test_config();
        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q;");
        let job = ScheduledJob::new("test_job", circuit).with_priority(Priority::high());

        let script = generate_batch_script(
            &job,
            &config,
            Path::new("/scratch/circuit.qasm"),
            Path::new("/scratch/result.json"),
        );

        assert!(script.contains("#!/bin/bash"));
        assert!(script.contains("#SBATCH --job-name=test_job"));
        assert!(script.contains("#SBATCH --partition=quantum"));
        assert!(script.contains("#SBATCH --account=project123"));
        assert!(script.contains("#SBATCH --time=01:00:00"));
        assert!(script.contains("module load python/3.11"));
        assert!(script.contains("source /opt/hiq/venv/bin/activate"));
        assert!(script.contains("/opt/hiq/bin/hiq run"));
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("my_job"), "my_job");
        assert_eq!(sanitize_name("my job"), "my_job");
        assert_eq!(sanitize_name("my/job:name"), "my_job_name");

        // Test length limit
        let long_name = "a".repeat(100);
        assert_eq!(sanitize_name(&long_name).len(), 64);
    }

    #[test]
    fn test_format_time() {
        assert_eq!(format_time(30), "00:30:00");
        assert_eq!(format_time(60), "01:00:00");
        assert_eq!(format_time(90), "01:30:00");
        assert_eq!(format_time(1440), "1-00:00:00"); // 24 hours
        assert_eq!(format_time(2880), "2-00:00:00"); // 48 hours
    }
}
