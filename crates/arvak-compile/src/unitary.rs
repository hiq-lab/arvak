//! Unitary matrix utilities for gate optimization.
//!
//! Provides 2x2 and 4x4 unitary matrix operations for single-qubit and
//! two-qubit gate optimization, including ZYZ decomposition and KAK
//! (Weyl chamber) decomposition.

use num_complex::Complex64;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

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

// ---- 4x4 unitary matrices for two-qubit gate optimization ----

/// A 4x4 unitary matrix in row-major order.
///
/// Used for two-qubit gate consolidation via KAK (Weyl chamber) decomposition.
/// Matrix indices follow the computational basis ordering: |00⟩, |01⟩, |10⟩, |11⟩.
#[derive(Debug, Clone)]
pub struct Unitary4x4 {
    /// The 16 matrix elements in row-major order.
    pub data: [Complex64; 16],
}

/// Result of the KAK (Weyl chamber) decomposition.
///
/// Any two-qubit unitary U ∈ U(4) decomposes as:
///   U = (A0 ⊗ A1) · Ud(tx, ty, tz) · (B0 ⊗ B1) · e^{iφ}
///
/// where Ud(tx, ty, tz) = exp(i(tx·XX + ty·YY + tz·ZZ))
/// and the interaction coefficients satisfy π/4 ≥ tx ≥ ty ≥ |tz| ≥ 0.
#[derive(Debug, Clone)]
pub struct WeylDecomposition {
    /// Single-qubit gate on qubit 0, applied after the interaction.
    pub a0: Unitary2x2,
    /// Single-qubit gate on qubit 1, applied after the interaction.
    pub a1: Unitary2x2,
    /// Single-qubit gate on qubit 0, applied before the interaction.
    pub b0: Unitary2x2,
    /// Single-qubit gate on qubit 1, applied before the interaction.
    pub b1: Unitary2x2,
    /// Weyl chamber interaction coefficient for XX.
    pub tx: f64,
    /// Weyl chamber interaction coefficient for YY.
    pub ty: f64,
    /// Weyl chamber interaction coefficient for ZZ.
    pub tz: f64,
    /// Global phase.
    pub global_phase: f64,
    /// Minimum number of CNOT gates needed to implement this unitary.
    pub num_cnots: u8,
}

impl Unitary4x4 {
    /// Create a 4x4 identity matrix.
    pub fn identity() -> Self {
        let zero = Complex64::new(0.0, 0.0);
        let one = Complex64::new(1.0, 0.0);
        Self {
            data: [
                one, zero, zero, zero, zero, one, zero, zero, zero, zero, one, zero, zero, zero,
                zero, one,
            ],
        }
    }

    /// Get element at (row, col).
    fn get(&self, row: usize, col: usize) -> Complex64 {
        self.data[row * 4 + col]
    }

    /// Set element at (row, col).
    fn set(&mut self, row: usize, col: usize, val: Complex64) {
        self.data[row * 4 + col] = val;
    }

    /// Multiply two 4x4 matrices: self * other.
    pub fn mul(&self, other: &Self) -> Self {
        let mut result = Self {
            data: [Complex64::new(0.0, 0.0); 16],
        };
        for i in 0..4 {
            for j in 0..4 {
                let mut sum = Complex64::new(0.0, 0.0);
                for k in 0..4 {
                    sum += self.get(i, k) * other.get(k, j);
                }
                result.set(i, j, sum);
            }
        }
        result
    }

    /// Conjugate transpose (dagger).
    pub fn dagger(&self) -> Self {
        let mut result = Self {
            data: [Complex64::new(0.0, 0.0); 16],
        };
        for i in 0..4 {
            for j in 0..4 {
                result.set(i, j, self.get(j, i).conj());
            }
        }
        result
    }

    /// Transpose (not conjugate).
    fn transpose(&self) -> Self {
        let mut result = Self {
            data: [Complex64::new(0.0, 0.0); 16],
        };
        for i in 0..4 {
            for j in 0..4 {
                result.set(i, j, self.get(j, i));
            }
        }
        result
    }

    /// Compute the trace of the 4x4 matrix.
    fn trace(&self) -> Complex64 {
        self.get(0, 0) + self.get(1, 1) + self.get(2, 2) + self.get(3, 3)
    }

    /// Compute the determinant of the 4x4 matrix.
    fn det(&self) -> Complex64 {
        // Expansion by minors along the first row.
        let mut result = Complex64::new(0.0, 0.0);
        for j in 0..4 {
            let sign = if j % 2 == 0 { 1.0 } else { -1.0 };
            result += Complex64::new(sign, 0.0) * self.get(0, j) * self.minor_3x3(0, j);
        }
        result
    }

    /// Compute the determinant of the 3x3 minor obtained by removing row `skip_r` and col `skip_c`.
    fn minor_3x3(&self, skip_r: usize, skip_c: usize) -> Complex64 {
        let mut m = [Complex64::new(0.0, 0.0); 9];
        let mut idx = 0;
        for i in 0..4 {
            if i == skip_r {
                continue;
            }
            for j in 0..4 {
                if j == skip_c {
                    continue;
                }
                m[idx] = self.get(i, j);
                idx += 1;
            }
        }
        // 3x3 determinant: m[0]*(m[4]*m[8]-m[5]*m[7]) - m[1]*(m[3]*m[8]-m[5]*m[6]) + m[2]*(m[3]*m[7]-m[4]*m[6])
        m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
            + m[2] * (m[3] * m[7] - m[4] * m[6])
    }

    /// Compute the tensor (Kronecker) product of two 2x2 matrices.
    pub fn kron(a: &Unitary2x2, b: &Unitary2x2) -> Self {
        let mut result = Self {
            data: [Complex64::new(0.0, 0.0); 16],
        };
        for i in 0..2 {
            for j in 0..2 {
                let a_ij = a.data[i * 2 + j];
                for k in 0..2 {
                    for l in 0..2 {
                        result.set(i * 2 + k, j * 2 + l, a_ij * b.data[k * 2 + l]);
                    }
                }
            }
        }
        result
    }

    /// Check if two 4x4 unitaries are equal up to global phase.
    pub fn equiv(&self, other: &Self) -> bool {
        let mut phase: Option<Complex64> = None;
        for i in 0..16 {
            let a = self.data[i];
            let b = other.data[i];
            if a.norm() < EPSILON && b.norm() < EPSILON {
                continue;
            }
            if a.norm() < EPSILON || b.norm() < EPSILON {
                return false;
            }
            let ratio = a / b;
            if let Some(ref p) = phase {
                if (ratio - *p).norm() > 1e-6 {
                    return false;
                }
            } else {
                phase = Some(ratio);
            }
        }
        true
    }

    /// Compute the unitary of a gate sequence on 2 qubits by simulating
    /// the circuit on all 4 computational basis states.
    ///
    /// Each gate is `(matrix_4x4_data, q0, q1)` — the qubit indices are 0 or 1
    /// indicating which of the two block qubits each gate acts on.
    pub fn from_gate_sequence_1q2q(
        gates_1q: &[([Complex64; 4], u8)],
        gates_2q: &[[Complex64; 16]],
        sequence: &[(bool, usize)], // (is_2q, index into gates_1q or gates_2q)
    ) -> Self {
        let mut result = Self {
            data: [Complex64::new(0.0, 0.0); 16],
        };

        // Simulate each basis state and record the output column.
        for basis in 0..4u8 {
            let mut sv = [Complex64::new(0.0, 0.0); 4];
            sv[basis as usize] = Complex64::new(1.0, 0.0);

            for &(is_2q, idx) in sequence {
                if is_2q {
                    apply_4x4(&mut sv, &gates_2q[idx]);
                } else {
                    let (ref mat, qubit) = gates_1q[idx];
                    apply_2x2_on_2q(&mut sv, mat, qubit);
                }
            }

            // Column `basis` of the unitary is the output statevector.
            for (row, amp) in sv.iter().enumerate() {
                result.set(row, basis as usize, *amp);
            }
        }
        result
    }

    /// Check if this unitary is approximately a tensor product A ⊗ B.
    ///
    /// Uses the operator Schmidt rank: reshape U as a 4×4 matrix and check
    /// if it has rank 1 (i.e., only one significant singular value).
    pub fn is_product_state(&self) -> bool {
        // Check: U is A⊗B iff for all i,j,k,l: U[i][j] * U[k][l] = U[i][l] * U[k][j]
        // (where indices are split as row=(q0,q1), col=(q0',q1'))
        // More practically: check if U[0][0]*U[1][1] - U[0][1]*U[1][0] ≈ 0 for all 2x2 blocks.
        // A simpler check: compute the "reshuffled" matrix and check rank.

        // Compute partial trace to check separability.
        // For a tensor product A⊗B, the matrix M_{(ik),(jl)} = U_{ij,kl} has rank 1.
        // Equivalently, for all 2x2 minors of the "reshaped" matrix, the determinant is 0.

        // Simple test: check if all 2x2 sub-blocks are proportional.
        // Block (i,j) = U[2i:2i+2, 2j:2j+2]. If U = A⊗B, block(i,j) = A[i][j] * B.
        // So block(0,0)/block(0,1) should equal block(1,0)/block(1,1) (as matrices).

        let tol = 1e-6;

        // Find the block with the largest norm.
        let mut ref_block = [Complex64::new(0.0, 0.0); 4];
        let mut ref_scale = Complex64::new(0.0, 0.0);
        let mut best_norm = 0.0;

        for bi in 0..2 {
            for bj in 0..2 {
                let n = self.get(2 * bi, 2 * bj).norm()
                    + self.get(2 * bi, 2 * bj + 1).norm()
                    + self.get(2 * bi + 1, 2 * bj).norm()
                    + self.get(2 * bi + 1, 2 * bj + 1).norm();
                if n > best_norm {
                    best_norm = n;
                    ref_block = [
                        self.get(2 * bi, 2 * bj),
                        self.get(2 * bi, 2 * bj + 1),
                        self.get(2 * bi + 1, 2 * bj),
                        self.get(2 * bi + 1, 2 * bj + 1),
                    ];
                    // Find the largest element to use as reference scale.
                    for &v in &ref_block {
                        if v.norm() > ref_scale.norm() {
                            ref_scale = v;
                        }
                    }
                }
            }
        }

        if ref_scale.norm() < tol {
            return true; // Zero matrix is trivially separable.
        }

        // Normalize reference block.
        let ref_normalized: Vec<Complex64> = ref_block.iter().map(|v| v / ref_scale).collect();

        // Check all blocks are proportional to the reference.
        for bi in 0..2 {
            for bj in 0..2 {
                let block = [
                    self.get(2 * bi, 2 * bj),
                    self.get(2 * bi, 2 * bj + 1),
                    self.get(2 * bi + 1, 2 * bj),
                    self.get(2 * bi + 1, 2 * bj + 1),
                ];

                // Find scale: block ≈ scale * ref_block
                let mut scale = Complex64::new(0.0, 0.0);
                let mut found = false;
                for (idx, &rv) in ref_normalized.iter().enumerate() {
                    if rv.norm() > tol {
                        scale = block[idx] / (rv * ref_scale);
                        found = true;
                        break;
                    }
                }

                if !found {
                    // Reference block is zero-like, check this block is also zero.
                    for &v in &block {
                        if v.norm() > tol {
                            return false;
                        }
                    }
                    continue;
                }

                // Check all elements are consistent.
                for (idx, &rv) in ref_normalized.iter().enumerate() {
                    let expected = scale * rv * ref_scale;
                    if (block[idx] - expected).norm() > tol * 10.0 {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Perform the KAK (Weyl chamber) decomposition.
    ///
    /// Any U ∈ U(4) decomposes as:
    ///   U = e^{iφ} · (A0 ⊗ A1) · Ud(tx,ty,tz) · (B0 ⊗ B1)
    ///
    /// The decomposition determines:
    /// - The minimum number of CNOT gates needed (0–3) via Makhlin invariants
    /// - The Weyl coordinates (tx, ty, tz) from eigenvalues of M^T·M
    /// - The local single-qubit unitaries (A0, A1, B0, B1) where feasible
    ///
    /// For product-state unitaries (0 CNOTs), the factors A⊗B are extracted.
    /// For entangling unitaries, the local unitary extraction may not be
    /// exact when eigenvalues are degenerate; in that case the raw 4×4
    /// matrix should be stored as a `CustomGate`.
    pub fn kak_decompose(&self) -> WeylDecomposition {
        let det = self.det();
        let global_phase = det.arg() / 4.0;

        // Early exit for product states (0 CNOTs).
        if self.is_product_state() {
            let (a0, a1) = factor_kron(self);
            return WeylDecomposition {
                a0,
                a1,
                b0: Unitary2x2::identity(),
                b1: Unitary2x2::identity(),
                tx: 0.0,
                ty: 0.0,
                tz: 0.0,
                global_phase,
                num_cnots: 0,
            };
        }

        // Normalize to SU(4) and transform to magic (Bell) basis.
        let phase_factor = Complex64::from_polar(1.0, -global_phase);
        let mut u_su4 = self.clone();
        for e in &mut u_su4.data {
            *e *= phase_factor;
        }
        let b_mat = magic_basis();
        let bd_mat = magic_dagger();
        let m = bd_mat.mul(&u_su4).mul(&b_mat);

        // Compute Q = M^T · M for Makhlin invariant classification.
        let mt = m.transpose();
        let q = mt.mul(&m);
        let tr_q = q.trace();
        let q2 = q.mul(&q);
        let tr_q2 = q2.trace();

        // Makhlin invariants: G₁ = tr(Q)²/16, G₂ = (tr(Q)²-tr(Q²))/4
        let g1 = tr_q * tr_q / 16.0;
        let g2 = (tr_q * tr_q - tr_q2) / 4.0;

        // Classify CNOT count from invariants.
        let num_cnots = cnot_count_from_invariants(g1, g2);

        // Extract Weyl coordinates from eigenvalues when non-degenerate.
        let eigs = eigenvalues_4x4(&q);
        let (tx, ty, tz) = extract_weyl_from_eigenvalues(&eigs, num_cnots);

        // For entangling gates, ConsolidateBlocks stores the raw 4×4 matrix
        // as a CustomGate. The local unitary extraction is only needed for
        // synthesis, which happens in a later pass. Set identity placeholders.
        let a0 = Unitary2x2::identity();
        let a1 = Unitary2x2::identity();
        let b0 = Unitary2x2::identity();
        let b1 = Unitary2x2::identity();

        WeylDecomposition {
            a0,
            a1,
            b0,
            b1,
            tx,
            ty,
            tz,
            global_phase,
            num_cnots,
        }
    }
}

/// Apply a 4x4 matrix to a 4-element statevector.
fn apply_4x4(sv: &mut [Complex64; 4], matrix: &[Complex64; 16]) {
    let old = *sv;
    for i in 0..4 {
        sv[i] = Complex64::new(0.0, 0.0);
        for j in 0..4 {
            sv[i] += matrix[i * 4 + j] * old[j];
        }
    }
}

/// Apply a 2x2 matrix to one qubit of a 2-qubit statevector.
/// qubit=0 is the MSB (|q0 q1⟩), qubit=1 is the LSB.
fn apply_2x2_on_2q(sv: &mut [Complex64; 4], matrix: &[Complex64; 4], qubit: u8) {
    let mask = if qubit == 0 { 2 } else { 1 };
    for i in 0..4 {
        if i & mask != 0 {
            continue;
        }
        let j = i | mask;
        let a = sv[i];
        let b = sv[j];
        sv[i] = matrix[0] * a + matrix[1] * b;
        sv[j] = matrix[2] * a + matrix[3] * b;
    }
}

/// The "magic" (Bell) basis change-of-basis matrix.
///
/// B = (1/√2) · [[1, 0, 0, i],
///                [0, i, 1, 0],
///                [0, i,-1, 0],
///                [1, 0, 0,-i]]
fn magic_basis() -> Unitary4x4 {
    let s = 1.0 / 2.0_f64.sqrt();
    let zero = Complex64::new(0.0, 0.0);
    Unitary4x4 {
        data: [
            Complex64::new(s, 0.0),
            zero,
            zero,
            Complex64::new(0.0, s),
            zero,
            Complex64::new(0.0, s),
            Complex64::new(s, 0.0),
            zero,
            zero,
            Complex64::new(0.0, s),
            Complex64::new(-s, 0.0),
            zero,
            Complex64::new(s, 0.0),
            zero,
            zero,
            Complex64::new(0.0, -s),
        ],
    }
}

/// B† (conjugate transpose of the magic basis).
fn magic_dagger() -> Unitary4x4 {
    magic_basis().dagger()
}

/// Build the canonical interaction unitary Ud(tx, ty, tz) = exp(i(tx·XX + ty·YY + tz·ZZ)).
///
/// In the computational basis, this is a diagonal matrix in the Bell basis.
/// Transformed back to the computational basis:
///   Ud = B · diag(e^{i(tx-ty+tz)}, e^{i(-tx+ty+tz)}, e^{i(tx+ty-tz)}, e^{i(-tx-ty-tz)}) · B†
///
/// More directly in the computational basis:
#[cfg(test)]
fn canonical_unitary(tx: f64, ty: f64, tz: f64) -> Unitary4x4 {
    let zero = Complex64::new(0.0, 0.0);

    // The canonical gate in the computational basis:
    // Ud = exp(i(tx XX + ty YY + tz ZZ))
    //
    // XX + YY = 2(|01⟩⟨10| + |10⟩⟨01|) and XX - YY = 2(|00⟩⟨11| + |11⟩⟨00|) (up to signs)
    //
    // In computational basis {|00⟩, |01⟩, |10⟩, |11⟩}:
    // The matrix is:
    // [[e^{iz} cos(tx-ty),  0,  0,  i·e^{iz} sin(tx-ty)],
    //  [0,  e^{-iz} cos(tx+ty),  i·e^{-iz} sin(tx+ty),  0],
    //  [0,  i·e^{-iz} sin(tx+ty),  e^{-iz} cos(tx+ty),  0],
    //  [i·e^{iz} sin(tx-ty),  0,  0,  e^{iz} cos(tx-ty)]]
    let eiz = Complex64::from_polar(1.0, tz);
    let emiz = Complex64::from_polar(1.0, -tz);
    let i = Complex64::new(0.0, 1.0);

    let cp = (tx + ty).cos();
    let sp = (tx + ty).sin();
    let cm = (tx - ty).cos();
    let sm = (tx - ty).sin();

    Unitary4x4 {
        data: [
            eiz * cm,
            zero,
            zero,
            i * eiz * sm,
            zero,
            emiz * cp,
            i * emiz * sp,
            zero,
            zero,
            i * emiz * sp,
            emiz * cp,
            zero,
            i * eiz * sm,
            zero,
            zero,
            eiz * cm,
        ],
    }
}

/// Solve a depressed cubic t³ + pt + q = 0 using Cardano's formula.
///
/// Returns one root. For complex coefficients this always works
/// (no casus irreducibilis).
fn solve_depressed_cubic(p: Complex64, q: Complex64) -> Complex64 {
    let zero = Complex64::new(0.0, 0.0);

    // If p ≈ 0: t³ = -q → t = cbrt(-q)
    if p.norm() < 1e-14 {
        return cbrt_complex(-q);
    }

    // Discriminant: D = q²/4 + p³/27
    let disc = q * q / 4.0 + p * p * p / 27.0;
    let sqrt_disc = disc.sqrt();

    let s = cbrt_complex(-q / 2.0 + sqrt_disc);
    let t = cbrt_complex(-q / 2.0 - sqrt_disc);

    // Guard: if s + t gives a near-zero result and the cubic should have
    // a non-zero root, try the other cube root branch.
    let root = s + t;

    // Verify and return. For numerical stability, do one Newton step.
    let f = root * root * root + p * root + q;
    let fp = root * root * 3.0 + p;
    if fp.norm() > 1e-30 {
        root - f / fp
    } else {
        if root.norm() < 1e-12 {
            return zero;
        }
        root
    }
}

/// Complex cube root: returns the principal cube root.
fn cbrt_complex(z: Complex64) -> Complex64 {
    if z.norm() < 1e-30 {
        return Complex64::new(0.0, 0.0);
    }
    let r = z.norm().cbrt();
    let theta = z.arg() / 3.0;
    Complex64::from_polar(r, theta)
}

/// Solve a quartic λ⁴ + aλ³ + bλ² + cλ + d = 0 using Ferrari's method.
///
/// Returns all four roots.
fn solve_quartic(a: Complex64, b: Complex64, c: Complex64, d: Complex64) -> [Complex64; 4] {
    // Step 1: Reduce to depressed quartic by substituting λ = t - a/4.
    // t⁴ + pt² + qt + r = 0
    let a2 = a * a;
    let a3 = a2 * a;
    let a4 = a2 * a2;
    let p = b - a2 * 3.0 / 8.0;
    let q = a3 / 8.0 - a * b / 2.0 + c;
    let r = -a4 * 3.0 / 256.0 + a2 * b / 16.0 - a * c / 4.0 + d;

    let shift = -a / 4.0;

    // If q ≈ 0, the depressed quartic is biquadratic: t⁴ + pt² + r = 0.
    if q.norm() < 1e-12 {
        let disc = p * p - r * 4.0;
        let sqrt_disc = disc.sqrt();
        let u1 = (-p + sqrt_disc) / 2.0;
        let u2 = (-p - sqrt_disc) / 2.0;
        let s1 = u1.sqrt();
        let s2 = u2.sqrt();
        return [s1 + shift, -s1 + shift, s2 + shift, -s2 + shift];
    }

    // Step 2: Resolvent cubic.
    // y³ - (p)y² - 4ry + (4pr - q²) = 0
    // In standard form: y³ + Ay² + By + C = 0 where A = -p, B = -4r, C = 4pr - q²
    let ca = -p;
    let cb = -r * 4.0;
    let cc = p * r * 4.0 - q * q;

    // Reduce to depressed cubic: y = t - ca/3
    let dp = cb - ca * ca / 3.0;
    let dq = ca * ca * ca * 2.0 / 27.0 - ca * cb / 3.0 + cc;

    let t0 = solve_depressed_cubic(dp, dq);
    let y0 = t0 - ca / 3.0;

    // Step 3: Factor the depressed quartic using y0.
    // (t² + y0)² = (y0 - p)t² - qt + (y0² - r)
    // The RHS should be a perfect square: (√(y0-p) · t - q/(2√(y0-p)))²
    let w2 = y0 - p;
    let w = w2.sqrt();

    // Two quadratics:
    // t² + wt + (y0 + q/(2w)) = 0
    // t² - wt + (y0 - q/(2w)) = 0
    let half_q_over_w = if w.norm() > 1e-14 {
        q / (w * 2.0)
    } else {
        // Degenerate: w ≈ 0 means the quartic has a special structure.
        // Fall back: use y0² - r for the constant term.
        (y0 * y0 - r).sqrt()
    };

    let c1 = y0 + half_q_over_w;
    let c2 = y0 - half_q_over_w;

    // Solve t² - wt + c2 = 0 and t² + wt + c1 = 0
    let disc1 = w * w - c1 * 4.0;
    let disc2 = w * w - c2 * 4.0;
    let sd1 = disc1.sqrt();
    let sd2 = disc2.sqrt();

    [
        (-w + sd1) / 2.0 + shift,
        (-w - sd1) / 2.0 + shift,
        (w + sd2) / 2.0 + shift,
        (w - sd2) / 2.0 + shift,
    ]
}

/// Eigenvalues of a 4×4 complex matrix via the characteristic polynomial
/// solved analytically with Ferrari's quartic formula.
fn eigenvalues_4x4(mat: &Unitary4x4) -> [Complex64; 4] {
    // Characteristic polynomial via Newton's identities:
    // det(A - λI) = λ⁴ - e₁λ³ + e₂λ² - e₃λ + e₄ = 0
    let a2 = mat.mul(mat);
    let a3 = a2.mul(mat);
    let a4 = a3.mul(mat);

    let p1 = mat.trace();
    let p2 = a2.trace();
    let p3 = a3.trace();
    let p4 = a4.trace();

    let e1 = p1;
    let e2 = (e1 * p1 - p2) / 2.0;
    let e3 = (e2 * p1 - e1 * p2 + p3) / 3.0;
    let e4 = (e3 * p1 - e2 * p2 + e1 * p3 - p4) / 4.0;

    // Rewrite as: λ⁴ + (-e₁)λ³ + (e₂)λ² + (-e₃)λ + e₄ = 0
    solve_quartic(-e1, e2, -e3, e4)
}

/// Determine minimum CNOT count from Makhlin invariants G₁ and G₂.
///
/// Classification rules:
/// - 0 CNOTs: G₁ = 1, G₂ = 3 (product state — should be caught earlier)
/// - 1 CNOT: G₁ real, G₂ real, G₂ = 2·G₁ + 1 (ty = tz = 0)
/// - 2 CNOTs: G₁ real and non-negative (tz = 0)
/// - 3 CNOTs: otherwise (G₁ complex or G₁ real and negative)
fn cnot_count_from_invariants(g1: Complex64, g2: Complex64) -> u8 {
    let tol = 1e-4;

    // 0 CNOTs: G₁ ≈ 1, G₂ ≈ 3
    if (g1.re - 1.0).abs() < tol
        && g1.im.abs() < tol
        && (g2.re - 3.0).abs() < tol
        && g2.im.abs() < tol
    {
        return 0;
    }

    // Check if G₁ is real.
    let g1_real = g1.im.abs() < tol;
    let g2_real = g2.im.abs() < tol;

    // 3 CNOTs: G₁ has imaginary part, or G₁ is real and negative.
    if !g1_real || g1.re < -tol {
        return 3;
    }

    // G₁ is real and non-negative from here.
    // 1 CNOT: G₂ is also real and G₂ = 2·G₁ + 1.
    if g2_real && (g2.re - (2.0 * g1.re + 1.0)).abs() < tol {
        return 1;
    }

    // 2 CNOTs: G₁ real and non-negative, but not on the 1-CNOT line.
    2
}

/// Extract Weyl coordinates (tx, ty, tz) from eigenvalues of Q = M^T·M.
///
/// Uses the CNOT count (from Makhlin invariants) to guide extraction
/// when eigenvalues are degenerate.
fn extract_weyl_from_eigenvalues(eigs: &[Complex64; 4], num_cnots: u8) -> (f64, f64, f64) {
    // For degenerate cases, use the known Weyl chamber structure.
    match num_cnots {
        0 => return (0.0, 0.0, 0.0),
        3 => {
            // For 3-CNOT gates, eigenvalue extraction from phases is unreliable
            // when eigenvalues are degenerate (e.g., SWAP has Q = scalar·I).
            // Use the known structure: SWAP → (π/4, π/4, π/4).
            // For general 3-CNOT gates, attempt extraction but fall back.
            let phases = extract_sorted_phases(eigs);
            let tx = ((phases[0] - phases[3]) / 2.0).abs();
            let ty = ((phases[1] - phases[2]) / 2.0).abs();

            // If phases are all degenerate, the eigenvalues don't distinguish
            // the coordinates. Use the maximal Weyl point as default.
            if tx < 1e-6 && ty < 1e-6 {
                return (FRAC_PI_4, FRAC_PI_4, FRAC_PI_4);
            }

            let tz = (phases[0] + phases[3] - phases[1] - phases[2]) / 4.0;
            return normalize_to_weyl_chamber(tx, ty, tz);
        }
        _ => {}
    }

    // For 1 and 2 CNOT cases, eigenvalues should be non-degenerate enough.
    let phases = extract_sorted_phases(eigs);
    let tx = ((phases[0] - phases[3]) / 2.0).abs();
    let ty = ((phases[1] - phases[2]) / 2.0).abs();
    let tz = (phases[0] + phases[3] - phases[1] - phases[2]) / 4.0;

    normalize_to_weyl_chamber(tx, ty, tz)
}

/// Extract half-phases from eigenvalues and sort descending.
fn extract_sorted_phases(eigs: &[Complex64; 4]) -> [f64; 4] {
    let mut phases: [f64; 4] = [
        eigs[0].arg() / 2.0,
        eigs[1].arg() / 2.0,
        eigs[2].arg() / 2.0,
        eigs[3].arg() / 2.0,
    ];

    // Remove global phase offset: center phases around 0.
    let sum: f64 = phases.iter().sum();
    let offset = sum / 4.0;
    for p in &mut phases {
        *p -= offset;
    }

    // Sort descending.
    phases.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    phases
}

/// Normalize coordinates into the Weyl chamber: π/4 ≥ tx ≥ ty ≥ |tz| ≥ 0.
fn normalize_to_weyl_chamber(mut tx: f64, mut ty: f64, mut tz: f64) -> (f64, f64, f64) {
    tx = normalize_weyl(tx).abs();
    ty = normalize_weyl(ty).abs();
    tz = normalize_weyl(tz);
    let tz_abs = tz.abs();

    // Sort: tx ≥ ty ≥ tz_abs.
    if ty > tx {
        std::mem::swap(&mut tx, &mut ty);
    }
    if tz_abs > ty {
        ty = tz_abs;
    }
    if ty > tx {
        std::mem::swap(&mut tx, &mut ty);
    }

    tx = tx.min(FRAC_PI_4);
    ty = ty.min(tx);

    (tx, ty, tz)
}

/// Normalize a Weyl coordinate to [-π/2, π/2].
fn normalize_weyl(x: f64) -> f64 {
    let mut v = x.rem_euclid(PI);
    if v > FRAC_PI_2 {
        v -= PI;
    }
    v
}

/// Factor a 4x4 unitary that is approximately a Kronecker product A ⊗ B
/// into its 2x2 factors.
///
/// Uses the first non-zero 2x2 block to extract the factors.
fn factor_kron(m: &Unitary4x4) -> (Unitary2x2, Unitary2x2) {
    // m ≈ A ⊗ B means m[2i+k][2j+l] = A[i][j] * B[k][l].
    // Find the largest element of A to avoid division by near-zero.
    let mut best_i = 0;
    let mut best_j = 0;
    let mut best_norm = 0.0;

    for i in 0..2 {
        for j in 0..2 {
            // A[i][j] can be estimated from any element of the (i,j) 2x2 block.
            let block_norm = m.get(2 * i, 2 * j).norm()
                + m.get(2 * i, 2 * j + 1).norm()
                + m.get(2 * i + 1, 2 * j).norm()
                + m.get(2 * i + 1, 2 * j + 1).norm();
            if block_norm > best_norm {
                best_norm = block_norm;
                best_i = i;
                best_j = j;
            }
        }
    }

    // Extract B from the (best_i, best_j) block, normalized.
    let block = [
        m.get(2 * best_i, 2 * best_j),
        m.get(2 * best_i, 2 * best_j + 1),
        m.get(2 * best_i + 1, 2 * best_j),
        m.get(2 * best_i + 1, 2 * best_j + 1),
    ];

    // Normalize B to unit determinant.
    let det_b = block[0] * block[3] - block[1] * block[2];
    let scale = if det_b.norm() > EPSILON {
        det_b.sqrt()
    } else {
        Complex64::new(1.0, 0.0)
    };

    let b = Unitary2x2::new(
        block[0] / scale,
        block[1] / scale,
        block[2] / scale,
        block[3] / scale,
    );

    // Extract A: A[i][j] = m[2i+k][2j+l] / B[k][l] for any (k,l) with B[k][l] ≠ 0.
    // Find best (k,l) in B.
    let mut best_k = 0;
    let mut best_l = 0;
    let mut best_b_norm = 0.0;
    for k in 0..2 {
        for l in 0..2 {
            let n = b.data[k * 2 + l].norm();
            if n > best_b_norm {
                best_b_norm = n;
                best_k = k;
                best_l = l;
            }
        }
    }

    let b_kl = b.data[best_k * 2 + best_l];
    let a = Unitary2x2::new(
        m.get(best_k, best_l) / b_kl,
        m.get(best_k, 2 + best_l) / b_kl,
        m.get(2 + best_k, best_l) / b_kl,
        m.get(2 + best_k, 2 + best_l) / b_kl,
    );

    // Normalize A to unit determinant.
    let det_a = a.data[0] * a.data[3] - a.data[1] * a.data[2];
    let scale_a = if det_a.norm() > EPSILON {
        det_a.sqrt()
    } else {
        Complex64::new(1.0, 0.0)
    };

    let a_norm = Unitary2x2::new(
        a.data[0] / scale_a,
        a.data[1] / scale_a,
        a.data[2] / scale_a,
        a.data[3] / scale_a,
    );

    (a_norm, b)
}

impl WeylDecomposition {
    /// Synthesize a circuit implementing this decomposition using CNOT gates.
    ///
    /// Returns a list of `(gate, qubit)` pairs where qubit is 0 or 1,
    /// and two-qubit gates are represented as `(CX, 0)` meaning CX(q0, q1).
    ///
    /// Uses the Vatan-Williams decomposition to achieve the minimum CNOT count.
    pub fn to_circuit(&self) -> Vec<TwoQubitGateOp> {
        let mut ops = Vec::new();

        match self.num_cnots {
            0 => {
                // Product state: just single-qubit gates.
                // U = (A0·B0) ⊗ (A1·B1) · phase
                let q0_gate = self.a0.mul(&self.b0);
                let q1_gate = self.a1.mul(&self.b1);
                push_1q_decomposed(&mut ops, &q0_gate, 0);
                push_1q_decomposed(&mut ops, &q1_gate, 1);
            }
            1 => {
                // One CNOT: U = (A0 ⊗ A1) · CX · (B0 ⊗ B1)
                // The interaction Ud(tx,0,0) decomposes as:
                //   Rz(q0, -2tx) after CX, with phase adjustments in A/B.
                push_1q_decomposed(&mut ops, &self.b0, 0);
                push_1q_decomposed(&mut ops, &self.b1, 1);

                // Absorb the interaction into the CX + Rz.
                ops.push(TwoQubitGateOp::Cx);
                if self.tx.abs() > EPSILON {
                    ops.push(TwoQubitGateOp::Rz(0, -2.0 * self.tx));
                }

                push_1q_decomposed(&mut ops, &self.a0, 0);
                push_1q_decomposed(&mut ops, &self.a1, 1);
            }
            2 => {
                // Two CNOTs.
                push_1q_decomposed(&mut ops, &self.b0, 0);
                push_1q_decomposed(&mut ops, &self.b1, 1);

                ops.push(TwoQubitGateOp::Cx);
                ops.push(TwoQubitGateOp::Ry(0, -2.0 * self.tx));
                ops.push(TwoQubitGateOp::Rz(1, 2.0 * self.ty));
                ops.push(TwoQubitGateOp::CxReverse);
                ops.push(TwoQubitGateOp::Ry(0, 2.0 * self.tx));

                push_1q_decomposed(&mut ops, &self.a0, 0);
                push_1q_decomposed(&mut ops, &self.a1, 1);
            }
            _ => {
                // Three CNOTs: general case.
                push_1q_decomposed(&mut ops, &self.b0, 0);
                push_1q_decomposed(&mut ops, &self.b1, 1);

                ops.push(TwoQubitGateOp::Cx);
                ops.push(TwoQubitGateOp::Rz(0, 2.0 * self.tz));
                ops.push(TwoQubitGateOp::Ry(1, 2.0 * self.ty));
                ops.push(TwoQubitGateOp::CxReverse);
                ops.push(TwoQubitGateOp::Ry(1, -2.0 * self.ty));
                ops.push(TwoQubitGateOp::Cx);
                ops.push(TwoQubitGateOp::Rz(0, -2.0 * self.tx));

                push_1q_decomposed(&mut ops, &self.a0, 0);
                push_1q_decomposed(&mut ops, &self.a1, 1);
            }
        }

        ops
    }

    /// Count the number of entangling (CX) gates in the synthesized circuit.
    pub fn cx_count(&self) -> u8 {
        self.num_cnots
    }
}

/// Operations in a two-qubit decomposed circuit.
#[derive(Debug, Clone)]
pub enum TwoQubitGateOp {
    /// RZ(angle) on qubit index (0 or 1).
    Rz(u8, f64),
    /// RY(angle) on qubit index (0 or 1).
    Ry(u8, f64),
    /// CX with qubit 0 as control, qubit 1 as target.
    Cx,
    /// CX with qubit 1 as control, qubit 0 as target.
    CxReverse,
}

/// Decompose a single-qubit unitary via ZYZ and push the ops.
fn push_1q_decomposed(ops: &mut Vec<TwoQubitGateOp>, u: &Unitary2x2, qubit: u8) {
    let (alpha, beta, gamma, _phase) = u.zyz_decomposition();
    let alpha = Unitary2x2::normalize_angle(alpha);
    let beta = Unitary2x2::normalize_angle(beta);
    let gamma = Unitary2x2::normalize_angle(gamma);

    if gamma.abs() > EPSILON {
        ops.push(TwoQubitGateOp::Rz(qubit, gamma));
    }
    if beta.abs() > EPSILON {
        ops.push(TwoQubitGateOp::Ry(qubit, beta));
    }
    if alpha.abs() > EPSILON {
        ops.push(TwoQubitGateOp::Rz(qubit, alpha));
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

    // ---- 4x4 / KAK decomposition tests ----

    /// Helper: build a CX unitary matrix.
    fn cx_unitary() -> Unitary4x4 {
        let o = Complex64::new(1.0, 0.0);
        let z = Complex64::new(0.0, 0.0);
        Unitary4x4 {
            data: [o, z, z, z, z, o, z, z, z, z, z, o, z, z, o, z],
        }
    }

    /// Helper: build a SWAP unitary matrix.
    fn swap_unitary() -> Unitary4x4 {
        let o = Complex64::new(1.0, 0.0);
        let z = Complex64::new(0.0, 0.0);
        Unitary4x4 {
            data: [o, z, z, z, z, z, o, z, z, o, z, z, z, z, z, o],
        }
    }

    /// Helper: build a CZ unitary matrix diag(1,1,1,-1).
    fn cz_unitary() -> Unitary4x4 {
        let o = Complex64::new(1.0, 0.0);
        let z = Complex64::new(0.0, 0.0);
        let m = Complex64::new(-1.0, 0.0);
        Unitary4x4 {
            data: [o, z, z, z, z, o, z, z, z, z, o, z, z, z, z, m],
        }
    }

    /// Helper: verify KAK reconstruction matches original unitary.
    fn verify_kak_reconstruction(u: &Unitary4x4, label: &str) {
        let kak = u.kak_decompose();
        let ud = canonical_unitary(kak.tx, kak.ty, kak.tz);
        let left = Unitary4x4::kron(&kak.a0, &kak.a1);
        let right = Unitary4x4::kron(&kak.b0, &kak.b1);
        let reconstructed = left.mul(&ud).mul(&right);
        assert!(
            u.equiv(&reconstructed),
            "{label} KAK reconstruction failed (num_cnots={})",
            kak.num_cnots
        );
    }

    #[test]
    fn test_quartic_solver_known_roots() {
        // (x-1)(x-2)(x-3)(x-4) = x⁴ - 10x³ + 35x² - 50x + 24
        let roots = solve_quartic(
            Complex64::new(-10.0, 0.0),
            Complex64::new(35.0, 0.0),
            Complex64::new(-50.0, 0.0),
            Complex64::new(24.0, 0.0),
        );
        let mut real_roots: Vec<f64> = roots.iter().map(|r| r.re).collect();
        real_roots.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(approx_eq(real_roots[0], 1.0), "root 0: {}", real_roots[0]);
        assert!(approx_eq(real_roots[1], 2.0), "root 1: {}", real_roots[1]);
        assert!(approx_eq(real_roots[2], 3.0), "root 2: {}", real_roots[2]);
        assert!(approx_eq(real_roots[3], 4.0), "root 3: {}", real_roots[3]);
    }

    #[test]
    fn test_quartic_solver_degenerate() {
        // (x-1)⁴ = x⁴ - 4x³ + 6x² - 4x + 1
        let roots = solve_quartic(
            Complex64::new(-4.0, 0.0),
            Complex64::new(6.0, 0.0),
            Complex64::new(-4.0, 0.0),
            Complex64::new(1.0, 0.0),
        );
        for (i, r) in roots.iter().enumerate() {
            assert!(
                (r.re - 1.0).abs() < 1e-3 && r.im.abs() < 1e-3,
                "degenerate root {i}: {r:?}"
            );
        }
    }

    #[test]
    fn test_eigenvalues_identity() {
        let id = Unitary4x4::identity();
        let eigs = eigenvalues_4x4(&id);
        for (i, e) in eigs.iter().enumerate() {
            assert!(
                (e.re - 1.0).abs() < 1e-6 && e.im.abs() < 1e-6,
                "identity eigenvalue {i}: {e:?}"
            );
        }
    }

    #[test]
    fn test_4x4_identity() {
        let id = Unitary4x4::identity();
        let kak = id.kak_decompose();
        assert_eq!(kak.num_cnots, 0, "identity should need 0 CNOTs");
    }

    #[test]
    fn test_kron_product() {
        let hx = Unitary4x4::kron(&Unitary2x2::h(), &Unitary2x2::x());
        let kak = hx.kak_decompose();
        assert_eq!(kak.num_cnots, 0, "H⊗X should need 0 CNOTs");
        verify_kak_reconstruction(&hx, "H⊗X");
    }

    #[test]
    fn test_kak_cx() {
        let cx = cx_unitary();
        let kak = cx.kak_decompose();
        assert!(
            kak.num_cnots <= 1,
            "CX should need ≤1 CNOT, got {}",
            kak.num_cnots
        );
    }

    #[test]
    fn test_kak_swap() {
        let swap = swap_unitary();
        let kak = swap.kak_decompose();
        assert_eq!(
            kak.num_cnots, 3,
            "SWAP should need 3 CNOTs, got {}",
            kak.num_cnots
        );
    }

    #[test]
    fn test_kak_cz() {
        let cz = cz_unitary();
        let kak = cz.kak_decompose();
        assert!(
            kak.num_cnots <= 1,
            "CZ should need ≤1 CNOT, got {}",
            kak.num_cnots
        );
    }

    #[test]
    fn test_kak_reconstruction_product() {
        // Verify KAK reconstruction works for product-state unitaries.
        let hx = Unitary4x4::kron(&Unitary2x2::h(), &Unitary2x2::x());
        verify_kak_reconstruction(&hx, "H⊗X");
        verify_kak_reconstruction(&Unitary4x4::identity(), "Identity");

        // RZ ⊗ RY
        let rz_ry = Unitary4x4::kron(&Unitary2x2::rz(1.2), &Unitary2x2::ry(0.7));
        verify_kak_reconstruction(&rz_ry, "RZ⊗RY");
    }

    #[test]
    fn test_kak_iswap() {
        // iSWAP gate: diag(1,0,0,0), off-diag i in middle block.
        let z = Complex64::new(0.0, 0.0);
        let o = Complex64::new(1.0, 0.0);
        let i = Complex64::new(0.0, 1.0);
        let iswap = Unitary4x4 {
            data: [o, z, z, z, z, z, i, z, z, i, z, z, z, z, z, o],
        };
        let kak = iswap.kak_decompose();
        assert_eq!(
            kak.num_cnots, 2,
            "iSWAP should need 2 CNOTs, got {}",
            kak.num_cnots
        );
    }

    #[test]
    fn test_canonical_unitary_identity() {
        let ud = canonical_unitary(0.0, 0.0, 0.0);
        let id = Unitary4x4::identity();
        assert!(ud.equiv(&id), "Ud(0,0,0) should be identity");
    }

    #[test]
    fn test_canonical_unitary_cx() {
        // CX ∼ exp(i·π/4·XX) up to local unitaries.
        // Ud(π/4, 0, 0) should have the right entanglement structure.
        let ud = canonical_unitary(FRAC_PI_4, 0.0, 0.0);
        // This should be equivalent to CX up to local unitaries,
        // meaning it's entangling but not identity.
        assert!(
            !ud.is_product_state(),
            "Ud(π/4,0,0) should not be a product state"
        );
    }
}
