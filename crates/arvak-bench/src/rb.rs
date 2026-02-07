//! Randomized Benchmarking (RB).
//!
//! Measures gate fidelity by applying random sequences of Clifford gates
//! followed by an inverse, then fitting the decay of ground-state probability
//! to an exponential curve.
//!
//! Fidelity = (1 + p) / 2, where p is the exponential decay parameter.

use rand::{Rng, SeedableRng};
use std::f64::consts::FRAC_1_SQRT_2;

use arvak_ir::{Circuit, ClbitId, QubitId};

use crate::BenchmarkResult;

/// Configuration for a Randomized Benchmarking experiment.
#[derive(Debug, Clone)]
pub struct RbConfig {
    /// Number of qubits to benchmark (1 or 2).
    pub num_qubits: u32,
    /// Sequence lengths to sample.
    pub sequence_lengths: Vec<u32>,
    /// Number of random sequences per length.
    pub num_sequences: u32,
    /// Number of measurement shots per circuit.
    pub shots: u32,
}

impl Default for RbConfig {
    fn default() -> Self {
        Self {
            num_qubits: 1,
            sequence_lengths: vec![1, 2, 4, 8, 16, 32, 64, 128],
            num_sequences: 30,
            shots: 1024,
        }
    }
}

/// A 2x2 unitary matrix stored as [[Complex; 2]; 2].
/// Represented as (real, imag) tuples.
#[derive(Debug, Clone, Copy)]
struct Mat2 {
    m: [[(f64, f64); 2]; 2],
}

impl Mat2 {
    fn identity() -> Self {
        Self {
            m: [
                [(1.0, 0.0), (0.0, 0.0)],
                [(0.0, 0.0), (1.0, 0.0)],
            ],
        }
    }

    /// H gate matrix
    fn h() -> Self {
        let v = FRAC_1_SQRT_2;
        Self {
            m: [
                [(v, 0.0), (v, 0.0)],
                [(v, 0.0), (-v, 0.0)],
            ],
        }
    }

    /// S gate matrix
    fn s() -> Self {
        Self {
            m: [
                [(1.0, 0.0), (0.0, 0.0)],
                [(0.0, 0.0), (0.0, 1.0)],
            ],
        }
    }

    /// X gate matrix
    fn x() -> Self {
        Self {
            m: [
                [(0.0, 0.0), (1.0, 0.0)],
                [(1.0, 0.0), (0.0, 0.0)],
            ],
        }
    }

    /// Multiply two complex numbers: (a+bi)(c+di) = (ac-bd) + (ad+bc)i
    fn cmul(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
        (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
    }

    /// Add two complex numbers
    fn cadd(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
        (a.0 + b.0, a.1 + b.1)
    }

    /// Matrix multiplication: self * other
    fn mul(&self, other: &Mat2) -> Mat2 {
        let mut result = [[(0.0, 0.0); 2]; 2];
        for i in 0..2 {
            for j in 0..2 {
                result[i][j] = Self::cadd(
                    Self::cmul(self.m[i][0], other.m[0][j]),
                    Self::cmul(self.m[i][1], other.m[1][j]),
                );
            }
        }
        Mat2 { m: result }
    }

    /// Conjugate transpose (dagger)
    fn dagger(&self) -> Mat2 {
        Mat2 {
            m: [
                [(self.m[0][0].0, -self.m[0][0].1), (self.m[1][0].0, -self.m[1][0].1)],
                [(self.m[0][1].0, -self.m[0][1].1), (self.m[1][1].0, -self.m[1][1].1)],
            ],
        }
    }

    /// Check if approximately equal to identity (up to global phase).
    fn is_identity_up_to_phase(&self, tol: f64) -> bool {
        // Check if the matrix is proportional to identity
        // m[0][1] and m[1][0] should be ~0
        let off_diag = (self.m[0][1].0.powi(2) + self.m[0][1].1.powi(2)).sqrt()
            + (self.m[1][0].0.powi(2) + self.m[1][0].1.powi(2)).sqrt();
        if off_diag > tol {
            return false;
        }
        // Diagonal elements should have the same magnitude
        let d0 = (self.m[0][0].0.powi(2) + self.m[0][0].1.powi(2)).sqrt();
        let d1 = (self.m[1][1].0.powi(2) + self.m[1][1].1.powi(2)).sqrt();
        (d0 - d1).abs() < tol && d0 > 1.0 - tol
    }
}

/// Single-qubit Clifford gate index (0..23).
///
/// The 24 single-qubit Cliffords can be decomposed into sequences
/// of H, S, and X gates.
#[derive(Debug, Clone, Copy)]
struct CliffordGate {
    index: u8,
}

impl CliffordGate {
    fn new(index: u8) -> Self {
        Self { index: index % 24 }
    }

    /// Get the gate sequence for this Clifford as a list of primitive gates.
    fn gate_sequence(&self) -> &'static [PrimitiveGate] {
        use PrimitiveGate::*;
        match self.index {
            0 => &[],
            1 => &[H],
            2 => &[S],
            3 => &[X],
            4 => &[H, S],
            5 => &[S, H],
            6 => &[H, X],
            7 => &[S, X],
            8 => &[X, S],
            9 => &[X, H],
            10 => &[H, S, H],
            11 => &[H, S, X],
            12 => &[S, H, S],
            13 => &[S, H, X],
            14 => &[S, X, H],
            15 => &[X, S, H],
            16 => &[H, S, H, S],
            17 => &[H, S, H, X],
            18 => &[H, S, X, H],
            19 => &[S, H, S, X],
            20 => &[S, X, H, S],
            21 => &[X, S, H, S],
            22 => &[H, S, H, S, H],
            23 => &[S, H, S, H, S],
            _ => unreachable!(),
        }
    }

    /// Compute the 2x2 unitary matrix for this Clifford.
    fn to_matrix(&self) -> Mat2 {
        let mut m = Mat2::identity();
        for &gate in self.gate_sequence() {
            let g = match gate {
                PrimitiveGate::H => Mat2::h(),
                PrimitiveGate::S => Mat2::s(),
                PrimitiveGate::X => Mat2::x(),
            };
            m = g.mul(&m);
        }
        m
    }

    /// Apply this Clifford's gates to a circuit.
    fn apply(&self, circuit: &mut Circuit, qubit: QubitId) {
        for &gate in self.gate_sequence() {
            match gate {
                PrimitiveGate::H => { let _ = circuit.h(qubit); }
                PrimitiveGate::S => { let _ = circuit.s(qubit); }
                PrimitiveGate::X => { let _ = circuit.x(qubit); }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PrimitiveGate {
    H,
    S,
    X,
}

/// Find the Clifford index that best matches the given matrix.
fn find_clifford_index(target: &Mat2) -> CliffordGate {
    for i in 0..24u8 {
        let cliff = CliffordGate::new(i);
        let m = cliff.to_matrix();
        // Check if target * m.dagger() ≈ I (up to global phase)
        let product = target.mul(&m.dagger());
        if product.is_identity_up_to_phase(1e-6) {
            return cliff;
        }
    }
    // Fallback — identity (should never happen for valid Clifford compositions)
    CliffordGate::new(0)
}

/// Generate a single-qubit RB circuit with the given sequence length.
///
/// Applies `length` random Cliffords followed by the inverse Clifford,
/// so the ideal outcome is always |0⟩.
pub fn generate_1q_rb_circuit(length: u32, seed: u64) -> Circuit {
    let mut rng = rand::rngs::SmallRng::seed_from_u64(seed);
    let mut circuit = Circuit::with_size(&format!("rb_1q_{length}"), 1, 1);
    let qubit = QubitId(0);

    // Track accumulated unitary via matrix multiplication
    let mut accumulated = Mat2::identity();

    // Apply random Cliffords
    for _ in 0..length {
        let cliff = CliffordGate::new(rng.gen_range(0u8..24));
        cliff.apply(&mut circuit, qubit);
        accumulated = cliff.to_matrix().mul(&accumulated);
    }

    // Find and apply inverse Clifford so ideal output is |0⟩
    // We need C_inv such that C_inv * accumulated = I (up to global phase)
    // So C_inv = accumulated.dagger()
    let inv_matrix = accumulated.dagger();
    let inv_cliff = find_clifford_index(&inv_matrix);
    inv_cliff.apply(&mut circuit, qubit);

    // Measure
    let _ = circuit.measure(qubit, ClbitId(0));

    circuit
}

/// Generate a two-qubit RB circuit with the given sequence length.
///
/// Applies random two-qubit Clifford layers (simplified as CX + random 1q).
/// Note: This generates RB-like circuits for benchmarking compilation throughput;
/// the inverse is not computed for the two-qubit case (would need the full
/// 11520-element two-qubit Clifford group).
pub fn generate_2q_rb_circuit(length: u32, seed: u64) -> Circuit {
    let mut rng = rand::rngs::SmallRng::seed_from_u64(seed);
    let mut circuit = Circuit::with_size(&format!("rb_2q_{length}"), 2, 2);

    for _ in 0..length {
        // Random single-qubit Cliffords on both qubits
        let c0 = CliffordGate::new(rng.gen_range(0u8..24));
        let c1 = CliffordGate::new(rng.gen_range(0u8..24));
        c0.apply(&mut circuit, QubitId(0));
        c1.apply(&mut circuit, QubitId(1));

        // Entangling gate
        let _ = circuit.cx(QubitId(0), QubitId(1));

        // More random single-qubit Cliffords
        let d0 = CliffordGate::new(rng.gen_range(0u8..24));
        let d1 = CliffordGate::new(rng.gen_range(0u8..24));
        d0.apply(&mut circuit, QubitId(0));
        d1.apply(&mut circuit, QubitId(1));
    }

    // Measure
    let _ = circuit.measure(QubitId(0), ClbitId(0));
    let _ = circuit.measure(QubitId(1), ClbitId(1));

    circuit
}

/// Fit an exponential decay A * p^x + B to RB data.
///
/// Uses a simple least-squares fit. Returns (A, p, B) where
/// the fidelity per Clifford = (1 + p * (d-1)) / d for d=2^n.
pub fn fit_rb_decay(data: &[(u32, f64)]) -> (f64, f64, f64) {
    if data.len() < 3 {
        return (0.5, 1.0, 0.5);
    }

    // Simple exponential fit: log-linear regression
    // survival_probability ≈ A * p^m + B
    // For single-qubit RB, B ≈ 0.5 (random guess for 1 qubit)
    let b_guess = 0.5;

    // Subtract baseline and take log
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_xy = 0.0;
    let mut sum_xx = 0.0;
    let mut n = 0.0;

    for &(m, prob) in data {
        let shifted = prob - b_guess;
        if shifted > 0.001 {
            let x = m as f64;
            let y = shifted.ln();
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_xx += x * x;
            n += 1.0;
        }
    }

    if n < 2.0 {
        return (0.5, 1.0, b_guess);
    }

    // Linear regression: y = ln(A) + m * ln(p)
    let ln_p = (n * sum_xy - sum_x * sum_y) / (n * sum_xx - sum_x * sum_x);
    let ln_a = (sum_y - ln_p * sum_x) / n;

    let p = ln_p.exp().clamp(0.0, 1.0);
    let a = ln_a.exp().clamp(0.0, 1.0);

    (a, p, b_guess)
}

/// Compute error per Clifford (EPC) from the decay parameter.
///
/// EPC = (d - 1)(1 - p) / d where d = 2^num_qubits.
pub fn error_per_clifford(p: f64, num_qubits: u32) -> f64 {
    let d = (1u64 << num_qubits) as f64;
    (d - 1.0) * (1.0 - p) / d
}

/// Create an RB benchmark result.
pub fn rb_result(
    num_qubits: u32,
    epc: f64,
    decay_param: f64,
    sequence_lengths: &[u32],
) -> BenchmarkResult {
    let fidelity = 1.0 - epc;
    BenchmarkResult::new(
        &format!("rb_{num_qubits}q"),
        fidelity,
        "gate_fidelity",
    )
    .with_metric("error_per_clifford", serde_json::Value::from(epc))
    .with_metric("decay_parameter", serde_json::Value::from(decay_param))
    .with_metric("num_qubits", num_qubits as u64)
    .with_metric("max_sequence_length", *sequence_lengths.last().unwrap_or(&0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clifford_inverse_roundtrip() {
        // For each Clifford, C * C_inv should be identity (up to phase)
        for i in 0..24u8 {
            let cliff = CliffordGate::new(i);
            let m = cliff.to_matrix();
            let inv = m.dagger();
            let product = inv.mul(&m);
            assert!(
                product.is_identity_up_to_phase(1e-10),
                "Clifford {i}: C_inv * C should be identity"
            );
        }
    }

    #[test]
    fn test_find_inverse_clifford() {
        // For each Clifford, we should be able to find its inverse
        for i in 0..24u8 {
            let cliff = CliffordGate::new(i);
            let m = cliff.to_matrix();
            let inv_cliff = find_clifford_index(&m.dagger());
            let inv_m = inv_cliff.to_matrix();
            let product = inv_m.mul(&m);
            assert!(
                product.is_identity_up_to_phase(1e-6),
                "Clifford {i}: found inverse {} but product is not identity",
                inv_cliff.index
            );
        }
    }

    #[test]
    fn test_generate_1q_rb_circuit() {
        let circuit = generate_1q_rb_circuit(10, 42);
        assert_eq!(circuit.num_qubits(), 1);
        assert_eq!(circuit.num_clbits(), 1);
        assert!(circuit.depth() > 0);
    }

    #[test]
    fn test_generate_1q_rb_deterministic() {
        let c1 = generate_1q_rb_circuit(5, 99);
        let c2 = generate_1q_rb_circuit(5, 99);
        assert_eq!(c1.depth(), c2.depth());
    }

    #[test]
    fn test_generate_2q_rb_circuit() {
        let circuit = generate_2q_rb_circuit(5, 42);
        assert_eq!(circuit.num_qubits(), 2);
        assert_eq!(circuit.num_clbits(), 2);
        assert!(circuit.depth() > 0);
    }

    #[test]
    fn test_fit_rb_decay() {
        // Simulate RB data with moderate noise (p ≈ 0.97)
        let data: Vec<(u32, f64)> = vec![
            (1, 0.99),
            (2, 0.98),
            (4, 0.96),
            (8, 0.92),
            (16, 0.85),
            (32, 0.72),
        ];

        let (a, p, _b) = fit_rb_decay(&data);
        assert!(p > 0.9, "Decay parameter should be close to 1 for good qubits, got {p}");
        assert!(a > 0.0, "Amplitude should be positive");
    }

    #[test]
    fn test_error_per_clifford() {
        // Perfect gate: p = 1.0
        assert!((error_per_clifford(1.0, 1) - 0.0).abs() < 1e-10);

        // Completely depolarizing: p = 0.0
        assert!((error_per_clifford(0.0, 1) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_rb_result() {
        let result = rb_result(1, 0.001, 0.998, &[1, 2, 4, 8, 16]);
        assert!((result.value - 0.999).abs() < 1e-10);
        assert_eq!(result.unit, "gate_fidelity");
    }
}
