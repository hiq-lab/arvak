//! Optimization passes.

mod cancel;
mod optimize_1q;

#[cfg(test)]
mod tests;

pub use cancel::{CancelCX, CommutativeCancellation};
pub use optimize_1q::{OneQubitBasis, Optimize1qGates};

/// Tolerance for angle comparisons.
pub(super) const EPSILON: f64 = 1e-10;
