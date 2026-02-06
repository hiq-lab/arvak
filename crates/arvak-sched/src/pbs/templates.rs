//! PBS batch script templates.

use std::path::Path;

use crate::job::ScheduledJob;
use crate::pbs::adapter::PbsConfig;

/// Generate a PBS batch script for a quantum job.
pub fn generate_pbs_script(
    job: &ScheduledJob,
    config: &PbsConfig,
    circuit_file: &Path,
    result_file: &Path,
) -> String {
    let mut script = String::new();

    // Shebang
    script.push_str("#!/bin/bash\n");

    // PBS directives
    script.push_str(&format!("#PBS -N {}\n", sanitize_name(&job.name)));
    script.push_str(&format!(
        "#PBS -o {}/pbs-$PBS_JOBID.out\n",
        config.work_dir.display()
    ));
    script.push_str(&format!(
        "#PBS -e {}/pbs-$PBS_JOBID.err\n",
        config.work_dir.display()
    ));

    // Queue selection (can be overridden by priority mapping)
    let queue = if let Some(ref queue_mapping) = config.priority_queue_mapping {
        queue_mapping
            .get(&job.priority.value())
            .unwrap_or(&config.queue)
            .clone()
    } else {
        config.queue.clone()
    };
    script.push_str(&format!("#PBS -q {}\n", queue));

    // Account if specified
    if let Some(ref account) = config.account {
        script.push_str(&format!("#PBS -A {}\n", account));
    }

    // Resource requests using -l flag
    script.push_str(&format!("#PBS -l walltime={}\n", config.walltime));
    script.push_str(&format!(
        "#PBS -l nodes={}:ppn={}\n",
        config.nodes, config.ppn
    ));
    script.push_str(&format!("#PBS -l mem={}\n", config.memory));

    // Join stdout and stderr (optional, but often useful)
    script.push_str("#PBS -j oe\n");

    // Export current environment
    script.push_str("#PBS -V\n");

    // Additional directives
    for directive in &config.extra_directives {
        script.push_str(&format!("#PBS {}\n", directive));
    }

    // Environment setup
    script.push_str("\n# Environment setup\n");
    script.push_str("set -e\n");
    script.push_str("set -o pipefail\n\n");

    // Change to submission directory (PBS specific)
    script.push_str("# Change to submission directory\n");
    script.push_str("cd $PBS_O_WORKDIR\n\n");

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
    script.push_str("echo \"Job ID: $PBS_JOBID\"\n");
    script.push_str("echo \"Job Name: $PBS_JOBNAME\"\n");
    script.push_str("echo \"Node: $PBS_NODEFILE\"\n");
    script.push_str("echo \"Queue: $PBS_QUEUE\"\n");
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

/// Generate a PBS batch script for multiple circuits (batch job).
pub fn generate_pbs_script_multi(
    job: &ScheduledJob,
    config: &PbsConfig,
    circuit_files: &[&Path],
    result_dir: &Path,
) -> String {
    let mut script = String::new();

    // Shebang
    script.push_str("#!/bin/bash\n");

    // PBS directives
    script.push_str(&format!("#PBS -N {}\n", sanitize_name(&job.name)));
    script.push_str(&format!(
        "#PBS -o {}/pbs-$PBS_JOBID.out\n",
        config.work_dir.display()
    ));
    script.push_str(&format!(
        "#PBS -e {}/pbs-$PBS_JOBID.err\n",
        config.work_dir.display()
    ));

    // Queue selection
    let queue = if let Some(ref queue_mapping) = config.priority_queue_mapping {
        queue_mapping
            .get(&job.priority.value())
            .unwrap_or(&config.queue)
            .clone()
    } else {
        config.queue.clone()
    };
    script.push_str(&format!("#PBS -q {}\n", queue));

    if let Some(ref account) = config.account {
        script.push_str(&format!("#PBS -A {}\n", account));
    }

    // Scale walltime based on number of circuits
    let scaled_walltime = scale_walltime(&config.walltime, circuit_files.len());
    script.push_str(&format!("#PBS -l walltime={}\n", scaled_walltime));
    script.push_str(&format!(
        "#PBS -l nodes={}:ppn={}\n",
        config.nodes, config.ppn
    ));
    script.push_str(&format!("#PBS -l mem={}\n", config.memory));
    script.push_str("#PBS -j oe\n");
    script.push_str("#PBS -V\n");

    // Additional directives
    for directive in &config.extra_directives {
        script.push_str(&format!("#PBS {}\n", directive));
    }

    // Environment setup
    script.push_str("\n# Environment setup\n");
    script.push_str("set -e\n");
    script.push_str("set -o pipefail\n\n");

    // Change to submission directory
    script.push_str("cd $PBS_O_WORKDIR\n\n");

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
    script.push_str("echo \"Job ID: $PBS_JOBID\"\n");
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

/// Sanitize a job name for PBS.
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(15) // PBS has a 15 character limit for job names (stricter than SLURM)
        .collect()
}

/// Scale walltime based on number of circuits.
///
/// Takes a walltime string in HH:MM:SS format and multiplies by the count.
fn scale_walltime(walltime: &str, count: usize) -> String {
    // Parse HH:MM:SS
    let parts: Vec<&str> = walltime.split(':').collect();
    if parts.len() != 3 {
        // Return scaled default if parsing fails
        return format!("{:02}:00:00", count);
    }

    let hours: u32 = parts[0].parse().unwrap_or(1);
    let minutes: u32 = parts[1].parse().unwrap_or(0);
    let seconds: u32 = parts[2].parse().unwrap_or(0);

    let total_seconds = (hours * 3600 + minutes * 60 + seconds) * count as u32;

    let new_hours = total_seconds / 3600;
    let new_minutes = (total_seconds % 3600) / 60;
    let new_seconds = total_seconds % 60;

    format!("{:02}:{:02}:{:02}", new_hours, new_minutes, new_seconds)
}

/// Generate PBS array job script (for embarrassingly parallel workloads).
#[allow(dead_code)]
pub fn generate_pbs_array_script(
    job: &ScheduledJob,
    config: &PbsConfig,
    circuit_files: &[&Path],
    result_dir: &Path,
) -> String {
    let mut script = String::new();

    // Shebang
    script.push_str("#!/bin/bash\n");

    // PBS directives
    script.push_str(&format!("#PBS -N {}\n", sanitize_name(&job.name)));
    script.push_str(&format!(
        "#PBS -o {}/pbs-$PBS_JOBID-$PBS_ARRAYID.out\n",
        config.work_dir.display()
    ));
    script.push_str(&format!(
        "#PBS -e {}/pbs-$PBS_JOBID-$PBS_ARRAYID.err\n",
        config.work_dir.display()
    ));
    script.push_str(&format!("#PBS -q {}\n", config.queue));

    if let Some(ref account) = config.account {
        script.push_str(&format!("#PBS -A {}\n", account));
    }

    // Array job specification
    script.push_str(&format!("#PBS -t 0-{}\n", circuit_files.len() - 1));

    script.push_str(&format!("#PBS -l walltime={}\n", config.walltime));
    script.push_str(&format!(
        "#PBS -l nodes={}:ppn={}\n",
        config.nodes, config.ppn
    ));
    script.push_str(&format!("#PBS -l mem={}\n", config.memory));
    script.push_str("#PBS -V\n");

    // Environment setup
    script.push_str("\n# Environment setup\n");
    script.push_str("set -e\n");
    script.push_str("cd $PBS_O_WORKDIR\n\n");

    // Load modules
    if !config.modules.is_empty() {
        script.push_str("# Load required modules\n");
        for module in &config.modules {
            script.push_str(&format!("module load {}\n", module));
        }
        script.push('\n');
    }

    // Activate venv
    if let Some(ref venv) = config.python_venv {
        script.push_str(&format!("source {}/bin/activate\n\n", venv.display()));
    }

    // Create circuit file array
    script.push_str("# Circuit files\n");
    script.push_str("CIRCUITS=(\n");
    for circuit_file in circuit_files {
        script.push_str(&format!("    \"{}\"\n", circuit_file.display()));
    }
    script.push_str(")\n\n");

    // Select circuit based on array index
    script.push_str("CIRCUIT=${CIRCUITS[$PBS_ARRAYID]}\n");
    script.push_str(&format!(
        "RESULT={}/result_$PBS_ARRAYID.json\n\n",
        result_dir.display()
    ));

    // Execute
    script.push_str("echo \"Array task $PBS_ARRAYID: Running $CIRCUIT\"\n");

    let backend_flag = if let Some(ref backend) = job.matched_backend {
        format!("--backend {}", backend)
    } else {
        String::new()
    };

    script.push_str(&format!(
        "{} run $CIRCUIT --shots {} {} --output $RESULT\n",
        config.hiq_binary.display(),
        job.shots,
        backend_flag,
    ));

    script.push_str("echo \"Task $PBS_ARRAYID completed with exit code $?\"\n");

    script
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{CircuitSpec, Priority};
    use std::path::PathBuf;

    fn test_config() -> PbsConfig {
        PbsConfig {
            queue: "quantum".to_string(),
            account: Some("project123".to_string()),
            walltime: "01:00:00".to_string(),
            memory: "4gb".to_string(),
            nodes: 1,
            ppn: 1,
            work_dir: PathBuf::from("/scratch/jobs"),
            hiq_binary: PathBuf::from("/opt/hiq/bin/hiq"),
            modules: vec!["python/3.11".to_string()],
            python_venv: Some(PathBuf::from("/opt/hiq/venv")),
            server: None,
            extra_directives: Vec::new(),
            priority_queue_mapping: None,
        }
    }

    #[test]
    fn test_generate_pbs_script() {
        let config = test_config();
        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q;");
        let job = ScheduledJob::new("test_job", circuit).with_priority(Priority::high());

        let script = generate_pbs_script(
            &job,
            &config,
            Path::new("/scratch/circuit.qasm"),
            Path::new("/scratch/result.json"),
        );

        assert!(script.contains("#!/bin/bash"));
        assert!(script.contains("#PBS -N test_job"));
        assert!(script.contains("#PBS -q quantum"));
        assert!(script.contains("#PBS -A project123"));
        assert!(script.contains("#PBS -l walltime=01:00:00"));
        assert!(script.contains("#PBS -l nodes=1:ppn=1"));
        assert!(script.contains("#PBS -l mem=4gb"));
        assert!(script.contains("module load python/3.11"));
        assert!(script.contains("source /opt/hiq/venv/bin/activate"));
        assert!(script.contains("/opt/hiq/bin/hiq run"));
        assert!(script.contains("cd $PBS_O_WORKDIR"));
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("my_job"), "my_job");
        assert_eq!(sanitize_name("my job"), "my_job");
        assert_eq!(sanitize_name("my/job:name"), "my_job_name");

        // Test length limit (PBS has 15 char limit)
        let long_name = "a".repeat(100);
        assert_eq!(sanitize_name(&long_name).len(), 15);
    }

    #[test]
    fn test_scale_walltime() {
        assert_eq!(scale_walltime("01:00:00", 1), "01:00:00");
        assert_eq!(scale_walltime("01:00:00", 2), "02:00:00");
        assert_eq!(scale_walltime("00:30:00", 3), "01:30:00");
        assert_eq!(scale_walltime("00:10:00", 10), "01:40:00");
        assert_eq!(scale_walltime("00:00:30", 4), "00:02:00");
    }

    #[test]
    fn test_generate_pbs_script_multi() {
        let config = test_config();
        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q;");
        let job = ScheduledJob::new("batch_job", circuit);

        let circuits = vec![
            Path::new("/scratch/c1.qasm"),
            Path::new("/scratch/c2.qasm"),
            Path::new("/scratch/c3.qasm"),
        ];

        let script =
            generate_pbs_script_multi(&job, &config, &circuits, Path::new("/scratch/results"));

        assert!(script.contains("#PBS -N batch_job"));
        assert!(script.contains("#PBS -l walltime=03:00:00")); // Scaled 3x
        assert!(script.contains("mkdir -p /scratch/results"));
        assert!(script.contains("echo \"Batch size: 3\""));
        assert!(script.contains("Running circuit 1 of 3"));
        assert!(script.contains("Running circuit 2 of 3"));
        assert!(script.contains("Running circuit 3 of 3"));
    }

    #[test]
    fn test_generate_pbs_array_script() {
        let config = test_config();
        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q;");
        let job = ScheduledJob::new("array_job", circuit);

        let circuits = vec![Path::new("/scratch/c1.qasm"), Path::new("/scratch/c2.qasm")];

        let script =
            generate_pbs_array_script(&job, &config, &circuits, Path::new("/scratch/results"));

        assert!(script.contains("#PBS -t 0-1")); // Array indices
        assert!(script.contains("CIRCUITS=("));
        assert!(script.contains("$PBS_ARRAYID"));
    }
}
