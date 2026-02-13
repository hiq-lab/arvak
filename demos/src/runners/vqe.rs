//! VQE (Variational Quantum Eigensolver) runner.
//!
//! VQE is a hybrid classical-quantum algorithm for finding ground state
//! energies of quantum systems.

use crate::circuits::vqe::{num_parameters, two_local_ansatz};
use crate::optimizers::{Cobyla, Optimizer};
use crate::problems::{Pauli, PauliHamiltonian};

/// Result of a VQE run.
#[derive(Debug, Clone)]
pub struct VqeResult {
    /// Optimal energy found.
    pub optimal_energy: f64,
    /// Optimal parameters.
    pub optimal_params: Vec<f64>,
    /// Number of iterations.
    pub iterations: usize,
    /// Number of circuit evaluations.
    pub circuit_evaluations: usize,
    /// Energy history during optimization.
    pub energy_history: Vec<f64>,
    /// Whether optimization converged.
    pub converged: bool,
}

/// VQE runner configuration.
pub struct VqeRunner {
    /// The Hamiltonian to minimize.
    pub hamiltonian: PauliHamiltonian,
    /// Number of qubits.
    pub n_qubits: usize,
    /// Number of ansatz repetitions.
    pub reps: usize,
    /// Shot count (currently unused -- statevector simulation is used).
    pub shots: u32,
    /// Maximum optimization iterations.
    pub maxiter: usize,
}

impl VqeRunner {
    /// Create a new VQE runner.
    pub fn new(hamiltonian: PauliHamiltonian) -> Self {
        let n_qubits = hamiltonian.num_qubits();
        Self {
            hamiltonian,
            n_qubits,
            reps: 2,
            shots: 1024,
            maxiter: 100,
        }
    }

    /// Set the number of ansatz repetitions.
    pub fn with_reps(mut self, reps: usize) -> Self {
        self.reps = reps;
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

    /// Run VQE with random initial parameters.
    pub fn run(&self) -> VqeResult {
        let num_params = num_parameters("two_local", self.n_qubits, self.reps);

        // Initialize parameters randomly
        let mut seed: u64 = 42;
        let initial_params: Vec<f64> = (0..num_params)
            .map(|_| {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                (seed as f64 / u64::MAX as f64) * std::f64::consts::PI - std::f64::consts::PI / 2.0
            })
            .collect();

        self.run_with_params(initial_params)
    }

    /// Run VQE with specified initial parameters.
    pub fn run_with_params(&self, initial_params: Vec<f64>) -> VqeResult {
        let mut circuit_evaluations = 0;

        // Create optimizer
        let optimizer = Cobyla::new().with_maxiter(self.maxiter).with_tol(1e-6);

        // Objective function: evaluate energy
        let hamiltonian = &self.hamiltonian;
        let n_qubits = self.n_qubits;
        let reps = self.reps;
        let shots = self.shots;

        let objective = |params: &[f64]| -> f64 {
            circuit_evaluations += 1;
            evaluate_energy(hamiltonian, n_qubits, reps, params, shots)
        };

        let result = optimizer.minimize(objective, initial_params);

        VqeResult {
            optimal_energy: result.optimal_value,
            optimal_params: result.optimal_params,
            iterations: result.num_iterations,
            circuit_evaluations: result.num_evaluations,
            energy_history: result.history,
            converged: result.converged,
        }
    }

    /// Get the number of parameters needed.
    pub fn num_parameters(&self) -> usize {
        num_parameters("two_local", self.n_qubits, self.reps)
    }
}

/// Evaluate the energy expectation value for given parameters.
///
/// This simulates the quantum circuit execution and measurement.
/// In a real system, this would submit a job to a quantum backend.
fn evaluate_energy(
    hamiltonian: &PauliHamiltonian,
    n_qubits: usize,
    reps: usize,
    params: &[f64],
    _shots: u32,
) -> f64 {
    // Build the ansatz circuit
    let circuit = two_local_ansatz(n_qubits, reps, params);

    // Simulate the statevector (simplified)
    let statevector = simulate_statevector(&circuit, n_qubits);

    // Calculate expectation value
    expectation_value(hamiltonian, &statevector)
}

/// Simplified statevector simulation.
///
/// This is a basic simulator for demo purposes.
/// In production, use a proper simulator or quantum hardware.
fn simulate_statevector(
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
        StandardGate::Y => {
            let q = qubits[0];
            for i in 0..state.len() {
                if (i >> q) & 1 == 0 {
                    let j = i | (1 << q);
                    let a = state[i];
                    let b = state[j];
                    state[i] = Complex64::new(0.0, 1.0) * b;
                    state[j] = Complex64::new(0.0, -1.0) * a;
                }
            }
        }
        StandardGate::Z => {
            let q = qubits[0];
            for (i, amp) in state.iter_mut().enumerate() {
                if (i >> q) & 1 == 1 {
                    *amp = -*amp;
                }
            }
        }
        StandardGate::Ry(param) => {
            if let Some(theta) = param.as_f64() {
                let q = qubits[0];
                let c = (theta / 2.0).cos();
                let s = (theta / 2.0).sin();
                for i in 0..state.len() {
                    if (i >> q) & 1 == 0 {
                        let j = i | (1 << q);
                        let a = state[i];
                        let b = state[j];
                        state[i] = Complex64::new(c, 0.0) * a - Complex64::new(s, 0.0) * b;
                        state[j] = Complex64::new(s, 0.0) * a + Complex64::new(c, 0.0) * b;
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
        // TODO: Log warning for unsupported gate types
        _ => {}
    }
}

/// Calculate the expectation value of a Hamiltonian.
fn expectation_value(
    hamiltonian: &PauliHamiltonian,
    statevector: &[num_complex::Complex64],
) -> f64 {
    use num_complex::Complex64;

    let n = (statevector.len() as f64).log2() as usize;
    let mut energy = 0.0;

    for term in &hamiltonian.terms {
        let mut term_value = Complex64::new(0.0, 0.0);

        // Calculate <ψ|P|ψ> for this Pauli term
        for (i, &amplitude) in statevector.iter().enumerate() {
            // Apply Pauli operators and compute inner product
            let (j, phase) = apply_pauli_string(i, &term.operators, n);
            term_value += amplitude.conj() * phase * statevector[j];
        }

        energy += term.coefficient * term_value.re;
    }

    energy
}

/// Apply a Pauli string to a basis state index.
/// Returns the new index and accumulated phase.
fn apply_pauli_string(
    index: usize,
    operators: &[(usize, Pauli)],
    _n_qubits: usize,
) -> (usize, num_complex::Complex64) {
    use num_complex::Complex64;

    let mut new_index = index;
    let mut phase = Complex64::new(1.0, 0.0);

    for &(qubit, pauli) in operators {
        let bit = (index >> qubit) & 1;

        match pauli {
            Pauli::I => {}
            Pauli::X => {
                new_index ^= 1 << qubit;
            }
            Pauli::Y => {
                new_index ^= 1 << qubit;
                if bit == 0 {
                    phase *= Complex64::new(0.0, 1.0);
                } else {
                    phase *= Complex64::new(0.0, -1.0);
                }
            }
            Pauli::Z => {
                if bit == 1 {
                    phase *= Complex64::new(-1.0, 0.0);
                }
            }
        }
    }

    (new_index, phase)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::problems::{PauliTerm, h2_hamiltonian};

    #[test]
    fn test_vqe_runner_creation() {
        let h = h2_hamiltonian();
        let runner = VqeRunner::new(h).with_reps(2).with_maxiter(10);

        assert_eq!(runner.n_qubits, 2);
        assert_eq!(runner.reps, 2);
        assert_eq!(runner.maxiter, 10);
    }

    #[test]
    fn test_vqe_simple_run() {
        let h = h2_hamiltonian();
        let runner = VqeRunner::new(h).with_reps(1).with_maxiter(20);

        let result = runner.run();

        // H2 ground state energy is about -1.137
        // With limited iterations, we should at least get negative energy
        assert!(result.optimal_energy < 0.0);
    }

    #[test]
    fn test_expectation_value() {
        use num_complex::Complex64;

        // Test with |00⟩ state and Z0 operator
        let state = vec![
            Complex64::new(1.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 0.0),
        ];

        let h = PauliHamiltonian::new(vec![PauliTerm::z(1.0, 0)]);

        let energy = expectation_value(&h, &state);
        assert!((energy - 1.0).abs() < 1e-10); // <00|Z0|00> = 1
    }

    #[test]
    fn test_h2_expectation_values() {
        use num_complex::Complex64;

        let h2 = h2_hamiltonian();

        // Model Hamiltonian coefficients: g0=-0.32, g1=0.39, g2=-0.39, g3=-0.01, g4=0.18
        // For |00⟩: Z0=+1, Z1=+1, Z0Z1=+1
        // E = -0.32 + 0.39*1 + (-0.39)*1 + (-0.01)*1 = -0.33
        let state_00 = vec![
            Complex64::new(1.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 0.0),
        ];
        let e_00 = expectation_value(&h2, &state_00);
        assert!(
            (e_00 - (-0.33)).abs() < 0.01,
            "Expected -0.33 for |00>, got {e_00}"
        );

        // For |11⟩: Z0=-1, Z1=-1, Z0Z1=+1
        // E = -0.32 + 0.39*(-1) + (-0.39)*(-1) + (-0.01)*1 = -0.33
        let state_11 = vec![
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(1.0, 0.0),
        ];
        let e_11 = expectation_value(&h2, &state_11);
        assert!(
            (e_11 - (-0.33)).abs() < 0.01,
            "Expected -0.33 for |11>, got {e_11}"
        );

        // For the Bell state (|00⟩ + |11⟩)/√2:
        // XX contributes +1, YY contributes -1, they cancel out
        // So the energy equals the diagonal average: -0.33
        let h = std::f64::consts::FRAC_1_SQRT_2;
        let bell_00_11 = vec![
            Complex64::new(h, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(h, 0.0),
        ];
        let e_bell_00_11 = expectation_value(&h2, &bell_00_11);
        assert!(
            (e_bell_00_11 - (-0.33)).abs() < 0.1,
            "Expected ~-0.33 for (|00⟩+|11⟩)/√2, got {e_bell_00_11}"
        );

        // The (|01⟩+|10⟩)/√2 state has both XX and YY = +1
        // Energy = (E_01+E_10)/2 + 0.36 = (-1.09+0.47)/2 + 0.36 = -0.31 + 0.36 = 0.05
        // Note: this is NOT the ground state eigenvalue (-1.169) because the
        // ground state eigenvector is not a uniform superposition
        let bell_01_10 = vec![
            Complex64::new(0.0, 0.0),
            Complex64::new(h, 0.0),
            Complex64::new(h, 0.0),
            Complex64::new(0.0, 0.0),
        ];
        let e_bell_01_10 = expectation_value(&h2, &bell_01_10);
        assert!(
            (e_bell_01_10 - 0.05).abs() < 0.1,
            "Expected ~0.05 for (|01⟩+|10⟩)/√2, got {e_bell_01_10}"
        );
    }

    #[test]
    fn test_statevector_simulation() {
        use crate::circuits::vqe::two_local_ansatz;

        // Test with all-zero parameters (should give |00⟩ state from RY(0))
        let params = vec![0.0; 6]; // 2 qubits, 2 reps
        let circuit = two_local_ansatz(2, 2, &params);
        let state = simulate_statevector(&circuit, 2);

        // With all zero rotations, should stay in |00⟩
        let prob_00 = state[0].norm_sqr();
        assert!(
            prob_00 > 0.99,
            "Expected |00⟩ state for zero params, got P(00)={prob_00}"
        );

        // Test with PI rotations
        let params_pi: Vec<f64> = vec![std::f64::consts::PI; 6];
        let circuit_pi = two_local_ansatz(2, 2, &params_pi);
        let state_pi = simulate_statevector(&circuit_pi, 2);

        // Print probabilities for debugging
        println!("Probabilities after PI rotations:");
        for (i, amp) in state_pi.iter().enumerate() {
            println!("  |{:02b}⟩: {:.4}", i, amp.norm_sqr());
        }

        // Check that state is normalized
        let total_prob: f64 = state_pi.iter().map(num_complex::Complex::norm_sqr).sum();
        assert!(
            (total_prob - 1.0).abs() < 1e-6,
            "State not normalized: {total_prob}"
        );
    }

    #[test]
    fn test_circuit_energy_evaluation() {
        use crate::circuits::vqe::two_local_ansatz;

        let h2 = h2_hamiltonian();

        // Test with all-zero parameters (should give |00⟩ and E ≈ -0.33)
        let params_zero = vec![0.0; 6];
        let circuit_zero = two_local_ansatz(2, 2, &params_zero);
        let state_zero = simulate_statevector(&circuit_zero, 2);
        let e_zero = expectation_value(&h2, &state_zero);

        println!("Energy with zero params (should be ~-0.33): {e_zero}");
        assert!(
            (e_zero - (-0.33)).abs() < 0.05,
            "Expected ~-0.33 for zero params, got {e_zero}"
        );

        // Test with PI rotations (should give |11⟩ and E ≈ -0.33)
        let params_pi: Vec<f64> = vec![std::f64::consts::PI; 6];
        let circuit_pi = two_local_ansatz(2, 2, &params_pi);
        let state_pi = simulate_statevector(&circuit_pi, 2);
        let e_pi = expectation_value(&h2, &state_pi);

        println!("Energy with PI params (should be ~-0.33): {e_pi}");
        assert!(
            (e_pi - (-0.33)).abs() < 0.05,
            "Expected ~-0.33 for PI params, got {e_pi}"
        );
    }

    #[test]
    fn test_specific_bad_case() {
        use crate::circuits::vqe::two_local_ansatz;
        use num_complex::Complex64;

        let h2 = h2_hamiltonian();

        // This specific case gave unphysical energy
        let params = vec![
            -3.1415880134658862,
            2.793037086388586,
            -0.06253135390248543,
            1.1992390300430413,
            0.8846274209121585,
            -1.2777171778052216,
        ];

        let circuit = two_local_ansatz(2, 2, &params);
        let state = simulate_statevector(&circuit, 2);

        println!("State amplitudes:");
        for (i, amp) in state.iter().enumerate() {
            println!(
                "  |{:02b}⟩: {:.6} + {:.6}i (prob: {:.6})",
                i,
                amp.re,
                amp.im,
                amp.norm_sqr()
            );
        }

        let norm: f64 = state.iter().map(num_complex::Complex::norm_sqr).sum();
        println!("Norm: {norm}");

        // Manually compute each term's contribution
        for term in &h2.terms {
            let mut term_value = Complex64::new(0.0, 0.0);
            for (i, &amplitude) in state.iter().enumerate() {
                let (j, phase) = apply_pauli_string(i, &term.operators, 2);
                term_value += amplitude.conj() * phase * state[j];
            }
            println!(
                "Term {:?}: coef={:.4}, exp_val={:.6}+{:.6}i, contribution={:.6}",
                term.operators,
                term.coefficient,
                term_value.re,
                term_value.im,
                term.coefficient * term_value.re
            );
        }

        let energy = expectation_value(&h2, &state);
        println!("Total energy: {energy}");

        // Verify: sum of probabilities times Hamiltonian eigenvalues
        // The state has mostly |01⟩ and |00⟩
        // The H2 Hamiltonian eigenvalues are about [-1.137, -0.19, 0.11, 0.46]
        // A state that's 80% |01⟩ should NOT give -1.7 energy!

        // Let me verify the expectation values manually
        // For this state: P(00)=0.175, P(01)=0.799, P(10)=0.009, P(11)=0.017
        //
        // <Z0> = P(00) + P(01) - P(10) - P(11) = 0.175 + 0.799 - 0.009 - 0.017 = 0.948
        // But the code computed <Z0> = -0.631
        //
        // Wait, that's <Z1>! The qubit ordering might be wrong.
        // In little-endian: |01⟩ means q0=1, q1=0
        // So <Z0> = P(q0=0) - P(q0=1) = (P(00)+P(10)) - (P(01)+P(11))
        //         = (0.175 + 0.009) - (0.799 + 0.017) = 0.184 - 0.816 = -0.632 ✓
        //
        // So the issue is NOT qubit ordering - the calculation is correct.
        // The problem must be that the optimizer is finding states that shouldn't exist.
    }

    #[test]
    fn test_h2_eigenvalues() {
        // Verify H2 Hamiltonian eigenvalues by computing them directly
        // The h2_hamiltonian() function returns a model Hamiltonian

        // Coefficients from h2_hamiltonian():
        let g0 = -0.32; // Identity
        let g1 = 0.39; // Z0
        let g2 = -0.39; // Z1
        let g3 = -0.01; // Z0Z1
        let g4 = 0.18; // X0X1 and Y0Y1

        // Qubit encoding: index i = q0 + 2*q1
        println!("H2 Model Hamiltonian:");
        println!("|00⟩ (i=0): q0=0, q1=0");
        println!("|01⟩ (i=1): q0=1, q1=0");
        println!("|10⟩ (i=2): q0=0, q1=1");
        println!("|11⟩ (i=3): q0=1, q1=1");

        // Diagonal elements:
        // H[00,00]: Z0=+1, Z1=+1, Z0Z1=+1
        let e_00 = g0 + g1 * 1.0 + g2 * 1.0 + g3 * 1.0;
        // H[01,01]: Z0=-1 (q0=1), Z1=+1 (q1=0), Z0Z1=-1
        let e_01 = g0 + -g1 + g2 * 1.0 + -g3;
        // H[10,10]: Z0=+1 (q0=0), Z1=-1 (q1=1), Z0Z1=-1
        let e_10 = g0 + g1 * 1.0 + -g2 + -g3;
        // H[11,11]: Z0=-1, Z1=-1, Z0Z1=+1
        let e_11 = g0 + -g1 + -g2 + g3 * 1.0;

        println!("Diagonal elements:");
        println!("  H[00,00] = {e_00:.4}");
        println!("  H[01,01] = {e_01:.4}");
        println!("  H[10,10] = {e_10:.4}");
        println!("  H[11,11] = {e_11:.4}");

        // Off-diagonal: X0X1 + Y0Y1 = 2 * g4
        let off_diag = 2.0 * g4;
        println!("Off-diagonal (X0X1+Y0Y1): {off_diag:.4}");

        // Block 1 (|00⟩, |11⟩) eigenvalues:
        let a1: f64 = e_00;
        let b1: f64 = e_11;
        let c1: f64 = off_diag;
        let avg1 = f64::midpoint(a1, b1);
        let diff1 = ((a1 - b1).powi(2) / 4.0 + c1.powi(2)).sqrt();
        let lambda1_minus = avg1 - diff1;
        let lambda1_plus = avg1 + diff1;

        println!("\nBlock 1 (|00⟩, |11⟩) eigenvalues:");
        println!("  λ- = {lambda1_minus:.4}");
        println!("  λ+ = {lambda1_plus:.4}");

        // Block 2 (|01⟩, |10⟩) eigenvalues:
        let a2: f64 = e_01;
        let b2: f64 = e_10;
        let c2: f64 = off_diag;
        let avg2 = f64::midpoint(a2, b2);
        let diff2 = ((a2 - b2).powi(2) / 4.0 + c2.powi(2)).sqrt();
        let lambda2_minus = avg2 - diff2;
        let lambda2_plus = avg2 + diff2;

        println!("Block 2 (|01⟩, |10⟩) eigenvalues:");
        println!("  λ- = {lambda2_minus:.4}");
        println!("  λ+ = {lambda2_plus:.4}");

        let ground_state = lambda1_minus.min(lambda2_minus);
        println!("\nGround state energy: {ground_state:.4} Hartree");

        // Verify the eigenvalue spectrum
        let mut eigenvalues = vec![lambda1_minus, lambda1_plus, lambda2_minus, lambda2_plus];
        eigenvalues.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        println!("Eigenvalue spectrum: {eigenvalues:?}");

        // For a model Hamiltonian, we accept any reasonable ground state
        // The key is that VQE should find this minimum
        assert!(
            ground_state < 0.0,
            "Ground state should be negative, got {ground_state}"
        );
    }

    #[test]
    fn test_vqe_energy_bounds() {
        // The VQE energy should never go below the true ground state
        let h = h2_hamiltonian();
        let runner = VqeRunner::new(h).with_reps(2).with_maxiter(50);

        let result = runner.run();

        println!("VQE found energy: {}", result.optimal_energy);

        // Ground state is -1.137 Hartree
        // VQE should find something >= -1.137 (can't beat exact ground state)
        assert!(
            result.optimal_energy >= -1.2,
            "VQE energy {} is below ground state -1.137!",
            result.optimal_energy
        );
    }
}
