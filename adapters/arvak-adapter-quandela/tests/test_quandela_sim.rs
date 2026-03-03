//! Integration test — Quandela sim:ascella / sim:belenos
//!
//! Submits a Bell state circuit to the Quandela Cloud simulator and asserts
//! that the |00⟩ + |11⟩ Bell fraction exceeds 90%.
//!
//! Requires:
//!   PCVL_CLOUD_TOKEN  — Quandela cloud token (or the key file)
//!
//! Skipped automatically when the token is not present.

use std::time::Duration;

use arvak_adapter_quandela::QuandelaBackend;
use arvak_hal::{Backend, JobStatus};

fn token_available() -> bool {
    let env_token = std::env::var("PCVL_CLOUD_TOKEN")
        .unwrap_or_default()
        .trim()
        .to_string();
    if !env_token.is_empty() {
        return true;
    }
    let keyfile = dirs::home_dir()
        .unwrap_or_default()
        .join(".openclaw/credentials/quandela/cloud.key");
    keyfile.exists()
        && std::fs::read_to_string(keyfile)
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
}

async fn poll_until_done(
    backend: &QuandelaBackend,
    job_id: &arvak_hal::JobId,
    max_wait_secs: u64,
) -> arvak_hal::HalResult<JobStatus> {
    let deadline = std::time::Instant::now() + Duration::from_secs(max_wait_secs);
    loop {
        let status = backend.status(job_id).await?;
        match &status {
            JobStatus::Completed | JobStatus::Failed(_) | JobStatus::Cancelled => {
                return Ok(status);
            }
            _ => {}
        }
        if std::time::Instant::now() >= deadline {
            return Ok(status);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[tokio::test]
async fn test_bell_state_sim_ascella() {
    if !token_available() {
        eprintln!("SKIP: PCVL_CLOUD_TOKEN not set");
        return;
    }

    let backend =
        QuandelaBackend::for_platform("sim:ascella").expect("failed to create sim:ascella backend");

    let circuit = arvak_ir::Circuit::bell().expect("failed to build Bell circuit");
    let shots = 500u32;

    // Validate first.
    let vr = backend.validate(&circuit).await.expect("validate failed");
    assert!(
        matches!(vr, arvak_hal::ValidationResult::Valid),
        "Bell circuit should be Valid, got {vr:?}"
    );

    // Submit.
    let job_id = backend
        .submit(&circuit, shots, None)
        .await
        .expect("submit failed");
    eprintln!("Submitted job: {job_id}");

    // Poll for completion (simulator should finish in < 60 s).
    let status = poll_until_done(&backend, &job_id, 120)
        .await
        .expect("status polling failed");
    assert!(
        matches!(status, JobStatus::Completed),
        "job did not complete (status: {status:?})"
    );

    // Fetch results.
    let result = backend.result(&job_id).await.expect("result fetch failed");
    eprintln!("Counts: {:?}", result.counts.sorted());

    let total = result.counts.total_shots();
    assert!(total > 0, "expected non-zero shot count");

    let bell = result.counts.get("00") + result.counts.get("11");
    #[allow(clippy::cast_precision_loss)]
    let frac = bell as f64 / total as f64;
    eprintln!("|00>+|11> fraction: {:.1}%", frac * 100.0);

    assert!(
        frac >= 0.90,
        "Bell fraction {:.1}% < 90% (counts: {:?})",
        frac * 100.0,
        result.counts.sorted()
    );
}
