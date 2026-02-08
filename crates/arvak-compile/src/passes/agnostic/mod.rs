//! Target-agnostic compilation passes.
//!
//! These passes operate purely on the DAG structure without consulting
//! target-specific properties (coupling map, basis gates). They are safe
//! to run on any circuit regardless of the target hardware.

pub mod noise_injection;
pub mod optimization;
pub mod verification;

pub use noise_injection::NoiseInjectionPass;
pub use optimization::{CancelCX, CommutativeCancellation, OneQubitBasis, Optimize1qGates};
pub use verification::{MeasurementBarrierVerification, VerificationResult};
