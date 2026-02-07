//! Standard quantum benchmark suite for Arvak.
//!
//! Implements industry-standard quantum benchmarks:
//! - **Quantum Volume (QV)**: Measures the effective size of a quantum computer
//! - **CLOPS**: Circuit Layer Operations Per Second (throughput benchmark)
//! - **Randomized Benchmarking (RB)**: Measures gate fidelity via random Clifford sequences

pub mod clops;
pub mod qv;
pub mod rb;

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Result of a benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    /// Name of the benchmark.
    pub name: String,
    /// Primary metric value.
    pub value: f64,
    /// Unit of the primary metric.
    pub unit: String,
    /// Total wall-clock time.
    pub duration: Duration,
    /// Additional metrics.
    pub metrics: serde_json::Map<String, serde_json::Value>,
}

impl BenchmarkResult {
    /// Create a new benchmark result.
    pub fn new(name: impl Into<String>, value: f64, unit: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value,
            unit: unit.into(),
            duration: Duration::ZERO,
            metrics: serde_json::Map::new(),
        }
    }

    /// Set the duration.
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Add a metric.
    pub fn with_metric(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.metrics.insert(key.into(), value.into());
        self
    }
}
