//! HAL error types.
//!
//! # HAL Contract v2
//!
//! Errors are categorized by recoverability:
//!
//! | Category | Variants | Recovery |
//! |----------|----------|----------|
//! | **Transient** | `BackendUnavailable`, `Timeout` | Retry with backoff |
//! | **Permanent** | `InvalidCircuit`, `CircuitTooLarge`, `InvalidShots`, `Unsupported` | Fix input |
//! | **Job-level** | `JobFailed`, `JobCancelled`, `JobNotFound` | Resubmit or abort |
//! | **Auth** | `AuthenticationFailed` | Re-authenticate |
//! | **Config** | `Configuration`, `Backend` | Fix configuration |

use thiserror::Error;

/// Errors that can occur in HAL operations.
///
/// All 13 spec variants are present. Arvak-specific extensions are
/// grouped at the end and clearly marked.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HalError {
    // ── HAL Contract v2 spec variants ─────────────────────────────

    /// Backend is not available (transient — retry with backoff).
    #[error("Backend not available: {0}")]
    BackendUnavailable(String),

    /// Authentication failed (re-authenticate).
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    /// Job submission failed.
    #[error("Job submission failed: {0}")]
    SubmissionFailed(String),

    /// Job execution failed (terminal — resubmit if needed).
    #[error("Job failed: {0}")]
    JobFailed(String),

    /// Job was cancelled (terminal).
    #[error("Job cancelled")]
    JobCancelled,

    /// Job not found.
    #[error("Job not found: {0}")]
    JobNotFound(String),

    /// Invalid circuit (permanent — fix input).
    #[error("Invalid circuit: {0}")]
    InvalidCircuit(String),

    /// Circuit exceeds backend capabilities (permanent — fix input).
    #[error("Circuit exceeds backend capabilities: {0}")]
    CircuitTooLarge(String),

    /// Invalid number of shots (permanent — fix input).
    #[error("Invalid shots: {0}")]
    InvalidShots(String),

    /// Unsupported feature (permanent — fix input or choose another backend).
    #[error("Unsupported feature: {0}")]
    Unsupported(String),

    /// Timeout waiting for job (transient — retry with backoff).
    #[error("Timeout waiting for job {0}")]
    Timeout(String),

    /// Configuration error (fix configuration).
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Generic backend error.
    #[error("Backend error: {0}")]
    Backend(String),

    // ── Arvak extensions (not part of HAL Contract v2 spec) ───────

    /// OIDC/token authentication error (Arvak extension).
    #[error("Authentication error: {0}")]
    Auth(String),

    /// Network transport error (Arvak extension).
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// Serialization error (Arvak extension).
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Result type for HAL operations.
pub type HalResult<T> = Result<T, HalError>;
