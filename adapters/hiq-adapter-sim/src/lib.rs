//! HIQ Local Simulator Backend
//!
//! This crate provides a local statevector simulator for testing
//! and development. It implements the Backend trait from hiq-hal.

mod simulator;
mod statevector;

pub use simulator::SimulatorBackend;
