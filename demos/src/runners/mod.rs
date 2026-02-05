//! Demo runners for executing quantum algorithms.

pub mod benchmark;
pub mod mitigation;
pub mod orchestrator;
pub mod qaoa;
pub mod scheduled;
pub mod vqe;

pub use benchmark::{
    BackendComparison, BenchmarkConfig, BenchmarkResult, BenchmarkTimer, benchmark_qaoa,
    benchmark_vqe, qaoa_scaling_benchmark, vqe_scaling_benchmark,
};
pub use mitigation::{MeasurementMitigator, MitigationConfig, ZneResult, zero_noise_extrapolation};
pub use orchestrator::run_multi_demo;
pub use qaoa::{QaoaResult, QaoaRunner};
pub use scheduled::{ScheduledDemoConfig, ScheduledDemoResult, ScheduledRunner};
pub use vqe::{VqeResult, VqeRunner};
