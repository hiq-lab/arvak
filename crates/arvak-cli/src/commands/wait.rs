//! Wait command implementation.
//!
//! Poll a job until it reaches a terminal state, then print results.

use anyhow::Result;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};

use arvak_sched::{ScheduledJobId, Scheduler};

use super::common::{create_scheduler, print_results};

/// Execute the wait command.
pub async fn execute(job_id: &str, timeout: u64) -> Result<()> {
    let scheduler = create_scheduler()?;

    let parsed_id = ScheduledJobId::parse(job_id)
        .map_err(|e| anyhow::anyhow!("Invalid job ID '{job_id}': {e}"))?;

    println!(
        "{} Waiting for job {} (timeout: {}s)",
        style("→").cyan().bold(),
        style(job_id).dim(),
        timeout
    );

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.set_message("Waiting for job to complete...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let start = std::time::Instant::now();
    let timeout_duration = std::time::Duration::from_secs(timeout);

    loop {
        let status = scheduler
            .status(&parsed_id)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get status: {e}"))?;

        spinner.set_message(format!("Status: {} ...", status.name()));

        if status.is_terminal() {
            spinner.finish_and_clear();

            if status.is_success() {
                let result = scheduler
                    .result(&parsed_id)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to get result: {e}"))?;
                print_results(&result);
            } else {
                println!(
                    "{} Job finished with status: {}",
                    style("✗").red().bold(),
                    style(status.name()).red()
                );
            }
            return Ok(());
        }

        if start.elapsed() > timeout_duration {
            spinner.finish_and_clear();
            anyhow::bail!(
                "Timeout after {}s. Job {} is still {}. Use 'arvak status {}' to check later.",
                timeout,
                job_id,
                status.name(),
                job_id
            );
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
