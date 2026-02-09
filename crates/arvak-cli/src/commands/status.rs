//! Status command implementation.
//!
//! Query job status from the local scheduler state store.

use anyhow::Result;
use console::style;

use arvak_sched::{JobFilter, ScheduledJobId, Scheduler};

use super::common::create_scheduler;

/// Execute the status command.
pub async fn execute(job_id: Option<&str>, all: bool) -> Result<()> {
    let scheduler = create_scheduler()?;

    if all {
        // List all jobs
        let jobs = scheduler
            .list_jobs(JobFilter::default())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list jobs: {e}"))?;

        if jobs.is_empty() {
            println!("No jobs found.");
            return Ok(());
        }

        println!("{} {} job(s):\n", style("→").cyan().bold(), jobs.len());

        // Table header
        println!(
            "  {:<36}  {:<20}  {:<12}  {:<8}  {}",
            style("JOB ID").bold(),
            style("NAME").bold(),
            style("STATUS").bold(),
            style("SHOTS").bold(),
            style("CREATED").bold()
        );
        println!("  {}", "-".repeat(100));

        for job in &jobs {
            let status_name = job.status.name();
            let status_styled = match status_name {
                "Completed" => style(status_name).green(),
                "Failed" | "Cancelled" => style(status_name).red(),
                "Pending" | "WaitingOnDependencies" => style(status_name).yellow(),
                _ => style(status_name).cyan(),
            };

            println!(
                "  {:<36}  {:<20}  {:<12}  {:<8}  {}",
                style(&job.id).dim(),
                job.name,
                status_styled,
                job.shots,
                job.created_at.format("%Y-%m-%d %H:%M"),
            );
        }

        return Ok(());
    }

    // Single job status
    let job_id_str = job_id
        .ok_or_else(|| anyhow::anyhow!("Please provide a job ID or use --all to list all jobs"))?;

    let parsed_id = ScheduledJobId::parse(job_id_str)
        .map_err(|e| anyhow::anyhow!("Invalid job ID '{job_id_str}': {e}"))?;

    let status = scheduler
        .status(&parsed_id)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get status: {e}"))?;

    let status_name = status.name();
    let status_styled = match status_name {
        "Completed" => style(status_name).green().bold(),
        "Failed" | "Cancelled" => style(status_name).red().bold(),
        "Pending" | "WaitingOnDependencies" => style(status_name).yellow().bold(),
        _ => style(status_name).cyan().bold(),
    };

    println!(
        "{} Job {} status: {}",
        style("→").cyan().bold(),
        style(job_id_str).dim(),
        status_styled
    );

    if let Some(slurm_id) = status.slurm_job_id() {
        println!("  SLURM job ID: {}", style(slurm_id).yellow());
    }

    if let Some(quantum_id) = status.quantum_job_id() {
        println!("  Quantum job ID: {}", style(quantum_id).yellow());
    }

    if status.is_terminal() {
        println!("  Terminal: {}", style("yes").dim());
    }

    Ok(())
}
