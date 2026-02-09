//! Benchmarking utilities for quantum algorithm performance.
//!
//! This module provides tools for measuring and comparing the performance
//! of quantum algorithms across different backends and configurations.

use std::time::{Duration, Instant};

/// Result of a benchmark run.
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    /// Name of the benchmark.
    pub name: String,
    /// Total execution time.
    pub total_time: Duration,
    /// Number of circuit evaluations.
    pub circuit_evaluations: usize,
    /// Average time per circuit evaluation.
    pub avg_time_per_evaluation: Duration,
    /// Result quality metric (algorithm-specific).
    pub quality_metric: f64,
    /// Additional metrics.
    pub metrics: Vec<(String, f64)>,
}

impl BenchmarkResult {
    /// Create a new benchmark result.
    pub fn new(name: impl Into<String>, total_time: Duration, circuit_evaluations: usize) -> Self {
        let avg = if circuit_evaluations > 0 {
            total_time / circuit_evaluations as u32
        } else {
            Duration::ZERO
        };

        Self {
            name: name.into(),
            total_time,
            circuit_evaluations,
            avg_time_per_evaluation: avg,
            quality_metric: 0.0,
            metrics: Vec::new(),
        }
    }

    /// Set the quality metric.
    pub fn with_quality(mut self, quality: f64) -> Self {
        self.quality_metric = quality;
        self
    }

    /// Add a custom metric.
    pub fn with_metric(mut self, name: impl Into<String>, value: f64) -> Self {
        self.metrics.push((name.into(), value));
        self
    }

    /// Get throughput in circuits per second.
    pub fn throughput(&self) -> f64 {
        if self.total_time.as_secs_f64() > 0.0 {
            self.circuit_evaluations as f64 / self.total_time.as_secs_f64()
        } else {
            0.0
        }
    }
}

/// Configuration for benchmark runs.
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    /// Number of warmup iterations.
    pub warmup_iterations: usize,
    /// Number of benchmark iterations.
    pub benchmark_iterations: usize,
    /// Whether to include detailed timing breakdown.
    pub detailed_timing: bool,
    /// Label for the backend being benchmarked.
    pub backend_label: String,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            warmup_iterations: 2,
            benchmark_iterations: 5,
            detailed_timing: false,
            backend_label: "simulator".to_string(),
        }
    }
}

impl BenchmarkConfig {
    /// Create a config for quick benchmarks.
    pub fn quick() -> Self {
        Self {
            warmup_iterations: 1,
            benchmark_iterations: 3,
            ..Default::default()
        }
    }

    /// Create a config for thorough benchmarks.
    pub fn thorough() -> Self {
        Self {
            warmup_iterations: 3,
            benchmark_iterations: 10,
            detailed_timing: true,
            ..Default::default()
        }
    }

    /// Set the backend label.
    pub fn with_backend(mut self, label: impl Into<String>) -> Self {
        self.backend_label = label.into();
        self
    }
}

/// Timer for benchmarking code sections.
pub struct BenchmarkTimer {
    start: Instant,
    laps: Vec<(String, Duration)>,
}

impl BenchmarkTimer {
    /// Start a new timer.
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
            laps: Vec::new(),
        }
    }

    /// Record a lap with a label.
    pub fn lap(&mut self, label: impl Into<String>) {
        let elapsed = self.start.elapsed();
        self.laps.push((label.into(), elapsed));
    }

    /// Get total elapsed time.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// Get lap times.
    pub fn laps(&self) -> &[(String, Duration)] {
        &self.laps
    }

    /// Reset the timer.
    pub fn reset(&mut self) {
        self.start = Instant::now();
        self.laps.clear();
    }
}

/// Compare results from different backends.
#[derive(Debug, Clone)]
pub struct BackendComparison {
    /// Results from each backend.
    pub results: Vec<BenchmarkResult>,
    /// Baseline backend name (for speedup calculations).
    pub baseline: Option<String>,
}

impl BackendComparison {
    /// Create a new comparison.
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
            baseline: None,
        }
    }

    /// Add a result.
    pub fn add_result(&mut self, result: BenchmarkResult) {
        self.results.push(result);
    }

    /// Set the baseline backend for speedup calculations.
    pub fn set_baseline(&mut self, name: impl Into<String>) {
        self.baseline = Some(name.into());
    }

    /// Calculate speedup relative to baseline.
    pub fn speedup(&self, name: &str) -> Option<f64> {
        let baseline_name = self.baseline.as_ref()?;
        let baseline_result = self.results.iter().find(|r| &r.name == baseline_name)?;
        let target_result = self.results.iter().find(|r| r.name == name)?;

        let baseline_time = baseline_result.total_time.as_secs_f64();
        let target_time = target_result.total_time.as_secs_f64();

        if target_time > 0.0 {
            Some(baseline_time / target_time)
        } else {
            None
        }
    }

    /// Get summary statistics.
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();
        lines.push("Backend Comparison:".to_string());
        lines.push("-".repeat(60));

        for result in &self.results {
            lines.push(format!(
                "{}: {:.3}s ({} evals, {:.1} circuits/s)",
                result.name,
                result.total_time.as_secs_f64(),
                result.circuit_evaluations,
                result.throughput()
            ));

            if let Some(baseline) = &self.baseline
                && result.name != *baseline
                && let Some(speedup) = self.speedup(&result.name)
            {
                lines.push(format!("  Speedup vs {baseline}: {speedup:.2}x"));
            }
        }

        lines.join("\n")
    }
}

impl Default for BackendComparison {
    fn default() -> Self {
        Self::new()
    }
}

/// Benchmark VQE performance.
pub fn benchmark_vqe(
    hamiltonian: &crate::problems::PauliHamiltonian,
    reps: usize,
    maxiter: usize,
    config: &BenchmarkConfig,
) -> BenchmarkResult {
    use crate::runners::VqeRunner;

    // Warmup
    for _ in 0..config.warmup_iterations {
        let runner = VqeRunner::new(hamiltonian.clone())
            .with_reps(reps)
            .with_maxiter(maxiter / 2);
        let _ = runner.run();
    }

    // Benchmark
    let mut total_time = Duration::ZERO;
    let mut total_evals = 0;
    let mut final_energy = 0.0;

    for _ in 0..config.benchmark_iterations {
        let runner = VqeRunner::new(hamiltonian.clone())
            .with_reps(reps)
            .with_maxiter(maxiter);

        let timer = Instant::now();
        let result = runner.run();
        total_time += timer.elapsed();
        total_evals += result.circuit_evaluations;
        final_energy = result.optimal_energy;
    }

    // Average over iterations
    let avg_time = total_time / config.benchmark_iterations as u32;
    let avg_evals = total_evals / config.benchmark_iterations;

    BenchmarkResult::new(
        format!("VQE ({} qubits)", hamiltonian.num_qubits()),
        avg_time,
        avg_evals,
    )
    .with_quality(final_energy)
    .with_metric("final_energy", final_energy)
    .with_metric("reps", reps as f64)
}

/// Benchmark QAOA performance.
pub fn benchmark_qaoa(
    graph: &crate::problems::Graph,
    layers: usize,
    maxiter: usize,
    config: &BenchmarkConfig,
) -> BenchmarkResult {
    use crate::runners::QaoaRunner;

    // Warmup
    for _ in 0..config.warmup_iterations {
        let runner = QaoaRunner::new(graph.clone())
            .with_layers(layers)
            .with_maxiter(maxiter / 2);
        let _ = runner.run();
    }

    // Benchmark
    let mut total_time = Duration::ZERO;
    let mut total_evals = 0;
    let mut best_ratio = 0.0;

    for _ in 0..config.benchmark_iterations {
        let runner = QaoaRunner::new(graph.clone())
            .with_layers(layers)
            .with_maxiter(maxiter);

        let timer = Instant::now();
        let result = runner.run();
        total_time += timer.elapsed();
        total_evals += result.circuit_evaluations;
        best_ratio = result.approximation_ratio;
    }

    // Average over iterations
    let avg_time = total_time / config.benchmark_iterations as u32;
    let avg_evals = total_evals / config.benchmark_iterations;

    BenchmarkResult::new(
        format!("QAOA ({} nodes)", graph.n_nodes),
        avg_time,
        avg_evals,
    )
    .with_quality(best_ratio)
    .with_metric("approximation_ratio", best_ratio)
    .with_metric("layers", layers as f64)
}

/// Run a scaling benchmark for VQE across different molecule sizes.
pub fn vqe_scaling_benchmark(config: &BenchmarkConfig) -> Vec<BenchmarkResult> {
    use crate::problems::{beh2_hamiltonian, h2_hamiltonian, h2o_hamiltonian, lih_hamiltonian};

    let hamiltonians = vec![
        ("H2 (2q)", h2_hamiltonian()),
        ("LiH (4q)", lih_hamiltonian()),
        ("BeH2 (6q)", beh2_hamiltonian()),
        ("H2O (8q)", h2o_hamiltonian()),
    ];

    hamiltonians
        .into_iter()
        .map(|(name, h)| {
            let n_qubits = h.num_qubits();
            let reps = if n_qubits <= 4 { 2 } else { 1 };
            let maxiter = if n_qubits <= 4 { 30 } else { 20 };

            let mut result = benchmark_vqe(&h, reps, maxiter, config);
            result.name = name.to_string();
            result
        })
        .collect()
}

/// Run a scaling benchmark for QAOA across different graph sizes.
pub fn qaoa_scaling_benchmark(config: &BenchmarkConfig) -> Vec<BenchmarkResult> {
    use crate::problems::Graph;

    let graphs = vec![
        ("4-node square", Graph::square_4()),
        ("4-node complete", Graph::complete_4()),
        ("6-node ring", Graph::ring_6()),
        ("6-node grid", Graph::grid_6()),
    ];

    graphs
        .into_iter()
        .map(|(name, g)| {
            let layers = 2;
            let maxiter = 30;

            let mut result = benchmark_qaoa(&g, layers, maxiter, config);
            result.name = name.to_string();
            result
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_timer() {
        let mut timer = BenchmarkTimer::start();
        std::thread::sleep(Duration::from_millis(10));
        timer.lap("first");
        std::thread::sleep(Duration::from_millis(10));
        timer.lap("second");

        assert!(timer.elapsed() >= Duration::from_millis(20));
        assert_eq!(timer.laps().len(), 2);
    }

    #[test]
    fn test_benchmark_result() {
        let result = BenchmarkResult::new("test", Duration::from_secs(2), 100)
            .with_quality(0.95)
            .with_metric("accuracy", 0.99);

        assert_eq!(result.throughput(), 50.0);
        assert_eq!(result.quality_metric, 0.95);
        assert_eq!(result.metrics.len(), 1);
    }

    #[test]
    fn test_backend_comparison() {
        let mut comparison = BackendComparison::new();

        comparison.add_result(BenchmarkResult::new("slow", Duration::from_secs(10), 100));
        comparison.add_result(BenchmarkResult::new("fast", Duration::from_secs(2), 100));
        comparison.set_baseline("slow");

        let speedup = comparison.speedup("fast").unwrap();
        assert!((speedup - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_quick_vqe_benchmark() {
        use crate::problems::h2_hamiltonian;

        let h = h2_hamiltonian();
        let config = BenchmarkConfig::quick();

        let result = benchmark_vqe(&h, 1, 10, &config);

        assert!(result.total_time > Duration::ZERO);
        assert!(result.circuit_evaluations > 0);
    }
}
