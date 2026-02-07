//! Target-specific compilation passes.
//!
//! These passes require target hardware properties from the PropertySet
//! (coupling map, basis gates, layout) and produce hardware-compatible
//! circuits for specific quantum devices.

pub mod layout;
pub mod routing;
pub mod translation;

pub use layout::TrivialLayout;
pub use routing::BasicRouting;
pub use translation::BasisTranslation;
