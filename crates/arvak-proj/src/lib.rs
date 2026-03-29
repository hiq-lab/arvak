//! `arvak-proj` — Projector simulator with commensurability-guided MPS.
//!
//! Decomposes a quantum circuit into stable (analytically solvable) and
//! volatile (must-simulate) partitions using the sin(C/2) commensurability
//! filter from the Tilde Pattern (Hinderink 2026). Only the volatile
//! partition is simulated via MPS with adaptive per-bond truncation.
//!
//! # Architecture
//!
//! ```text
//! Circuit → frequency extraction → channel assessment → partitioning
//!     → stable (analytical) + volatile (MPS/QPU) → reassembly
//! ```
//!
//! # References
//!
//! - Hinderink (2026), "Pair-Counting Scaling of the Quantum Chaos Threshold"
//! - Hinderink (2026), "The Tilde Pattern: Commensurability as Low-Pass Filter"

pub mod channel;
pub mod error;
pub mod frequency;
pub mod mps;
pub mod partition;
pub mod reassembly;

#[cfg(test)]
mod accuracy;
#[cfg(test)]
mod bench;
#[cfg(test)]
mod shootout;
#[cfg(test)]
mod validation;
