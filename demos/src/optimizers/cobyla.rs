//! COBYLA (Constrained Optimization BY Linear Approximation) optimizer.
//!
//! This is a derivative-free optimization algorithm suitable for
//! variational quantum algorithms where gradients are expensive.

use super::Optimizer;

/// Result of an optimization run.
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    /// Optimal parameter values.
    pub optimal_params: Vec<f64>,
    /// Optimal objective value.
    pub optimal_value: f64,
    /// Number of function evaluations.
    pub num_evaluations: usize,
    /// Number of iterations.
    pub num_iterations: usize,
    /// History of objective values.
    pub history: Vec<f64>,
    /// Whether the optimization converged.
    pub converged: bool,
}

/// COBYLA optimizer configuration.
#[derive(Debug, Clone)]
pub struct Cobyla {
    /// Maximum number of iterations.
    pub maxiter: usize,
    /// Convergence tolerance.
    pub tol: f64,
    /// Initial trust region radius.
    pub rhobeg: f64,
    /// Final trust region radius.
    pub rhoend: f64,
}

impl Default for Cobyla {
    fn default() -> Self {
        Self {
            maxiter: 100,
            tol: 1e-6,
            rhobeg: 0.5,
            rhoend: 1e-4,
        }
    }
}

impl Cobyla {
    /// Create a new COBYLA optimizer with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum iterations.
    pub fn with_maxiter(mut self, maxiter: usize) -> Self {
        self.maxiter = maxiter;
        self
    }

    /// Set convergence tolerance.
    pub fn with_tol(mut self, tol: f64) -> Self {
        self.tol = tol;
        self
    }

    /// Set trust region parameters.
    pub fn with_trust_region(mut self, rhobeg: f64, rhoend: f64) -> Self {
        self.rhobeg = rhobeg;
        self.rhoend = rhoend;
        self
    }
}

impl Optimizer for Cobyla {
    fn minimize<F>(&self, mut objective: F, initial_params: Vec<f64>) -> OptimizationResult
    where
        F: FnMut(&[f64]) -> f64,
    {
        // Simplified COBYLA implementation using Nelder-Mead-like simplex method
        // Real COBYLA uses linear approximations with a trust region
        // This is a simplified version for demo purposes

        let n = initial_params.len();
        let x = initial_params.clone();
        let mut f_x = objective(&x);
        let mut history = vec![f_x];
        let mut num_evaluations = 1;

        // Initialize simplex
        let mut simplex: Vec<Vec<f64>> = vec![x.clone()];
        let mut f_simplex: Vec<f64> = vec![f_x];

        for i in 0..n {
            let mut point = x.clone();
            point[i] += self.rhobeg;
            let f_point = objective(&point);
            num_evaluations += 1;
            simplex.push(point);
            f_simplex.push(f_point);
        }

        let mut rho = self.rhobeg;
        let mut converged = false;

        for _iteration in 0..self.maxiter {
            // Sort simplex by function value
            let mut indices: Vec<usize> = (0..=n).collect();
            indices.sort_by(|&a, &b| f_simplex[a].partial_cmp(&f_simplex[b]).unwrap());

            // Best, second worst, and worst
            let best_idx = indices[0];
            let worst_idx = indices[n];

            // Check convergence
            let spread = f_simplex[worst_idx] - f_simplex[best_idx];
            if spread < self.tol && rho <= self.rhoend {
                converged = true;
                break;
            }

            // Contract trust region if needed
            if spread < self.tol {
                rho = (rho * 0.5).max(self.rhoend);

                // Reset simplex around best point
                let best = simplex[best_idx].clone();
                simplex = vec![best.clone()];
                f_simplex = vec![f_simplex[best_idx]];

                for i in 0..n {
                    let mut point = best.clone();
                    point[i] += rho;
                    let f_point = objective(&point);
                    num_evaluations += 1;
                    simplex.push(point);
                    f_simplex.push(f_point);
                }
                continue;
            }

            // Calculate centroid of all points except worst
            let mut centroid = vec![0.0; n];
            for &idx in &indices[..n] {
                for i in 0..n {
                    centroid[i] += simplex[idx][i];
                }
            }
            for val in &mut centroid {
                *val /= n as f64;
            }

            // Reflection
            let mut reflected: Vec<f64> = centroid
                .iter()
                .zip(&simplex[worst_idx])
                .map(|(c, w)| 2.0 * c - w)
                .collect();

            // Bound the step size
            for i in 0..n {
                let diff = reflected[i] - centroid[i];
                if diff.abs() > rho {
                    reflected[i] = centroid[i] + rho * diff.signum();
                }
            }

            let f_reflected = objective(&reflected);
            num_evaluations += 1;

            if f_reflected < f_simplex[best_idx] {
                // Expansion
                let expanded: Vec<f64> = centroid
                    .iter()
                    .zip(&reflected)
                    .map(|(c, r)| c + 2.0 * (r - c))
                    .collect();
                let f_expanded = objective(&expanded);
                num_evaluations += 1;

                if f_expanded < f_reflected {
                    simplex[worst_idx] = expanded;
                    f_simplex[worst_idx] = f_expanded;
                } else {
                    simplex[worst_idx] = reflected;
                    f_simplex[worst_idx] = f_reflected;
                }
            } else if f_reflected < f_simplex[indices[n - 1]] {
                // Accept reflection
                simplex[worst_idx] = reflected;
                f_simplex[worst_idx] = f_reflected;
            } else {
                // Contraction
                let contracted: Vec<f64> = centroid
                    .iter()
                    .zip(&simplex[worst_idx])
                    .map(|(c, w)| 0.5 * (c + w))
                    .collect();
                let f_contracted = objective(&contracted);
                num_evaluations += 1;

                if f_contracted < f_simplex[worst_idx] {
                    simplex[worst_idx] = contracted;
                    f_simplex[worst_idx] = f_contracted;
                } else {
                    // Shrink
                    let best = simplex[best_idx].clone();
                    for i in 0..=n {
                        if i != best_idx {
                            for j in 0..n {
                                simplex[i][j] = 0.5 * (best[j] + simplex[i][j]);
                            }
                            f_simplex[i] = objective(&simplex[i]);
                            num_evaluations += 1;
                        }
                    }
                }
            }

            // Update best
            let min_idx = f_simplex
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .map(|(i, _)| i)
                .unwrap();

            if f_simplex[min_idx] < f_x {
                f_x = f_simplex[min_idx];
                history.push(f_x);
            }
        }

        // Find best point
        let min_idx = f_simplex
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap();

        OptimizationResult {
            optimal_params: simplex[min_idx].clone(),
            optimal_value: f_simplex[min_idx],
            num_evaluations,
            num_iterations: history.len(),
            history,
            converged,
        }
    }
}

/// Simple SPSA (Simultaneous Perturbation Stochastic Approximation) optimizer.
///
/// This is a gradient-free stochastic optimization algorithm that
/// estimates gradients using random perturbations.
#[derive(Debug, Clone)]
pub struct Spsa {
    /// Maximum number of iterations.
    pub maxiter: usize,
    /// Initial step size for gradient estimation.
    pub a: f64,
    /// Perturbation size.
    pub c: f64,
    /// Learning rate decay parameter.
    pub alpha: f64,
    /// Perturbation decay parameter.
    pub gamma: f64,
}

impl Default for Spsa {
    fn default() -> Self {
        Self {
            maxiter: 100,
            a: 0.1,
            c: 0.1,
            alpha: 0.602,
            gamma: 0.101,
        }
    }
}

impl Spsa {
    /// Create a new SPSA optimizer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum iterations.
    pub fn with_maxiter(mut self, maxiter: usize) -> Self {
        self.maxiter = maxiter;
        self
    }
}

impl Optimizer for Spsa {
    fn minimize<F>(&self, mut objective: F, initial_params: Vec<f64>) -> OptimizationResult
    where
        F: FnMut(&[f64]) -> f64,
    {
        let n = initial_params.len();
        let mut x = initial_params;
        let mut f_x = objective(&x);
        let mut history = vec![f_x];
        let mut num_evaluations = 1;

        // Simple LCG for reproducible randomness
        let mut rand_state: u64 = 42;
        let mut rand = || -> f64 {
            rand_state = rand_state.wrapping_mul(1103515245).wrapping_add(12345);
            if (rand_state >> 16) & 1 == 1 {
                1.0
            } else {
                -1.0
            }
        };

        for k in 0..self.maxiter {
            let a_k = self.a / (k + 1) as f64;
            let c_k = self.c / ((k + 1) as f64).powf(self.gamma);

            // Random perturbation direction
            let delta: Vec<f64> = (0..n).map(|_| rand()).collect();

            // Perturbed points
            let x_plus: Vec<f64> = x.iter().zip(&delta).map(|(xi, di)| xi + c_k * di).collect();
            let x_minus: Vec<f64> = x.iter().zip(&delta).map(|(xi, di)| xi - c_k * di).collect();

            // Evaluate
            let f_plus = objective(&x_plus);
            let f_minus = objective(&x_minus);
            num_evaluations += 2;

            // Gradient estimate
            let grad: Vec<f64> = delta
                .iter()
                .map(|di| (f_plus - f_minus) / (2.0 * c_k * di))
                .collect();

            // Update
            for i in 0..n {
                x[i] -= a_k * grad[i];
            }

            f_x = objective(&x);
            num_evaluations += 1;
            history.push(f_x);
        }

        OptimizationResult {
            optimal_params: x,
            optimal_value: f_x,
            num_evaluations,
            num_iterations: self.maxiter,
            history,
            converged: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cobyla_simple() {
        let cobyla = Cobyla::new().with_maxiter(200);

        // Minimize (x-1)^2 + (y-2)^2
        let result = cobyla.minimize(
            |params| {
                let x = params[0];
                let y = params[1];
                (x - 1.0).powi(2) + (y - 2.0).powi(2)
            },
            vec![0.0, 0.0],
        );

        assert!(result.optimal_value < 0.01);
        assert!((result.optimal_params[0] - 1.0).abs() < 0.1);
        assert!((result.optimal_params[1] - 2.0).abs() < 0.1);
    }

    #[test]
    fn test_cobyla_rosenbrock() {
        let cobyla = Cobyla::new().with_maxiter(500);

        // Rosenbrock function (minimum at (1, 1))
        let result = cobyla.minimize(
            |params| {
                let x = params[0];
                let y = params[1];
                (1.0 - x).powi(2) + 100.0 * (y - x.powi(2)).powi(2)
            },
            vec![0.0, 0.0],
        );

        // Rosenbrock is hard, just check we improved
        assert!(result.optimal_value < 1.0);
    }

    #[test]
    fn test_spsa_simple() {
        let spsa = Spsa::new().with_maxiter(100);

        // Minimize x^2 + y^2
        let result = spsa.minimize(
            |params| params[0].powi(2) + params[1].powi(2),
            vec![1.0, 1.0],
        );

        assert!(result.optimal_value < 0.5);
    }
}
