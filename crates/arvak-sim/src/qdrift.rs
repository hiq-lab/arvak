//! QDrift stochastic product-formula synthesis.
//!
//! QDrift (Campbell 2019) approximates `exp(-i H t)` by randomly sampling
//! Hamiltonian terms with probability proportional to their coefficients,
//! rather than applying all terms uniformly.
//!
//! Algorithm:
//!   λ = Σ |c_k|
//!   τ = λ t / N                 (effective time per sample)
//!   For j = 1..N:
//!     Draw index k with p_k = |c_k| / λ
//!     Apply exp(-i · sign(c_k) · λ · τ · P_k)
//!       = exp(-i · sign(c_k) · λ² t / N · P_k)
//!
//! Error (in diamond norm): O(λ² t² / N).
//!
//! # Reference
//! E. Campbell, "Random Compiler for Fast Hamiltonian Simulation",
//! PRL 123, 070503 (2019). <https://doi.org/10.1103/PhysRevLett.123.070503>

use rand::Rng;
use tracing::debug;

use arvak_ir::Circuit;

use crate::error::{SimError, SimResult};
use crate::hamiltonian::{Hamiltonian, HamiltonianTerm};
use crate::synthesis::append_exp_pauli;

/// QDrift stochastic time-evolution synthesiser.
pub struct QDriftEvolution {
    hamiltonian: Hamiltonian,
    /// Total evolution time t.
    t: f64,
    /// Number of random samples N.
    n_samples: usize,
    /// Override circuit width; None → inferred from Hamiltonian.
    n_qubits: Option<u32>,
}

impl QDriftEvolution {
    /// Construct a new QDrift synthesiser.
    ///
    /// # Arguments
    /// * `hamiltonian` — the Hamiltonian H = Σ c_k P_k
    /// * `t`          — total evolution time
    /// * `n_samples`  — number of random channel samples N (higher → more accurate)
    pub fn new(hamiltonian: Hamiltonian, t: f64, n_samples: usize) -> Self {
        Self {
            hamiltonian,
            t,
            n_samples,
            n_qubits: None,
        }
    }

    /// Override circuit width.
    #[must_use]
    pub fn with_n_qubits(mut self, n: u32) -> Self {
        self.n_qubits = Some(n);
        self
    }

    /// Synthesise a QDrift circuit using the given random number generator.
    ///
    /// Seeding `rng` makes the circuit reproducible:
    /// ```rust,ignore
    /// use rand::SeedableRng;
    /// let rng = rand::rngs::SmallRng::seed_from_u64(42);
    /// let circuit = qdrift.circuit_with_rng(rng)?;
    /// ```
    pub fn circuit_with_rng<R: Rng>(&self, mut rng: R) -> SimResult<Circuit> {
        self.validate()?;

        let lambda = self.hamiltonian.lambda();
        if lambda == 0.0 {
            // All coefficients zero — trivial identity circuit.
            let n_qubits = self.effective_n_qubits();
            return Ok(Circuit::with_size("qdrift", n_qubits, 0));
        }

        let n_qubits = self.effective_n_qubits();
        // Effective time per sample: τ = λ t / N
        let tau = lambda * self.t / self.n_samples as f64;

        let mut circuit = Circuit::with_size("qdrift", n_qubits, 0);
        debug!(
            n_terms = self.hamiltonian.n_terms(),
            n_samples = self.n_samples,
            lambda,
            tau,
            n_qubits,
            "synthesising QDrift circuit"
        );

        // Build CDF for weighted sampling.
        let weights: Vec<f64> = self
            .hamiltonian
            .terms()
            .iter()
            .map(|t| t.coeff.abs() / lambda)
            .collect();

        for _ in 0..self.n_samples {
            let k = sample_index(&weights, &mut rng);
            let original = &self.hamiltonian.terms()[k];

            // Each sample implements exp(-i · sign(c_k) · λ · τ · P_k).
            // sign(c_k) · λ  is the effective coefficient for this draw.
            let eff_coeff = original.coeff.signum() * lambda;
            let sampled_term = HamiltonianTerm::new(eff_coeff, original.pauli.clone());
            append_exp_pauli(&mut circuit, &sampled_term, tau, n_qubits)?;
        }

        // Keep all qubits live in the DAG.
        for q in 0..n_qubits {
            circuit.rz(0.0f64, arvak_ir::QubitId(q))?;
        }

        Ok(circuit)
    }

    /// Synthesise a QDrift circuit using the thread-local RNG.
    pub fn circuit(&self) -> SimResult<Circuit> {
        self.circuit_with_rng(rand::thread_rng())
    }

    fn validate(&self) -> SimResult<()> {
        if self.hamiltonian.n_terms() == 0 {
            return Err(SimError::EmptyHamiltonian);
        }
        if self.n_samples == 0 {
            return Err(SimError::InvalidSamples(0));
        }
        Ok(())
    }

    fn effective_n_qubits(&self) -> u32 {
        self.n_qubits
            .unwrap_or_else(|| self.hamiltonian.min_qubits())
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Sample an index from a normalised probability distribution (CDF method).
fn sample_index<R: Rng>(weights: &[f64], rng: &mut R) -> usize {
    let u: f64 = rng.r#gen();
    let mut cumsum = 0.0;
    for (i, &w) in weights.iter().enumerate() {
        cumsum += w;
        if u < cumsum {
            return i;
        }
    }
    // Floating-point rounding: return last index.
    weights.len() - 1
}
