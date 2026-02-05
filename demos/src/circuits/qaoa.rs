//! QAOA (Quantum Approximate Optimization Algorithm) circuits.
//!
//! QAOA is a variational algorithm for combinatorial optimization problems.
//! It alternates between cost and mixer unitaries with tunable parameters.

use hiq_ir::Circuit;
use hiq_ir::qubit::QubitId;
use std::f64::consts::PI;

use crate::problems::Graph;

/// Generate a QAOA circuit for the Max-Cut problem.
///
/// QAOA consists of:
/// 1. Initial state: |+⟩^n (uniform superposition)
/// 2. For each layer p:
///    - Cost unitary: exp(-i γ C) where C encodes the graph
///    - Mixer unitary: exp(-i β B) where B = Σ Xⱼ
///
/// # Arguments
/// * `graph` - The Max-Cut graph
/// * `gamma` - Cost parameters (one per layer)
/// * `beta` - Mixer parameters (one per layer)
///
/// # Returns
/// A QAOA circuit with measurements.
pub fn qaoa_circuit(graph: &Graph, gamma: &[f64], beta: &[f64]) -> Circuit {
    assert_eq!(
        gamma.len(),
        beta.len(),
        "gamma and beta must have same length"
    );
    let p = gamma.len(); // Number of QAOA layers

    let n = graph.n_nodes;
    let mut circuit = Circuit::with_size("qaoa", n as u32, n as u32);

    // Step 1: Initialize |+⟩^n
    for q in 0..n {
        circuit.h(QubitId(q as u32)).unwrap();
    }

    // Step 2: Apply p layers of cost and mixer unitaries
    for layer in 0..p {
        // Cost unitary: exp(-i γ C)
        // For Max-Cut: C = -1/2 Σ_{(i,j)∈E} (1 - Z_i Z_j)
        // exp(-i γ C) = Π_{(i,j)∈E} exp(i γ/2 Z_i Z_j)
        apply_cost_unitary(&mut circuit, graph, gamma[layer]);

        // Mixer unitary: exp(-i β B) where B = Σ X_j
        // exp(-i β B) = Π_j exp(-i β X_j) = Π_j RX(2β)
        apply_mixer_unitary(&mut circuit, n, beta[layer]);
    }

    // Step 3: Measure all qubits
    circuit.measure_all().unwrap();

    circuit
}

/// Apply the cost unitary for Max-Cut.
///
/// For each edge (i,j), apply: RZZ(γ) = exp(-i γ/2 Z_i Z_j)
/// RZZ can be decomposed as: CNOT(i,j) · RZ(γ)[j] · CNOT(i,j)
fn apply_cost_unitary(circuit: &mut Circuit, graph: &Graph, gamma: f64) {
    for (i, j, weight) in &graph.edges {
        // RZZ(gamma * weight) decomposition
        let angle = gamma * weight;
        circuit.cx(QubitId(*i as u32), QubitId(*j as u32)).unwrap();
        circuit.rz(angle, QubitId(*j as u32)).unwrap();
        circuit.cx(QubitId(*i as u32), QubitId(*j as u32)).unwrap();
    }
}

/// Apply the mixer unitary.
///
/// For each qubit, apply RX(2β).
fn apply_mixer_unitary(circuit: &mut Circuit, n_qubits: usize, beta: f64) {
    let angle = 2.0 * beta;
    for q in 0..n_qubits {
        circuit.rx(angle, QubitId(q as u32)).unwrap();
    }
}

/// Generate a QAOA circuit without measurements (for expectation value calculation).
pub fn qaoa_circuit_no_measure(graph: &Graph, gamma: &[f64], beta: &[f64]) -> Circuit {
    assert_eq!(gamma.len(), beta.len());
    let p = gamma.len();
    let n = graph.n_nodes;

    let mut circuit = Circuit::with_size("qaoa", n as u32, 0);

    // Initialize |+⟩^n
    for q in 0..n {
        circuit.h(QubitId(q as u32)).unwrap();
    }

    // Apply p layers
    for layer in 0..p {
        apply_cost_unitary(&mut circuit, graph, gamma[layer]);
        apply_mixer_unitary(&mut circuit, n, beta[layer]);
    }

    circuit
}

/// Strategy for initializing QAOA parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InitStrategy {
    /// Linear interpolation: gamma increases, beta decreases.
    Linear,
    /// Fixed values: all gamma and beta are the same.
    Fixed,
    /// Trotterized adiabatic: mimics adiabatic evolution.
    TrotterizedAdiabatic,
    /// Random initialization within bounds.
    Random,
    /// Fourier-based initialization for smoother landscapes.
    Fourier,
}

/// Calculate initial parameters for QAOA.
///
/// Returns (gamma, beta) initialized according to the specified strategy.
/// Different strategies work better for different problem types.
pub fn initial_parameters(p: usize) -> (Vec<f64>, Vec<f64>) {
    initial_parameters_with_strategy(p, InitStrategy::TrotterizedAdiabatic)
}

/// Calculate initial parameters with a specific strategy.
///
/// # Strategies
/// - `Linear`: Simple linear interpolation, good baseline
/// - `Fixed`: All parameters equal, useful for single-layer QAOA
/// - `TrotterizedAdiabatic`: Mimics adiabatic evolution, often best for deeper circuits
/// - `Random`: Random in [0, π/2], useful with multiple restarts
/// - `Fourier`: Sine/cosine basis, smoother optimization landscape
pub fn initial_parameters_with_strategy(p: usize, strategy: InitStrategy) -> (Vec<f64>, Vec<f64>) {
    match strategy {
        InitStrategy::Linear => {
            // gamma starts small and increases, beta starts large and decreases
            let gamma: Vec<f64> = (0..p)
                .map(|i| PI / 4.0 * (i + 1) as f64 / p as f64)
                .collect();
            let beta: Vec<f64> = (0..p)
                .map(|i| PI / 4.0 * (p - i) as f64 / p as f64)
                .collect();
            (gamma, beta)
        }
        InitStrategy::Fixed => {
            // Fixed at typical good values for shallow QAOA
            let gamma = vec![PI / 4.0; p];
            let beta = vec![PI / 8.0; p];
            (gamma, beta)
        }
        InitStrategy::TrotterizedAdiabatic => {
            // Mimics adiabatic time evolution with linear schedule
            // s(t) goes from 0 to 1, with gamma ~ s and beta ~ (1-s)
            // This often gives the best results for p >= 2
            let dt = 1.0 / (p + 1) as f64;
            let gamma: Vec<f64> = (1..=p)
                .map(|i| {
                    let s = i as f64 * dt;
                    s * PI / 2.0 * dt
                })
                .collect();
            let beta: Vec<f64> = (1..=p)
                .map(|i| {
                    let s = i as f64 * dt;
                    (1.0 - s) * PI / 2.0 * dt
                })
                .collect();
            (gamma, beta)
        }
        InitStrategy::Random => {
            // Deterministic pseudo-random for reproducibility
            let mut seed: u64 = 42;
            let mut rand = || {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                (seed as f64 / u64::MAX as f64) * PI / 2.0
            };
            let gamma: Vec<f64> = (0..p).map(|_| rand()).collect();
            let beta: Vec<f64> = (0..p).map(|_| rand()).collect();
            (gamma, beta)
        }
        InitStrategy::Fourier => {
            // Use sine/cosine basis for smoother landscape
            // gamma_k = sum_j u_j * sin((j+1) * pi * k / (2p))
            // For simplicity, just use the first Fourier mode
            let gamma: Vec<f64> = (0..p)
                .map(|k| PI / 4.0 * ((k as f64 + 0.5) * PI / p as f64).sin())
                .collect();
            let beta: Vec<f64> = (0..p)
                .map(|k| PI / 4.0 * ((k as f64 + 0.5) * PI / p as f64).cos())
                .collect();
            (gamma, beta)
        }
    }
}

/// Calculate graph-aware initial parameters.
///
/// Uses properties of the graph to choose better starting parameters:
/// - `avg_degree`: Higher degree graphs need smaller gamma
/// - `n_nodes`: Larger graphs may need adjusted parameters
/// - `max_cut_bound`: Upper bound on max cut value
pub fn graph_aware_initial_parameters(graph: &Graph, p: usize) -> (Vec<f64>, Vec<f64>) {
    let n = graph.n_nodes as f64;
    let m = graph.edges.len() as f64;
    let avg_degree = if n > 0.0 { 2.0 * m / n } else { 1.0 };

    // Scale gamma inversely with average degree
    // Higher connectivity means smaller gamma steps work better
    let gamma_scale = 1.0 / avg_degree.sqrt();

    let (mut gamma, beta) = initial_parameters_with_strategy(p, InitStrategy::TrotterizedAdiabatic);

    // Scale gamma values
    for g in &mut gamma {
        *g *= gamma_scale;
    }

    (gamma, beta)
}

/// Bounds for QAOA parameters.
///
/// Constraining parameters to these bounds often improves optimization.
#[derive(Debug, Clone)]
pub struct ParameterBounds {
    /// Minimum gamma value.
    pub gamma_min: f64,
    /// Maximum gamma value.
    pub gamma_max: f64,
    /// Minimum beta value.
    pub beta_min: f64,
    /// Maximum beta value.
    pub beta_max: f64,
}

impl Default for ParameterBounds {
    fn default() -> Self {
        Self {
            gamma_min: 0.0,
            gamma_max: PI,
            beta_min: 0.0,
            beta_max: PI / 2.0,
        }
    }
}

impl ParameterBounds {
    /// Tight bounds for faster convergence on typical Max-Cut instances.
    pub fn tight() -> Self {
        Self {
            gamma_min: 0.0,
            gamma_max: PI / 2.0,
            beta_min: 0.0,
            beta_max: PI / 4.0,
        }
    }

    /// Clip parameters to bounds.
    pub fn clip(&self, gamma: &mut [f64], beta: &mut [f64]) {
        for g in gamma {
            *g = g.clamp(self.gamma_min, self.gamma_max);
        }
        for b in beta {
            *b = b.clamp(self.beta_min, self.beta_max);
        }
    }
}

/// Calculate the number of QAOA parameters.
pub fn num_parameters(p: usize) -> usize {
    2 * p // p gamma values + p beta values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qaoa_circuit() {
        let graph = Graph::square_4();
        let gamma = vec![0.5];
        let beta = vec![0.3];

        let circuit = qaoa_circuit(&graph, &gamma, &beta);

        assert_eq!(circuit.num_qubits(), 4);
        assert_eq!(circuit.num_clbits(), 4);
        assert!(circuit.depth() > 0);
    }

    #[test]
    fn test_qaoa_multi_layer() {
        let graph = Graph::square_4();
        let gamma = vec![0.1, 0.2, 0.3];
        let beta = vec![0.3, 0.2, 0.1];

        let circuit = qaoa_circuit(&graph, &gamma, &beta);

        assert_eq!(circuit.num_qubits(), 4);
    }

    #[test]
    fn test_initial_parameters() {
        let (gamma, beta) = initial_parameters(3);

        assert_eq!(gamma.len(), 3);
        assert_eq!(beta.len(), 3);

        // Gamma should be increasing
        assert!(gamma[0] < gamma[1]);
        assert!(gamma[1] < gamma[2]);

        // Beta should be decreasing
        assert!(beta[0] > beta[1]);
        assert!(beta[1] > beta[2]);
    }

    #[test]
    fn test_num_parameters() {
        assert_eq!(num_parameters(1), 2);
        assert_eq!(num_parameters(3), 6);
    }
}
