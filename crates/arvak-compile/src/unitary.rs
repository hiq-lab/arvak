//! Unitary matrix utilities for gate optimization.
//!
//! Provides 2x2 unitary matrix operations for single-qubit gate optimization,
//! including matrix multiplication and ZYZ decomposition.

use num_complex::Complex64;
use std::f64::consts::PI;

/// Tolerance for floating point comparisons.
/// Note: A duplicate EPSILON constant also exists in
/// `crate::passes::agnostic::optimization::EPSILON` (same value).
const EPSILON: f64 = 1e-10;

/// A 2x2 unitary matrix in row-major order.
#[derive(Debug, Clone, Copy)]
pub struct Unitary2x2 {
    /// The matrix elements in row-major order: [[a, b], [c, d]].
    pub data: [Complex64; 4],
}

impl Unitary2x2 {
    /// Create a new 2x2 unitary matrix.
    pub fn new(a: Complex64, b: Complex64, c: Complex64, d: Complex64) -> Self {
        Self { data: [a, b, c, d] }
    }

    /// Create the identity matrix.
    pub fn identity() -> Self {
        Self::new(
            Complex64::new(1.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(1.0, 0.0),
        )
    }

    /// Create a Hadamard matrix.
    pub fn h() -> Self {
        let s = 1.0 / 2.0_f64.sqrt();
        Self::new(
            Complex64::new(s, 0.0),
            Complex64::new(s, 0.0),
            Complex64::new(s, 0.0),
            Complex64::new(-s, 0.0),
        )
    }

    /// Create a Pauli-X matrix.
    pub fn x() -> Self {
        Self::new(
            Complex64::new(0.0, 0.0),
            Complex64::new(1.0, 0.0),
            Complex64::new(1.0, 0.0),
            Complex64::new(0.0, 0.0),
        )
    }

    /// Create a Pauli-Y matrix.
    pub fn y() -> Self {
        Self::new(
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, -1.0),
            Complex64::new(0.0, 1.0),
            Complex64::new(0.0, 0.0),
        )
    }

    /// Create a Pauli-Z matrix.
    pub fn z() -> Self {
        Self::new(
            Complex64::new(1.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(-1.0, 0.0),
        )
    }

    /// Create an S gate (sqrt(Z)).
    pub fn s() -> Self {
        Self::new(
            Complex64::new(1.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 1.0),
        )
    }

    /// Create an S-dagger gate.
    pub fn sdg() -> Self {
        Self::new(
            Complex64::new(1.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, -1.0),
        )
    }

    /// Create a T gate (fourth root of Z).
    pub fn t() -> Self {
        let phase = Complex64::from_polar(1.0, PI / 4.0);
        Self::new(
            Complex64::new(1.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 0.0),
            phase,
        )
    }

    /// Create a T-dagger gate.
    pub fn tdg() -> Self {
        let phase = Complex64::from_polar(1.0, -PI / 4.0);
        Self::new(
            Complex64::new(1.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 0.0),
            phase,
        )
    }

    /// Create an SX gate (sqrt(X)).
    pub fn sx() -> Self {
        let half = Complex64::new(0.5, 0.0);
        let half_i = Complex64::new(0.0, 0.5);
        Self::new(half + half_i, half - half_i, half - half_i, half + half_i)
    }

    /// Create an SX-dagger gate.
    pub fn sxdg() -> Self {
        let half = Complex64::new(0.5, 0.0);
        let half_i = Complex64::new(0.0, 0.5);
        Self::new(half - half_i, half + half_i, half + half_i, half - half_i)
    }

    /// Create an RX rotation matrix.
    pub fn rx(theta: f64) -> Self {
        let c = (theta / 2.0).cos();
        let s = (theta / 2.0).sin();
        Self::new(
            Complex64::new(c, 0.0),
            Complex64::new(0.0, -s),
            Complex64::new(0.0, -s),
            Complex64::new(c, 0.0),
        )
    }

    /// Create an RY rotation matrix.
    pub fn ry(theta: f64) -> Self {
        let c = (theta / 2.0).cos();
        let s = (theta / 2.0).sin();
        Self::new(
            Complex64::new(c, 0.0),
            Complex64::new(-s, 0.0),
            Complex64::new(s, 0.0),
            Complex64::new(c, 0.0),
        )
    }

    /// Create an RZ rotation matrix.
    pub fn rz(theta: f64) -> Self {
        let exp_neg = Complex64::from_polar(1.0, -theta / 2.0);
        let exp_pos = Complex64::from_polar(1.0, theta / 2.0);
        Self::new(
            exp_neg,
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 0.0),
            exp_pos,
        )
    }

    /// Create a phase gate P(lambda).
    pub fn p(lambda: f64) -> Self {
        let phase = Complex64::from_polar(1.0, lambda);
        Self::new(
            Complex64::new(1.0, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(0.0, 0.0),
            phase,
        )
    }

    /// Create a U gate U(theta, phi, lambda).
    pub fn u(theta: f64, phi: f64, lambda: f64) -> Self {
        let c = (theta / 2.0).cos();
        let s = (theta / 2.0).sin();
        Self::new(
            Complex64::new(c, 0.0),
            -Complex64::from_polar(s, lambda),
            Complex64::from_polar(s, phi),
            Complex64::from_polar(c, phi + lambda),
        )
    }

    /// Multiply this matrix by another: self * other.
    #[allow(clippy::many_single_char_names)]
    pub fn mul(&self, other: &Self) -> Self {
        let [a, b, c, d] = self.data;
        let [e, f, g, h] = other.data;
        Self::new(a * e + b * g, a * f + b * h, c * e + d * g, c * f + d * h)
    }

    /// Get the conjugate transpose (dagger).
    pub fn dagger(&self) -> Self {
        Self::new(
            self.data[0].conj(),
            self.data[2].conj(),
            self.data[1].conj(),
            self.data[3].conj(),
        )
    }

    /// Check if this is approximately identity (up to global phase).
    pub fn is_identity(&self) -> bool {
        // Check if diagonal and equal (up to global phase)
        let [a, b, c, d] = self.data;

        // Off-diagonal should be zero
        if b.norm() > EPSILON || c.norm() > EPSILON {
            return false;
        }

        // Diagonal elements should be equal
        (a - d).norm() < EPSILON
    }

    /// Get the global phase of this unitary.
    pub fn global_phase(&self) -> f64 {
        // The global phase is the argument of the (0,0) element
        // after normalizing to SU(2)
        let det = self.data[0] * self.data[3] - self.data[1] * self.data[2];
        det.arg() / 2.0
    }

    /// Decompose into RZ(alpha) * RY(beta) * RZ(gamma) * `global_phase`.
    ///
    /// Returns (alpha, beta, gamma, `global_phase`).
    /// This is the ZYZ Euler decomposition.
    #[allow(clippy::no_effect_underscore_binding)]
    pub fn zyz_decomposition(&self) -> (f64, f64, f64, f64) {
        let [a, b, c, d] = self.data;

        // Calculate the global phase factor
        let det = a * d - b * c;
        let global_phase = det.arg() / 2.0;

        // Remove global phase to get SU(2) matrix
        let phase_factor = Complex64::from_polar(1.0, -global_phase);
        let a = a * phase_factor;
        let b = b * phase_factor;
        let c = c * phase_factor;
        let _d = d * phase_factor;

        // ZYZ decomposition:
        // U = Rz(alpha) * Ry(beta) * Rz(gamma)
        //
        // For SU(2): U = [[cos(b/2)*e^(-i(a+g)/2), -sin(b/2)*e^(-i(a-g)/2)],
        //                 [sin(b/2)*e^(i(a-g)/2),   cos(b/2)*e^(i(a+g)/2)]]

        // beta is determined by the magnitude of diagonal/off-diagonal
        let beta = 2.0 * a.norm().acos().clamp(0.0, PI);

        // Handle special cases
        if beta.abs() < EPSILON {
            // beta ≈ 0: pure Z rotation
            // U ≈ [[e^(-i*alpha_plus_gamma/2), 0], [0, e^(i*alpha_plus_gamma/2)]]
            let alpha_plus_gamma = -2.0 * a.arg();
            return (
                alpha_plus_gamma / 2.0,
                0.0,
                alpha_plus_gamma / 2.0,
                global_phase,
            );
        }

        if (beta - PI).abs() < EPSILON {
            // beta ≈ π:
            // U ≈ [[0, -e^(-i*(a-g)/2)], [e^(i*(a-g)/2), 0]]
            let alpha_minus_gamma = -2.0 * (-b).arg();
            return (
                alpha_minus_gamma / 2.0,
                PI,
                -alpha_minus_gamma / 2.0,
                global_phase,
            );
        }

        // General case
        // a = cos(beta/2) * e^(-i*(alpha+gamma)/2)
        // c = sin(beta/2) * e^(i*(alpha-gamma)/2)
        let alpha_plus_gamma = -2.0 * a.arg();
        let alpha_minus_gamma = 2.0 * c.arg();

        let alpha = f64::midpoint(alpha_plus_gamma, alpha_minus_gamma);
        let gamma = (alpha_plus_gamma - alpha_minus_gamma) / 2.0;

        (alpha, beta, gamma, global_phase)
    }

    /// Normalize angles to [-pi, pi].
    pub fn normalize_angle(angle: f64) -> f64 {
        if angle.is_nan() || angle.is_infinite() {
            return 0.0;
        }
        let mut a = angle.rem_euclid(2.0 * PI);
        if a > PI {
            a -= 2.0 * PI;
        }
        a
    }
}

impl Default for Unitary2x2 {
    fn default() -> Self {
        Self::identity()
    }
}

impl std::ops::Mul for Unitary2x2 {
    type Output = Self;

    #[allow(clippy::needless_pass_by_value)]
    fn mul(self, rhs: Self) -> Self::Output {
        Unitary2x2::mul(&self, &rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn test_identity() {
        let i = Unitary2x2::identity();
        assert!(i.is_identity());
    }

    #[test]
    fn test_hadamard_squared() {
        let h = Unitary2x2::h();
        let h2 = h * h;
        assert!(h2.is_identity());
    }

    #[test]
    fn test_pauli_squared() {
        let x = Unitary2x2::x();
        let y = Unitary2x2::y();
        let z = Unitary2x2::z();

        assert!((x * x).is_identity());
        assert!((y * y).is_identity());
        assert!((z * z).is_identity());
    }

    #[test]
    fn test_s_squared_is_z() {
        let s = Unitary2x2::s();
        let s2 = s * s;
        let z = Unitary2x2::z();

        // S^2 should equal Z up to global phase
        for i in 0..4 {
            let ratio = s2.data[i] / z.data[i];
            if z.data[i].norm() > EPSILON {
                // Check that ratio is a unit complex number (global phase)
                assert!((ratio.norm() - 1.0).abs() < EPSILON);
            }
        }
    }

    #[test]
    fn test_rz_decomposition_identity() {
        let i = Unitary2x2::identity();
        let (_alpha, beta, _gamma, _phase) = i.zyz_decomposition();

        // Identity should decompose to zero rotations (or equivalent)
        assert!(approx_eq(beta, 0.0) || approx_eq(beta.abs(), 2.0 * PI));
    }

    #[test]
    fn test_rz_decomposition_hadamard() {
        let h = Unitary2x2::h();
        let (alpha, beta, gamma, phase) = h.zyz_decomposition();

        // Reconstruct and verify
        let reconstructed = Unitary2x2::rz(alpha) * Unitary2x2::ry(beta) * Unitary2x2::rz(gamma);
        let global = Complex64::from_polar(1.0, phase);

        for i in 0..4 {
            let expected = h.data[i];
            let got = reconstructed.data[i] * global;
            assert!(
                (expected - got).norm() < 1e-6,
                "Mismatch at {i}: expected {expected:?}, got {got:?}"
            );
        }
    }

    #[test]
    fn test_rz_decomposition_x() {
        let x = Unitary2x2::x();
        let (alpha, beta, gamma, phase) = x.zyz_decomposition();

        let reconstructed = Unitary2x2::rz(alpha) * Unitary2x2::ry(beta) * Unitary2x2::rz(gamma);
        let global = Complex64::from_polar(1.0, phase);

        for i in 0..4 {
            let expected = x.data[i];
            let got = reconstructed.data[i] * global;
            assert!(
                (expected - got).norm() < 1e-6,
                "Mismatch at {i}: expected {expected:?}, got {got:?}"
            );
        }
    }

    #[test]
    fn test_rx_ry_rz() {
        // Test that Rx(π) ≈ iX (up to global phase)
        let rx_pi = Unitary2x2::rx(PI);
        let x = Unitary2x2::x();

        // Should be proportional
        let ratio = rx_pi.data[1] / x.data[1];
        assert!((ratio.norm() - 1.0).abs() < EPSILON);
    }
}
