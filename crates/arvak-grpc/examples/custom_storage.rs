//! Example demonstrating custom storage backend usage.
//!
//! This example shows how to:
//! 1. Create a `JobStore` with default in-memory storage
//! 2. Create a `JobStore` with a custom storage backend
//! 3. Store and retrieve jobs
//! 4. Update job status and results
//!
//! Run with: cargo run --example `custom_storage`

use arvak_grpc::JobStore;
use arvak_grpc::storage::{JobStorage, MemoryStorage};
use arvak_hal::job::{JobId, JobStatus};
use arvak_hal::result::{Counts, ExecutionResult};
use arvak_ir::circuit::Circuit;
use serde_json::Value;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Arvak gRPC Storage Example ===\n");

    // Example 1: Default in-memory storage
    println!("1. Using default in-memory storage:");
    let job_store = JobStore::new();

    let circuit = Circuit::with_size("bell_state", 2, 0);
    let job_id = job_store
        .create_job(circuit, "simulator".to_string(), 1000)
        .await?;

    println!("   Created job: {}", job_id.0);

    let job = job_store.get_job(&job_id).await?;
    println!("   Job status: {:?}", job.status);
    println!("   Job submitted at: {}", job.submitted_at);
    println!();

    // Example 2: Custom storage backend
    println!("2. Using custom storage backend:");
    let custom_storage: Arc<dyn JobStorage> = Arc::new(MemoryStorage::new());
    let custom_store = JobStore::with_storage(custom_storage);

    let circuit2 = Circuit::with_size("ghz_state", 3, 0);
    let job_id2 = custom_store
        .create_job(circuit2, "simulator".to_string(), 2000)
        .await?;

    println!("   Created job: {}", job_id2.0);
    println!();

    // Example 3: Job lifecycle management
    println!("3. Job lifecycle management:");

    // Update to Running
    job_store.update_status(&job_id, JobStatus::Running).await?;
    let job = job_store.get_job(&job_id).await?;
    println!("   Status: {:?}", job.status);
    println!("   Started at: {:?}", job.started_at);

    // Simulate completion with results
    let counts = Counts::from_pairs(vec![("00", 485), ("11", 515)]);

    let result = ExecutionResult {
        counts,
        shots: 1000,
        execution_time_ms: Some(120),
        metadata: Value::Null,
    };

    job_store.store_result(&job_id, result).await?;
    let job = job_store.get_job(&job_id).await?;
    println!("   Status: {:?}", job.status);
    println!("   Completed at: {:?}", job.completed_at);

    // Retrieve result
    let result = job_store.get_result(&job_id).await?;
    println!("   Result counts: {:?}", result.counts);
    println!("   Execution time: {:?}ms", result.execution_time_ms);
    println!();

    // Example 4: Error handling
    println!("4. Error handling:");
    let nonexistent_job = JobId::new("nonexistent-123".to_string());
    match job_store.get_job(&nonexistent_job).await {
        Ok(_) => println!("   Unexpected success!"),
        Err(e) => println!("   Expected error: {e}"),
    }
    println!();

    println!("=== Example Complete ===");

    Ok(())
}
