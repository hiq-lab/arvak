//! QAOA (Quantum Approximate Optimization Algorithm) runner.
//!
//! QAOA finds approximate solutions to combinatorial optimization problems.

use crate::circuits::qaoa::{
    InitStrategy, ParameterBounds, graph_aware_initial_parameters,
    initial_parameters_with_strategy, num_parameters, qaoa_circuit_no_measure,
};
use crate::optimizers::{Cobyla, Optimizer};
use crate::problems::Graph;

/// Result of a QAOA run.
#[derive(Debug, Clone)]
pub struct QaoaResult {
    /// Best cut value found.
    pub best_cut: f64,
    /// Best bitstring (node assignment).
    pub best_bitstring: usize,
    /// Optimal gamma parameters.
    pub optimal_gamma: Vec<f64>,
    /// Optimal beta parameters.
    pub optimal_beta: Vec<f64>,
    /// Number of iterations.
    pub iterations: usize,
    /// Number of circuit evaluations.
    pub circuit_evaluations: usize,
    /// Approximation ratio (`best_cut` / `max_cut`).
    pub approximation_ratio: f64,
    /// Energy history during optimization.
    pub energy_history: Vec<f64>,
}

/// QAOA runner configuration.
pub struct QaoaRunner {
    /// The graph to optimize.
    pub graph: Graph,
    /// Number of QAOA layers.
    pub p: usize,
    /// Number of measurement shots per evaluation.
    pub shots: u32,
    /// Maximum optimization iterations.
    pub maxiter: usize,
    /// Initialization strategy.
    pub init_strategy: InitStrategy,
    /// Use graph-aware initialization.
    pub use_graph_aware_init: bool,
    /// Parameter bounds for optimization.
    pub bounds: Option<ParameterBounds>,
}

impl QaoaRunner {
    /// Create a new QAOA runner.
    pub fn new(graph: Graph) -> Self {
        Self {
            graph,
            p: 1,
            shots: 1024,
            maxiter: 100,
            init_strategy: InitStrategy::TrotterizedAdiabatic,
            use_graph_aware_init: true,
            bounds: Some(ParameterBounds::tight()),
        }
    }

    /// Set the number of QAOA layers.
    pub fn with_layers(mut self, p: usize) -> Self {
        self.p = p;
        self
    }

    /// Set the number of shots.
    pub fn with_shots(mut self, shots: u32) -> Self {
        self.shots = shots;
        self
    }

    /// Set maximum iterations.
    pub fn with_maxiter(mut self, maxiter: usize) -> Self {
        self.maxiter = maxiter;
        self
    }

    /// Set the initialization strategy.
    pub fn with_init_strategy(mut self, strategy: InitStrategy) -> Self {
        self.init_strategy = strategy;
        self
    }

    /// Enable or disable graph-aware initialization.
    pub fn with_graph_aware_init(mut self, enabled: bool) -> Self {
        self.use_graph_aware_init = enabled;
        self
    }

    /// Set parameter bounds.
    pub fn with_bounds(mut self, bounds: ParameterBounds) -> Self {
        self.bounds = Some(bounds);
        self
    }

    /// Disable parameter bounds.
    pub fn without_bounds(mut self) -> Self {
        self.bounds = None;
        self
    }

    /// Run QAOA with automatic initial parameters.
    pub fn run(&self) -> QaoaResult {
        let (gamma, beta) = if self.use_graph_aware_init {
            graph_aware_initial_parameters(&self.graph, self.p)
        } else {
            initial_parameters_with_strategy(self.p, self.init_strategy)
        };
        let initial_params: Vec<f64> = gamma.into_iter().chain(beta).collect();
        self.run_with_params(initial_params)
    }

    /// Run QAOA with multiple random restarts and return the best result.
    pub fn run_with_restarts(&self, n_restarts: usize) -> QaoaResult {
        let mut best_result: Option<QaoaResult> = None;

        for restart in 0..n_restarts {
            // Use different initialization for each restart
            let (mut gamma, mut beta) = if restart == 0 && self.use_graph_aware_init {
                graph_aware_initial_parameters(&self.graph, self.p)
            } else {
                // Use random initialization with different seeds
                let mut seed: u64 = 42 + restart as u64 * 12345;
                let mut rand = || {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    (seed as f64 / u64::MAX as f64) * std::f64::consts::PI / 2.0
                };
                (
                    (0..self.p).map(|_| rand()).collect(),
                    (0..self.p).map(|_| rand()).collect(),
                )
            };

            // Apply bounds if configured
            if let Some(ref bounds) = self.bounds {
                bounds.clip(&mut gamma, &mut beta);
            }

            let initial_params: Vec<f64> = gamma.into_iter().chain(beta).collect();
            let result = self.run_with_params(initial_params);

            if best_result.is_none() || result.best_cut > best_result.as_ref().unwrap().best_cut {
                best_result = Some(result);
            }
        }

        best_result.unwrap()
    }

    /// Run QAOA with specified initial parameters.
    pub fn run_with_params(&self, initial_params: Vec<f64>) -> QaoaResult {
        let p = self.p;
        let graph = &self.graph;

        // Create optimizer
        let optimizer = Cobyla::new().with_maxiter(self.maxiter).with_tol(1e-4);

        // Objective function: minimize negative expected cut (maximize cut)
        let objective = |params: &[f64]| -> f64 {
            let gamma = &params[..p];
            let beta = &params[p..];
            -evaluate_expected_cut(graph, gamma, beta)
        };

        let result = optimizer.minimize(objective, initial_params);

        // Extract optimal parameters
        let optimal_gamma = result.optimal_params[..p].to_vec();
        let optimal_beta = result.optimal_params[p..].to_vec();

        // Sample the final distribution to find best bitstring
        let (best_bitstring, best_cut) = sample_best_solution(graph, &optimal_gamma, &optimal_beta);

        // Compute exact max cut for approximation ratio
        let (_, max_cut) = graph.max_cut_brute_force();
        let approximation_ratio = if max_cut > 0.0 {
            best_cut / max_cut
        } else {
            1.0
        };

        QaoaResult {
            best_cut,
            best_bitstring,
            optimal_gamma,
            optimal_beta,
            iterations: result.num_iterations,
            circuit_evaluations: result.num_evaluations,
            approximation_ratio,
            energy_history: result.history.iter().map(|x| -x).collect(),
        }
    }

    /// Get the number of parameters needed.
    pub fn num_parameters(&self) -> usize {
        num_parameters(self.p)
    }
}

/// Evaluate the expected cut value for given parameters.
fn evaluate_expected_cut(graph: &Graph, gamma: &[f64], beta: &[f64]) -> f64 {
    let circuit = qaoa_circuit_no_measure(graph, gamma, beta);
    let statevector = simulate_qaoa_statevector(&circuit, graph.n_nodes);

    // Calculate expected cut value
    let mut expected_cut = 0.0;
    for (i, &amplitude) in statevector.iter().enumerate() {
        let prob = amplitude.norm_sqr();
        let cut = graph.cut_value_from_bitstring(i);
        expected_cut += prob * cut;
    }

    expected_cut
}

/// Sample the best solution from the final QAOA state.
fn sample_best_solution(graph: &Graph, gamma: &[f64], beta: &[f64]) -> (usize, f64) {
    let circuit = qaoa_circuit_no_measure(graph, gamma, beta);
    let statevector = simulate_qaoa_statevector(&circuit, graph.n_nodes);

    // Find the bitstring with highest probability that also has good cut
    let mut best_bitstring = 0;
    let mut best_cut = 0.0;

    for (i, &amplitude) in statevector.iter().enumerate() {
        let prob = amplitude.norm_sqr();
        if prob > 0.01 {
            // Only consider states with significant probability
            let cut = graph.cut_value_from_bitstring(i);
            if cut > best_cut {
                best_cut = cut;
                best_bitstring = i;
            }
        }
    }

    // If no high-probability state found, find max probability state
    if best_cut == 0.0 {
        let (max_idx, _) = statevector
            .iter()
            .enumerate()
            .max_by(
                |(_, a): &(usize, &num_complex::Complex64),
                 (_, b): &(usize, &num_complex::Complex64)| {
                    a.norm_sqr().partial_cmp(&b.norm_sqr()).unwrap()
                },
            )
            .unwrap();
        best_bitstring = max_idx;
        best_cut = graph.cut_value_from_bitstring(max_idx);
    }

    (best_bitstring, best_cut)
}

/// Simplified statevector simulation for QAOA.
fn simulate_qaoa_statevector(
    circuit: &arvak_ir::Circuit,
    n_qubits: usize,
) -> Vec<num_complex::Complex64> {
    use num_complex::Complex64;

    let dim = 1 << n_qubits;
    let mut state = vec![Complex64::new(0.0, 0.0); dim];
    state[0] = Complex64::new(1.0, 0.0); // |0...0⟩

    // Apply gates from the circuit DAG
    for (_, instr) in circuit.dag().topological_ops() {
        if let arvak_ir::instruction::InstructionKind::Gate(gate) = &instr.kind {
            let qubits: Vec<usize> = instr.qubits.iter().map(|q| q.0 as usize).collect();

            if let arvak_ir::gate::GateKind::Standard(std_gate) = &gate.kind {
                apply_gate(&mut state, std_gate, &qubits);
            }
        }
    }

    state
}

/// Apply a standard gate to the statevector.
fn apply_gate(
    state: &mut [num_complex::Complex64],
    gate: &arvak_ir::gate::StandardGate,
    qubits: &[usize],
) {
    use arvak_ir::gate::StandardGate;
    use num_complex::Complex64;

    match gate {
        StandardGate::H => {
            let q = qubits[0];
            let h = std::f64::consts::FRAC_1_SQRT_2;
            for i in 0..state.len() {
                if (i >> q) & 1 == 0 {
                    let j = i | (1 << q);
                    let a = state[i];
                    let b = state[j];
                    state[i] = Complex64::new(h, 0.0) * (a + b);
                    state[j] = Complex64::new(h, 0.0) * (a - b);
                }
            }
        }
        StandardGate::X => {
            let q = qubits[0];
            for i in 0..state.len() {
                if (i >> q) & 1 == 0 {
                    let j = i | (1 << q);
                    state.swap(i, j);
                }
            }
        }
        StandardGate::Rx(param) => {
            if let Some(theta) = param.as_f64() {
                let q = qubits[0];
                let c = (theta / 2.0).cos();
                let s = (theta / 2.0).sin();
                for i in 0..state.len() {
                    if (i >> q) & 1 == 0 {
                        let j = i | (1 << q);
                        let a = state[i];
                        let b = state[j];
                        state[i] = Complex64::new(c, 0.0) * a - Complex64::new(0.0, s) * b;
                        state[j] = Complex64::new(0.0, -s) * a + Complex64::new(c, 0.0) * b;
                    }
                }
            }
        }
        StandardGate::Rz(param) => {
            if let Some(theta) = param.as_f64() {
                let q = qubits[0];
                let phase0 = Complex64::new((-theta / 2.0).cos(), (-theta / 2.0).sin());
                let phase1 = Complex64::new((theta / 2.0).cos(), (theta / 2.0).sin());
                for (i, amp) in state.iter_mut().enumerate() {
                    if (i >> q) & 1 == 0 {
                        *amp = phase0 * *amp;
                    } else {
                        *amp = phase1 * *amp;
                    }
                }
            }
        }
        StandardGate::CX => {
            let control = qubits[0];
            let target = qubits[1];
            for i in 0..state.len() {
                if (i >> control) & 1 == 1 && (i >> target) & 1 == 0 {
                    let j = i | (1 << target);
                    state.swap(i, j);
                }
            }
        }
        StandardGate::CZ => {
            let q0 = qubits[0];
            let q1 = qubits[1];
            for (i, amp) in state.iter_mut().enumerate() {
                if (i >> q0) & 1 == 1 && (i >> q1) & 1 == 1 {
                    *amp = -*amp;
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qaoa_runner_creation() {
        let graph = Graph::square_4();
        let runner = QaoaRunner::new(graph).with_layers(2).with_maxiter(10);

        assert_eq!(runner.p, 2);
        assert_eq!(runner.maxiter, 10);
    }

    #[test]
    fn test_qaoa_simple_run() {
        let graph = Graph::square_4();
        let runner = QaoaRunner::new(graph).with_layers(1).with_maxiter(20);

        let result = runner.run();

        // For a 4-node square, max cut is 4
        assert!(result.best_cut >= 2.0);
        assert!(result.approximation_ratio >= 0.5);
    }

    #[test]
    fn test_qaoa_expected_cut() {
        let graph = Graph::square_4();
        let gamma = vec![0.5];
        let beta = vec![0.3];

        let expected = evaluate_expected_cut(&graph, &gamma, &beta);

        // Expected cut should be positive for any reasonable parameters
        assert!(expected > 0.0);
        assert!(expected <= 4.0); // Max possible for this graph
    }
}
