//! Built-in compilation passes.

mod layout;
mod optimization;
mod routing;
mod translation;
pub mod verification;

pub use layout::TrivialLayout;
pub use optimization::{CancelCX, CommutativeCancellation, OneQubitBasis, Optimize1qGates};
pub use routing::BasicRouting;
pub use translation::BasisTranslation;
pub use verification::{MeasurementBarrierVerification, VerificationResult};
