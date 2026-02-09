//! Result command implementation.
//!
//! Retrieve and display results for a completed job.

use anyhow::Result;
use console::style;

use arvak_sched::{ScheduledJobId, Scheduler};

use super::common::{create_scheduler, print_results};

/// Execute the result command.
pub async fn execute(job_id: &str, format: &str) -> Result<()> {
    let scheduler = create_scheduler()?;

    let parsed_id = ScheduledJobId::parse(job_id)
        .map_err(|e| anyhow::anyhow!("Invalid job ID '{job_id}': {e}"))?;

    println!(
        "{} Fetching results for job {}",
        style("→").cyan().bold(),
        style(job_id).dim()
    );

    let result = scheduler
        .result(&parsed_id)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get result: {e}"))?;

    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&result)
                .map_err(|e| anyhow::anyhow!("JSON serialization failed: {e}"))?;
            println!("{json}");
        }
        _ => {
            print_results(&result);
        }
    }

    Ok(())
}
