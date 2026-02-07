//! Classical Optimizers for VQE
//!
//! This module provides classical optimization algorithms for
//! the variational loop in VQE.

use std::f64::consts::PI;

/// Trait for classical optimizers
pub trait Optimizer {
    /// Take one optimization step given current parameters and measured cost
    fn step(&mut self, params: &[f64], cost: f64) -> Vec<f64>;

    /// Check if optimizer has converged
    fn converged(&self) -> bool;

    /// Get the best parameters found so far
    fn best_params(&self) -> Option<&[f64]>;

    /// Get the best cost found so far
    fn best_cost(&self) -> f64;
}

/// Nelder-Mead Simplex optimizer
///
/// A derivative-free optimizer that works well for noisy quantum
/// cost function evaluations. Uses a simplex of n+1 points in n dimensions.
pub struct NelderMeadOptimizer {
    /// Number of parameters
    num_params: usize,

    /// Parameter bounds
    bounds: Vec<(f64, f64)>,

    /// Convergence tolerance
    tolerance: f64,

    /// Simplex vertices (n+1 points, each with n parameters)
    simplex: Vec<Vec<f64>>,

    /// Cost at each simplex vertex
    simplex_costs: Vec<f64>,

    /// Current evaluation index within simplex
    eval_index: usize,

    /// Phase of the algorithm
    phase: NelderMeadPhase,

    /// Best parameters found
    best_params: Option<Vec<f64>>,

    /// Best cost found
    best_cost: f64,

    /// Trial point being evaluated
    trial_point: Vec<f64>,

    /// Trial cost
    trial_cost: f64,

    /// Iteration counter
    iteration: usize,

    /// Maximum iterations
    max_iterations: usize,

    /// Convergence flag
    converged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum NelderMeadPhase {
    /// Evaluating initial simplex
    InitialEval,
    /// Evaluating reflection point
    Reflection,
    /// Evaluating expansion point
    Expansion,
    /// Evaluating contraction point
    Contraction,
    /// Shrinking simplex
    Shrink,
}

impl NelderMeadOptimizer {
    /// Create a new Nelder-Mead optimizer
    pub fn new(num_params: usize) -> Self {
        Self {
            num_params,
            bounds: vec![(-PI, PI); num_params],
            tolerance: 1e-6,
            simplex: Vec::new(),
            simplex_costs: Vec::new(),
            eval_index: 0,
            phase: NelderMeadPhase::InitialEval,
            best_params: None,
            best_cost: f64::MAX,
            trial_point: Vec::new(),
            trial_cost: f64::MAX,
            iteration: 0,
            max_iterations: 1000,
            converged: false,
        }
    }

    /// Set parameter bounds
    pub fn with_bounds(mut self, bounds: Vec<(f64, f64)>) -> Self {
        assert_eq!(bounds.len(), self.num_params);
        self.bounds = bounds;
        self
    }

    /// Set convergence tolerance
    pub fn with_tolerance(mut self, tol: f64) -> Self {
        self.tolerance = tol;
        self
    }

    /// Set maximum iterations
    pub fn with_max_iterations(mut self, max_iter: usize) -> Self {
        self.max_iterations = max_iter;
        self
    }

    /// Project parameters onto bounds
    fn project(&self, params: &[f64]) -> Vec<f64> {
        params
            .iter()
            .enumerate()
            .map(|(i, &p)| p.clamp(self.bounds[i].0, self.bounds[i].1))
            .collect()
    }

    /// Initialize simplex around a starting point
    fn init_simplex(&mut self, center: &[f64]) {
        self.simplex.clear();
        self.simplex_costs.clear();

        // Initial step size
        let step = 0.5;

        // First vertex is the center
        self.simplex.push(center.to_vec());
        self.simplex_costs.push(f64::MAX);

        // Create n more vertices by stepping along each coordinate
        for i in 0..self.num_params {
            let mut vertex = center.to_vec();
            // Use different step if center[i] is zero
            let delta = if center[i].abs() < 1e-10 {
                step
            } else {
                step * center[i].abs()
            };
            vertex[i] += delta;
            self.simplex.push(self.project(&vertex));
            self.simplex_costs.push(f64::MAX);
        }

        self.eval_index = 0;
        self.phase = NelderMeadPhase::InitialEval;
    }

    /// Sort simplex by cost (best first)
    fn sort_simplex(&mut self) {
        let mut indices: Vec<usize> = (0..self.simplex.len()).collect();
        indices.sort_by(|&a, &b| {
            self.simplex_costs[a]
                .partial_cmp(&self.simplex_costs[b])
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let sorted_simplex: Vec<Vec<f64>> =
            indices.iter().map(|&i| self.simplex[i].clone()).collect();
        let sorted_costs: Vec<f64> = indices.iter().map(|&i| self.simplex_costs[i]).collect();

        self.simplex = sorted_simplex;
        self.simplex_costs = sorted_costs;
    }

    /// Compute centroid of all points except the worst
    fn centroid(&self) -> Vec<f64> {
        let n = self.simplex.len() - 1; // Exclude worst point
        let mut center = vec![0.0; self.num_params];

        for vertex in &self.simplex[..n] {
            for (c, &v) in center.iter_mut().zip(vertex.iter()) {
                *c += v;
            }
        }

        for c in &mut center {
            *c /= n as f64;
        }

        center
    }

    /// Check if simplex has converged
    fn check_convergence(&self) -> bool {
        if self.simplex_costs.is_empty() {
            return false;
        }

        // Check if cost spread is small
        let cost_spread = self
            .simplex_costs
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max)
            - self
                .simplex_costs
                .iter()
                .cloned()
                .fold(f64::INFINITY, f64::min);

        if cost_spread < self.tolerance {
            return true;
        }

        // Check if simplex is small
        let mut max_dist = 0.0f64;
        for i in 1..self.simplex.len() {
            let dist: f64 = self.simplex[0]
                .iter()
                .zip(self.simplex[i].iter())
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt();
            max_dist = max_dist.max(dist);
        }

        max_dist < self.tolerance
    }
}

impl Optimizer for NelderMeadOptimizer {
    fn step(&mut self, params: &[f64], cost: f64) -> Vec<f64> {
        self.iteration += 1;

        // Update best if improved
        if cost < self.best_cost {
            self.best_cost = cost;
            self.best_params = Some(params.to_vec());
        }

        // Initialize simplex on first call
        if self.simplex.is_empty() {
            self.init_simplex(params);
            self.simplex_costs[0] = cost;
            self.eval_index = 1;

            if self.eval_index < self.simplex.len() {
                return self.simplex[self.eval_index].clone();
            }
        }

        // State machine for Nelder-Mead algorithm
        match self.phase {
            NelderMeadPhase::InitialEval => {
                // Still evaluating initial simplex
                self.simplex_costs[self.eval_index] = cost;
                self.eval_index += 1;

                if self.eval_index < self.simplex.len() {
                    // More points to evaluate
                    return self.simplex[self.eval_index].clone();
                }

                // Initial simplex complete, sort and start optimization
                self.sort_simplex();

                if self.check_convergence() || self.iteration >= self.max_iterations {
                    self.converged = true;
                    return self.simplex[0].clone();
                }

                // Compute reflection point
                let centroid = self.centroid();
                let worst = &self.simplex[self.num_params];
                let alpha = 1.0; // Reflection coefficient

                self.trial_point = centroid
                    .iter()
                    .zip(worst.iter())
                    .map(|(&c, &w)| c + alpha * (c - w))
                    .collect();
                self.trial_point = self.project(&self.trial_point);

                self.phase = NelderMeadPhase::Reflection;
                self.trial_point.clone()
            }

            NelderMeadPhase::Reflection => {
                self.trial_cost = cost;
                let f_best = self.simplex_costs[0];
                let f_second_worst = self.simplex_costs[self.num_params - 1];
                let f_worst = self.simplex_costs[self.num_params];

                if self.trial_cost < f_best {
                    // Reflection is best so far, try expansion
                    let centroid = self.centroid();
                    let gamma = 2.0; // Expansion coefficient

                    let expansion: Vec<f64> = centroid
                        .iter()
                        .zip(self.trial_point.iter())
                        .map(|(&c, &r)| c + gamma * (r - c))
                        .collect();

                    self.trial_point = self.project(&expansion);
                    self.phase = NelderMeadPhase::Expansion;
                    self.trial_point.clone()
                } else if self.trial_cost < f_second_worst {
                    // Accept reflection
                    self.simplex[self.num_params] = self.trial_point.clone();
                    self.simplex_costs[self.num_params] = self.trial_cost;
                    self.sort_simplex();

                    if self.check_convergence() || self.iteration >= self.max_iterations {
                        self.converged = true;
                        return self.simplex[0].clone();
                    }

                    // Start new reflection
                    let centroid = self.centroid();
                    let worst = &self.simplex[self.num_params];
                    self.trial_point = centroid
                        .iter()
                        .zip(worst.iter())
                        .map(|(&c, &w)| c + (c - w))
                        .collect();
                    self.trial_point = self.project(&self.trial_point);
                    self.phase = NelderMeadPhase::Reflection;
                    self.trial_point.clone()
                } else {
                    // Try contraction
                    let centroid = self.centroid();
                    let rho = 0.5; // Contraction coefficient

                    let contract_point = if self.trial_cost < f_worst {
                        // Outside contraction
                        &self.trial_point
                    } else {
                        // Inside contraction
                        &self.simplex[self.num_params]
                    };

                    self.trial_point = centroid
                        .iter()
                        .zip(contract_point.iter())
                        .map(|(&c, &p)| c + rho * (p - c))
                        .collect();
                    self.trial_point = self.project(&self.trial_point);
                    self.phase = NelderMeadPhase::Contraction;
                    self.trial_point.clone()
                }
            }

            NelderMeadPhase::Expansion => {
                let f_reflection = self.trial_cost;
                let expansion_cost = cost;

                if expansion_cost < f_reflection {
                    // Accept expansion
                    self.simplex[self.num_params] = params.to_vec();
                    self.simplex_costs[self.num_params] = expansion_cost;
                } else {
                    // Accept reflection (need to recompute it)
                    let centroid = self.centroid();
                    let worst = &self.simplex[self.num_params];
                    let reflected: Vec<f64> = centroid
                        .iter()
                        .zip(worst.iter())
                        .map(|(&c, &w)| c + (c - w))
                        .collect();
                    self.simplex[self.num_params] = self.project(&reflected);
                    self.simplex_costs[self.num_params] = f_reflection;
                }

                self.sort_simplex();

                if self.check_convergence() || self.iteration >= self.max_iterations {
                    self.converged = true;
                    return self.simplex[0].clone();
                }

                // Start new iteration
                let centroid = self.centroid();
                let worst = &self.simplex[self.num_params];
                self.trial_point = centroid
                    .iter()
                    .zip(worst.iter())
                    .map(|(&c, &w)| c + (c - w))
                    .collect();
                self.trial_point = self.project(&self.trial_point);
                self.phase = NelderMeadPhase::Reflection;
                self.trial_point.clone()
            }

            NelderMeadPhase::Contraction => {
                let f_worst = self.simplex_costs[self.num_params];

                if cost < f_worst {
                    // Accept contraction
                    self.simplex[self.num_params] = params.to_vec();
                    self.simplex_costs[self.num_params] = cost;
                    self.sort_simplex();

                    if self.check_convergence() || self.iteration >= self.max_iterations {
                        self.converged = true;
                        return self.simplex[0].clone();
                    }

                    // Start new iteration
                    let centroid = self.centroid();
                    let worst = &self.simplex[self.num_params];
                    self.trial_point = centroid
                        .iter()
                        .zip(worst.iter())
                        .map(|(&c, &w)| c + (c - w))
                        .collect();
                    self.trial_point = self.project(&self.trial_point);
                    self.phase = NelderMeadPhase::Reflection;
                    self.trial_point.clone()
                } else {
                    // Shrink simplex toward best point
                    self.phase = NelderMeadPhase::Shrink;
                    self.eval_index = 1;

                    let sigma = 0.5; // Shrink coefficient
                    let best = self.simplex[0].clone();

                    for i in 1..self.simplex.len() {
                        self.simplex[i] = best
                            .iter()
                            .zip(self.simplex[i].iter())
                            .map(|(&b, &v)| b + sigma * (v - b))
                            .collect();
                        self.simplex[i] = self.project(&self.simplex[i]);
                    }

                    self.simplex[1].clone()
                }
            }

            NelderMeadPhase::Shrink => {
                self.simplex_costs[self.eval_index] = cost;
                self.eval_index += 1;

                if self.eval_index < self.simplex.len() {
                    return self.simplex[self.eval_index].clone();
                }

                // Shrink complete
                self.sort_simplex();

                if self.check_convergence() || self.iteration >= self.max_iterations {
                    self.converged = true;
                    return self.simplex[0].clone();
                }

                // Start new iteration
                let centroid = self.centroid();
                let worst = &self.simplex[self.num_params];
                self.trial_point = centroid
                    .iter()
                    .zip(worst.iter())
                    .map(|(&c, &w)| c + (c - w))
                    .collect();
                self.trial_point = self.project(&self.trial_point);
                self.phase = NelderMeadPhase::Reflection;
                self.trial_point.clone()
            }
        }
    }

    fn converged(&self) -> bool {
        self.converged
    }

    fn best_params(&self) -> Option<&[f64]> {
        self.best_params.as_deref()
    }

    fn best_cost(&self) -> f64 {
        self.best_cost
    }
}

/// SPSA (Simultaneous Perturbation Stochastic Approximation) optimizer
///
/// A gradient-free stochastic optimizer that estimates gradients using
/// only two function evaluations per iteration, regardless of dimension.
/// Very efficient for noisy quantum cost functions.
#[allow(dead_code)]
pub struct SpsaOptimizer {
    /// Number of parameters
    num_params: usize,

    /// Parameter bounds
    bounds: Vec<(f64, f64)>,

    /// Initial step size for gradient
    a: f64,

    /// Perturbation size
    c: f64,

    /// Decay parameters
    alpha: f64,
    gamma: f64,

    /// Stability constant
    big_a: f64,

    /// Iteration counter
    iteration: usize,

    /// Best parameters found
    best_params: Option<Vec<f64>>,

    /// Best cost found
    best_cost: f64,

    /// Current base point
    current_params: Vec<f64>,

    /// Random perturbation direction
    delta: Vec<f64>,

    /// Phase: 0 = need positive eval, 1 = need negative eval, 2 = update
    phase: usize,

    /// Cost at positive perturbation
    cost_plus: f64,

    /// Convergence tolerance
    tolerance: f64,

    /// Previous cost for convergence check
    prev_cost: f64,

    /// Convergence flag
    converged: bool,
}

#[allow(dead_code)]
impl SpsaOptimizer {
    /// Create a new SPSA optimizer
    pub fn new(num_params: usize) -> Self {
        Self {
            num_params,
            bounds: vec![(-PI, PI); num_params],
            a: 0.1,
            c: 0.1,
            alpha: 0.602,
            gamma: 0.101,
            big_a: 10.0,
            iteration: 0,
            best_params: None,
            best_cost: f64::MAX,
            current_params: vec![0.0; num_params],
            delta: Vec::new(),
            phase: 0,
            cost_plus: 0.0,
            tolerance: 1e-6,
            prev_cost: f64::MAX,
            converged: false,
        }
    }

    /// Set parameter bounds
    pub fn with_bounds(mut self, bounds: Vec<(f64, f64)>) -> Self {
        assert_eq!(bounds.len(), self.num_params);
        self.bounds = bounds;
        self
    }

    /// Set step size
    pub fn with_step_size(mut self, a: f64) -> Self {
        self.a = a;
        self
    }

    /// Set perturbation size
    pub fn with_perturbation(mut self, c: f64) -> Self {
        self.c = c;
        self
    }

    /// Project parameters onto bounds
    fn project(&self, params: &[f64]) -> Vec<f64> {
        params
            .iter()
            .enumerate()
            .map(|(i, &p)| p.clamp(self.bounds[i].0, self.bounds[i].1))
            .collect()
    }

    /// Generate random perturbation direction (Bernoulli ±1)
    fn generate_delta(&mut self) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        self.delta.clear();
        for i in 0..self.num_params {
            // Simple pseudo-random based on iteration and index
            let mut hasher = DefaultHasher::new();
            (self.iteration, i).hash(&mut hasher);
            let hash = hasher.finish();
            let sign = if hash.is_multiple_of(2) { 1.0 } else { -1.0 };
            self.delta.push(sign);
        }
    }

    /// Get current step sizes
    fn get_step_sizes(&self) -> (f64, f64) {
        let k = self.iteration as f64;
        let ak = self.a / (k + 1.0 + self.big_a).powf(self.alpha);
        let ck = self.c / (k + 1.0).powf(self.gamma);
        (ak, ck)
    }
}

impl Optimizer for SpsaOptimizer {
    fn step(&mut self, params: &[f64], cost: f64) -> Vec<f64> {
        // Update best if improved
        if cost < self.best_cost {
            self.best_cost = cost;
            self.best_params = Some(params.to_vec());
        }

        match self.phase {
            0 => {
                // Initialize or start new iteration
                if self.current_params.iter().all(|&x| x == 0.0) && self.iteration == 0 {
                    self.current_params = params.to_vec();
                }

                self.iteration += 1;
                self.generate_delta();

                let (_, ck) = self.get_step_sizes();

                // Compute positive perturbation
                let plus: Vec<f64> = self
                    .current_params
                    .iter()
                    .zip(self.delta.iter())
                    .map(|(&p, &d)| p + ck * d)
                    .collect();

                self.phase = 1;
                self.project(&plus)
            }

            1 => {
                // Received cost for positive perturbation
                self.cost_plus = cost;

                let (_, ck) = self.get_step_sizes();

                // Compute negative perturbation
                let minus: Vec<f64> = self
                    .current_params
                    .iter()
                    .zip(self.delta.iter())
                    .map(|(&p, &d)| p - ck * d)
                    .collect();

                self.phase = 2;
                self.project(&minus)
            }

            2 => {
                // Received cost for negative perturbation
                let cost_minus = cost;

                let (ak, ck) = self.get_step_sizes();

                // Estimate gradient and update
                let gradient: Vec<f64> = self
                    .delta
                    .iter()
                    .map(|&d| (self.cost_plus - cost_minus) / (2.0 * ck * d))
                    .collect();

                // Update parameters
                self.current_params = self
                    .current_params
                    .iter()
                    .zip(gradient.iter())
                    .map(|(&p, &g)| p - ak * g)
                    .collect();
                self.current_params = self.project(&self.current_params);

                // Check convergence
                let avg_cost = (self.cost_plus + cost_minus) / 2.0;
                if (avg_cost - self.prev_cost).abs() < self.tolerance {
                    self.converged = true;
                }
                self.prev_cost = avg_cost;

                self.phase = 0;
                self.current_params.clone()
            }

            _ => params.to_vec(),
        }
    }

    fn converged(&self) -> bool {
        self.converged
    }

    fn best_params(&self) -> Option<&[f64]> {
        self.best_params.as_deref()
    }

    fn best_cost(&self) -> f64 {
        self.best_cost
    }
}

// Keep CobylaOptimizer as an alias for NelderMead for backward compatibility
pub type CobylaOptimizer = NelderMeadOptimizer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nelder_mead_creation() {
        let opt = NelderMeadOptimizer::new(2);
        assert_eq!(opt.num_params, 2);
    }

    #[test]
    fn test_nelder_mead_quadratic() {
        // Test on simple quadratic f(x) = x^2
        let mut opt = NelderMeadOptimizer::new(1)
            .with_bounds(vec![(-5.0, 5.0)])
            .with_tolerance(1e-4);

        let mut params = vec![2.0]; // Start away from minimum

        for _ in 0..100 {
            let cost = params[0] * params[0]; // f(x) = x^2
            params = opt.step(&params, cost);

            if opt.converged() {
                break;
            }
        }

        // Should converge near x=0
        let best = opt.best_params().unwrap();
        assert!(best[0].abs() < 0.1, "Expected near 0, got {}", best[0]);
    }

    #[test]
    fn test_spsa_creation() {
        let opt = SpsaOptimizer::new(2)
            .with_step_size(0.1)
            .with_perturbation(0.05);
        assert_eq!(opt.num_params, 2);
    }

    #[test]
    fn test_spsa_quadratic() {
        // Test on simple quadratic f(x) = x^2
        let mut opt = SpsaOptimizer::new(1)
            .with_bounds(vec![(-5.0, 5.0)])
            .with_step_size(0.5)
            .with_perturbation(0.1);

        let mut params = vec![2.0]; // Start away from minimum

        for _ in 0..100 {
            let cost = params[0] * params[0]; // f(x) = x^2
            params = opt.step(&params, cost);
        }

        // Should move toward x=0
        assert!(
            opt.best_cost() < 4.0,
            "Should improve from initial cost of 4.0"
        );
    }

    #[test]
    fn test_optimizer_trait() {
        // Test that both optimizers implement the trait correctly
        fn test_optimizer<O: Optimizer>(mut opt: O) {
            let params = vec![1.0];
            let _ = opt.step(&params, 1.0);
            let _ = opt.best_cost();
            let _ = opt.converged();
        }

        test_optimizer(NelderMeadOptimizer::new(1));
        test_optimizer(SpsaOptimizer::new(1));
    }
}
