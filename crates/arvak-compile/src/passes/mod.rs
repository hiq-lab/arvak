//! Built-in compilation passes.
//!
//! Passes are organized into two categories:
//! - [`agnostic`]: Target-agnostic passes that operate purely on DAG structure
//! - [`target`]: Target-specific passes that require hardware properties

pub mod agnostic;
pub mod target;

// Re-exports for backward compatibility
pub use agnostic::{
    CancelCX, CommutativeCancellation, MeasurementBarrierVerification, OneQubitBasis,
    Optimize1qGates, VerificationResult,
};
pub use target::{BasicRouting, BasisTranslation, TrivialLayout};
