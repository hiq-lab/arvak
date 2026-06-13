//! Statevector simulation engine.

use num_complex::Complex64;
use std::f64::consts::PI;

use arvak_ir::{GateKind, Instruction, InstructionKind, StandardGate};

/// A statevector representing a quantum state.
pub struct Statevector {
    /// The state amplitudes (2^n complex numbers).
    amplitudes: Vec<Complex64>,
    /// Number of qubits.
    num_qubits: usize,
}

impl Statevector {
    /// Create a new statevector initialized to |0...0⟩.
    pub fn new(num_qubits: usize) -> Self {
        assert!(
            num_qubits <= 26,
            "Statevector simulation limited to 26 qubits ({})",
            num_qubits
        );
        let size = 1 << num_qubits;
        let mut amplitudes = vec![Complex64::new(0.0, 0.0); size];
        amplitudes[0] = Complex64::new(1.0, 0.0);
        Self {
            amplitudes,
            num_qubits,
        }
    }

    /// Get the number of qubits.
    #[allow(dead_code)]
    pub fn num_qubits(&self) -> usize {
        self.num_qubits
    }

    /// Apply an instruction to the statevector.
    ///
    /// `rng` is used for the stochastic collapse of `Reset` instructions.
    ///
    /// Returns an error if a parametric gate has unresolved symbolic parameters
    /// or if a custom gate is encountered.
    pub fn apply<R: rand::Rng>(
        &mut self,
        instruction: &Instruction,
        rng: &mut R,
    ) -> Result<(), String> {
        match &instruction.kind {
            InstructionKind::Gate(gate) => {
                let qubits: Vec<_> = instruction.qubits.iter().map(|q| q.0 as usize).collect();
                self.apply_gate(&gate.kind, &qubits)?;
            }
            InstructionKind::Reset => {
                let qubit = instruction.qubits[0].0 as usize;
                let r: f64 = rng.r#gen();
                self.reset(qubit, r);
            }
            InstructionKind::Measure
            | InstructionKind::Barrier
            | InstructionKind::Delay { .. }
            | InstructionKind::Shuttle { .. }
            | InstructionKind::NoiseChannel { .. } => {
                // These don't modify the statevector in simulation
            }
        }
        Ok(())
    }

    /// Apply a gate to specific qubits.
    fn apply_gate(&mut self, gate: &GateKind, qubits: &[usize]) -> Result<(), String> {
        match gate {
            GateKind::Standard(std_gate) => {
                self.apply_standard_gate(std_gate, qubits)?;
            }
            GateKind::Custom(custom_gate) => {
                return Err(format!(
                    "Custom gate '{}' cannot be simulated: matrix multiplication not yet implemented",
                    custom_gate.name
                ));
            }
        }
        Ok(())
    }

    /// Apply a standard gate.
    fn apply_standard_gate(&mut self, gate: &StandardGate, qubits: &[usize]) -> Result<(), String> {
        match gate {
            // Single-qubit gates
            StandardGate::I => {}
            StandardGate::X => self.apply_x(qubits[0]),
            StandardGate::Y => self.apply_y(qubits[0]),
            StandardGate::Z => self.apply_z(qubits[0]),
            StandardGate::H => self.apply_h(qubits[0]),
            StandardGate::S => self.apply_phase(qubits[0], PI / 2.0),
            StandardGate::Sdg => self.apply_phase(qubits[0], -PI / 2.0),
            StandardGate::T => self.apply_phase(qubits[0], PI / 4.0),
            StandardGate::Tdg => self.apply_phase(qubits[0], -PI / 4.0),
            StandardGate::SX => self.apply_sx(qubits[0]),
            StandardGate::SXdg => self.apply_sxdg(qubits[0]),
            StandardGate::Rx(theta) => {
                let t = theta
                    .as_f64()
                    .ok_or_else(|| "Rx gate has unresolved symbolic parameter".to_string())?;
                self.apply_rx(qubits[0], t);
            }
            StandardGate::Ry(theta) => {
                let t = theta
                    .as_f64()
                    .ok_or_else(|| "Ry gate has unresolved symbolic parameter".to_string())?;
                self.apply_ry(qubits[0], t);
            }
            StandardGate::Rz(theta) => {
                let t = theta
                    .as_f64()
                    .ok_or_else(|| "Rz gate has unresolved symbolic parameter".to_string())?;
                self.apply_rz(qubits[0], t);
            }
            StandardGate::P(theta) => {
                let t = theta
                    .as_f64()
                    .ok_or_else(|| "P gate has unresolved symbolic parameter".to_string())?;
                self.apply_phase(qubits[0], t);
            }
            StandardGate::U(theta, phi, lambda) => {
                let t = theta.as_f64().ok_or_else(|| {
                    "U gate has unresolved symbolic parameter (theta)".to_string()
                })?;
                let p = phi
                    .as_f64()
                    .ok_or_else(|| "U gate has unresolved symbolic parameter (phi)".to_string())?;
                let l = lambda.as_f64().ok_or_else(|| {
                    "U gate has unresolved symbolic parameter (lambda)".to_string()
                })?;
                self.apply_u(qubits[0], t, p, l);
            }
            StandardGate::PRX(theta, phi) => {
                let t = theta.as_f64().ok_or_else(|| {
                    "PRX gate has unresolved symbolic parameter (theta)".to_string()
                })?;
                let p = phi.as_f64().ok_or_else(|| {
                    "PRX gate has unresolved symbolic parameter (phi)".to_string()
                })?;
                self.apply_prx(qubits[0], t, p);
            }

            // Two-qubit gates
            StandardGate::CX => self.apply_cx(qubits[0], qubits[1]),
            StandardGate::CY => self.apply_cy(qubits[0], qubits[1]),
            StandardGate::CZ => self.apply_cz(qubits[0], qubits[1]),
            StandardGate::CH => self.apply_ch(qubits[0], qubits[1]),
            StandardGate::Swap => self.apply_swap(qubits[0], qubits[1]),
            StandardGate::ISwap => self.apply_iswap(qubits[0], qubits[1]),
            StandardGate::CRz(theta) => {
                let t = theta
                    .as_f64()
                    .ok_or_else(|| "CRz gate has unresolved symbolic parameter".to_string())?;
                self.apply_crz(qubits[0], qubits[1], t);
            }
            StandardGate::CP(theta) => {
                let t = theta
                    .as_f64()
                    .ok_or_else(|| "CP gate has unresolved symbolic parameter".to_string())?;
                self.apply_cp(qubits[0], qubits[1], t);
            }

            // Three-qubit gates
            StandardGate::CCX => self.apply_ccx(qubits[0], qubits[1], qubits[2]),
            StandardGate::CSwap => self.apply_cswap(qubits[0], qubits[1], qubits[2]),

            _ => {
                return Err(format!("Unhandled gate type in simulation: {:?}", gate));
            }
        }
        Ok(())
    }

    // =========================================================================
    // Single-qubit gate implementations
    // =========================================================================

    fn apply_x(&mut self, qubit: usize) {
        let mask = 1 << qubit;
        for i in 0..(1 << self.num_qubits) {
            if i & mask == 0 {
                let j = i | mask;
                self.amplitudes.swap(i, j);
            }
        }
    }

    fn apply_y(&mut self, qubit: usize) {
        let mask = 1 << qubit;
        let i_val = Complex64::new(0.0, 1.0);
        for i in 0..(1 << self.num_qubits) {
            if i & mask == 0 {
                let j = i | mask;
                let tmp = self.amplitudes[i];
                self.amplitudes[i] = -i_val * self.amplitudes[j];
                self.amplitudes[j] = i_val * tmp;
            }
        }
    }

    fn apply_z(&mut self, qubit: usize) {
        let mask = 1 << qubit;
        for i in 0..(1 << self.num_qubits) {
            if i & mask != 0 {
                self.amplitudes[i] = -self.amplitudes[i];
            }
        }
    }

    fn apply_h(&mut self, qubit: usize) {
        let mask = 1 << qubit;
        let sqrt2_inv = 1.0 / 2.0_f64.sqrt();
        for i in 0..(1 << self.num_qubits) {
            if i & mask == 0 {
                let j = i | mask;
                let a = self.amplitudes[i];
                let b = self.amplitudes[j];
                self.amplitudes[i] = sqrt2_inv * (a + b);
                self.amplitudes[j] = sqrt2_inv * (a - b);
            }
        }
    }

    fn apply_phase(&mut self, qubit: usize, theta: f64) {
        let mask = 1 << qubit;
        let phase = Complex64::from_polar(1.0, theta);
        for i in 0..(1 << self.num_qubits) {
            if i & mask != 0 {
                self.amplitudes[i] *= phase;
            }
        }
    }

    fn apply_rx(&mut self, qubit: usize, theta: f64) {
        let mask = 1 << qubit;
        let c = (theta / 2.0).cos();
        let s = (theta / 2.0).sin();
        let neg_i_s = Complex64::new(0.0, -s);
        for i in 0..(1 << self.num_qubits) {
            if i & mask == 0 {
                let j = i | mask;
                let a = self.amplitudes[i];
                let b = self.amplitudes[j];
                self.amplitudes[i] = c * a + neg_i_s * b;
                self.amplitudes[j] = neg_i_s * a + c * b;
            }
        }
    }

    fn apply_ry(&mut self, qubit: usize, theta: f64) {
        let mask = 1 << qubit;
        let c = (theta / 2.0).cos();
        let s = (theta / 2.0).sin();
        for i in 0..(1 << self.num_qubits) {
            if i & mask == 0 {
                let j = i | mask;
                let a = self.amplitudes[i];
                let b = self.amplitudes[j];
                self.amplitudes[i] = c * a - s * b;
                self.amplitudes[j] = s * a + c * b;
            }
        }
    }

    fn apply_rz(&mut self, qubit: usize, theta: f64) {
        let mask = 1 << qubit;
        let phase_0 = Complex64::from_polar(1.0, -theta / 2.0);
        let phase_1 = Complex64::from_polar(1.0, theta / 2.0);
        for i in 0..(1 << self.num_qubits) {
            if i & mask == 0 {
                self.amplitudes[i] *= phase_0;
            } else {
                self.amplitudes[i] *= phase_1;
            }
        }
    }

    fn apply_u(&mut self, qubit: usize, theta: f64, phi: f64, lambda: f64) {
        let mask = 1 << qubit;
        let c = (theta / 2.0).cos();
        let s = (theta / 2.0).sin();
        let e_il = Complex64::from_polar(1.0, lambda);
        let e_ip = Complex64::from_polar(1.0, phi);
        let e_ipl = Complex64::from_polar(1.0, phi + lambda);

        for i in 0..(1 << self.num_qubits) {
            if i & mask == 0 {
                let j = i | mask;
                let a = self.amplitudes[i];
                let b = self.amplitudes[j];
                self.amplitudes[i] = c * a - e_il * s * b;
                self.amplitudes[j] = e_ip * s * a + e_ipl * c * b;
            }
        }
    }

    fn apply_prx(&mut self, qubit: usize, theta: f64, phi: f64) {
        // PRX(θ, φ) = RZ(φ) · RX(θ) · RZ(-φ)
        self.apply_rz(qubit, -phi);
        self.apply_rx(qubit, theta);
        self.apply_rz(qubit, phi);
    }

    fn apply_sx(&mut self, qubit: usize) {
        self.apply_rx(qubit, PI / 2.0);
    }

    fn apply_sxdg(&mut self, qubit: usize) {
        self.apply_rx(qubit, -PI / 2.0);
    }

    // =========================================================================
    // Two-qubit gate implementations
    // =========================================================================

    fn apply_cx(&mut self, control: usize, target: usize) {
        let ctrl_mask = 1 << control;
        let tgt_mask = 1 << target;
        for i in 0..(1 << self.num_qubits) {
            if (i & ctrl_mask != 0) && (i & tgt_mask == 0) {
                let j = i | tgt_mask;
                self.amplitudes.swap(i, j);
            }
        }
    }

    fn apply_cy(&mut self, control: usize, target: usize) {
        let ctrl_mask = 1 << control;
        let tgt_mask = 1 << target;
        let i_val = Complex64::new(0.0, 1.0);
        for i in 0..(1 << self.num_qubits) {
            if (i & ctrl_mask != 0) && (i & tgt_mask == 0) {
                let j = i | tgt_mask;
                let tmp = self.amplitudes[i];
                self.amplitudes[i] = -i_val * self.amplitudes[j];
                self.amplitudes[j] = i_val * tmp;
            }
        }
    }

    fn apply_cz(&mut self, control: usize, target: usize) {
        let ctrl_mask = 1 << control;
        let tgt_mask = 1 << target;
        for i in 0..(1 << self.num_qubits) {
            if (i & ctrl_mask != 0) && (i & tgt_mask != 0) {
                self.amplitudes[i] = -self.amplitudes[i];
            }
        }
    }

    fn apply_ch(&mut self, control: usize, target: usize) {
        let ctrl_mask = 1 << control;
        let tgt_mask = 1 << target;
        let sqrt2_inv = 1.0 / 2.0_f64.sqrt();
        for i in 0..(1 << self.num_qubits) {
            if (i & ctrl_mask != 0) && (i & tgt_mask == 0) {
                let j = i | tgt_mask;
                let a = self.amplitudes[i];
                let b = self.amplitudes[j];
                self.amplitudes[i] = sqrt2_inv * (a + b);
                self.amplitudes[j] = sqrt2_inv * (a - b);
            }
        }
    }

    fn apply_swap(&mut self, q1: usize, q2: usize) {
        let mask1 = 1 << q1;
        let mask2 = 1 << q2;
        for i in 0..(1 << self.num_qubits) {
            let b1 = (i & mask1) != 0;
            let b2 = (i & mask2) != 0;
            if b1 && !b2 {
                let j = (i & !mask1) | mask2;
                self.amplitudes.swap(i, j);
            }
        }
    }

    fn apply_iswap(&mut self, q1: usize, q2: usize) {
        let mask1 = 1 << q1;
        let mask2 = 1 << q2;
        let i_val = Complex64::new(0.0, 1.0);
        for i in 0..(1 << self.num_qubits) {
            let b1 = (i & mask1) != 0;
            let b2 = (i & mask2) != 0;
            if b1 && !b2 {
                let j = (i & !mask1) | mask2;
                let tmp = self.amplitudes[i];
                self.amplitudes[i] = i_val * self.amplitudes[j];
                self.amplitudes[j] = i_val * tmp;
            }
        }
    }

    fn apply_crz(&mut self, control: usize, target: usize, theta: f64) {
        let ctrl_mask = 1 << control;
        let tgt_mask = 1 << target;
        let phase_0 = Complex64::from_polar(1.0, -theta / 2.0);
        let phase_1 = Complex64::from_polar(1.0, theta / 2.0);
        for i in 0..(1 << self.num_qubits) {
            if i & ctrl_mask != 0 {
                if i & tgt_mask == 0 {
                    self.amplitudes[i] *= phase_0;
                } else {
                    self.amplitudes[i] *= phase_1;
                }
            }
        }
    }

    fn apply_cp(&mut self, control: usize, target: usize, theta: f64) {
        let ctrl_mask = 1 << control;
        let tgt_mask = 1 << target;
        let phase = Complex64::from_polar(1.0, theta);
        for i in 0..(1 << self.num_qubits) {
            if (i & ctrl_mask != 0) && (i & tgt_mask != 0) {
                self.amplitudes[i] *= phase;
            }
        }
    }

    // =========================================================================
    // Three-qubit gate implementations
    // =========================================================================

    fn apply_ccx(&mut self, c1: usize, c2: usize, target: usize) {
        let c1_mask = 1 << c1;
        let c2_mask = 1 << c2;
        let tgt_mask = 1 << target;
        for i in 0..(1 << self.num_qubits) {
            if (i & c1_mask != 0) && (i & c2_mask != 0) && (i & tgt_mask == 0) {
                let j = i | tgt_mask;
                self.amplitudes.swap(i, j);
            }
        }
    }

    fn apply_cswap(&mut self, control: usize, t1: usize, t2: usize) {
        let ctrl_mask = 1 << control;
        let t1_mask = 1 << t1;
        let t2_mask = 1 << t2;
        for i in 0..(1 << self.num_qubits) {
            if i & ctrl_mask != 0 {
                let b1 = (i & t1_mask) != 0;
                let b2 = (i & t2_mask) != 0;
                if b1 && !b2 {
                    let j = (i & !t1_mask) | t2_mask;
                    self.amplitudes.swap(i, j);
                }
            }
        }
    }

    /// Reset a qubit to |0⟩ via stochastic projective measurement.
    ///
    /// `r` is a uniform random sample in [0, 1) selecting the measurement
    /// outcome. The state is projected onto the measured branch,
    /// renormalized, and (if the outcome was |1⟩) flipped back to |0⟩.
    ///
    /// Note: the previous implementation coherently *added* the |1⟩
    /// amplitudes into the |0⟩ branch, which is unphysical — resetting a
    /// qubit in |−⟩ annihilated the entire statevector.
    fn reset(&mut self, qubit: usize, r: f64) {
        let mask = 1 << qubit;

        // Probability of measuring |1⟩ on this qubit.
        let mut p1 = 0.0;
        for i in 0..(1 << self.num_qubits) {
            if i & mask != 0 {
                p1 += self.amplitudes[i].norm_sqr();
            }
        }

        let outcome_one = r < p1;
        let p_branch = if outcome_one { p1 } else { 1.0 - p1 };
        // Guard: with a normalized state p_branch > 0 for the sampled
        // outcome, but protect against rounding pathologies.
        let scale = if p_branch > 1e-300 {
            1.0 / p_branch.sqrt()
        } else {
            1.0
        };

        for i in 0..(1 << self.num_qubits) {
            if i & mask != 0 {
                let j = i & !mask;
                if outcome_one {
                    // Project onto |1⟩, renormalize, then flip to |0⟩.
                    self.amplitudes[j] = self.amplitudes[i] * scale;
                }
                self.amplitudes[i] = Complex64::new(0.0, 0.0);
            } else if !outcome_one {
                // Project onto |0⟩ and renormalize.
                self.amplitudes[i] *= scale;
            }
        }
    }

    /// Sample a measurement outcome.
    pub fn sample<R: rand::Rng>(&self, rng: &mut R) -> usize {
        let r: f64 = rng.r#gen();

        let mut cumulative = 0.0;
        for (i, amp) in self.amplitudes.iter().enumerate() {
            cumulative += amp.norm_sqr();
            if r < cumulative {
                return i;
            }
        }

        // Fallback (shouldn't happen with normalized states)
        self.amplitudes.len() - 1
    }

    /// Sample `shots` measurement outcomes from the final state.
    ///
    /// Builds the cumulative distribution once, then draws each shot with a
    /// binary search — O(2^n + shots·n) instead of O(shots·2^n).
    pub fn sample_counts<R: rand::Rng>(
        &self,
        shots: u32,
        rng: &mut R,
    ) -> rustc_hash::FxHashMap<usize, u32> {
        let mut cumulative = Vec::with_capacity(self.amplitudes.len());
        let mut acc = 0.0;
        for amp in &self.amplitudes {
            acc += amp.norm_sqr();
            cumulative.push(acc);
        }

        let mut counts: rustc_hash::FxHashMap<usize, u32> = rustc_hash::FxHashMap::default();
        for _ in 0..shots {
            let r: f64 = rng.r#gen::<f64>() * acc.min(1.0);
            let idx = cumulative.partition_point(|&c| c <= r);
            let idx = idx.min(self.amplitudes.len() - 1);
            *counts.entry(idx).or_insert(0) += 1;
        }
        counts
    }

    /// Convert measurement outcome to bitstring.
    ///
    /// HAL Contract bit order: the rightmost character is qubit 0 (OpenQASM 3
    /// / Qiskit convention), so the string is the binary representation of the
    /// basis-state index (statevector index bit k = qubit k). The previous
    /// implementation reversed the string, violating the contract.
    pub fn outcome_to_bitstring(&self, outcome: usize) -> String {
        format!("{:0width$b}", outcome, width = self.num_qubits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: Complex64, b: Complex64) -> bool {
        (a - b).norm() < 1e-10
    }

    #[test]
    fn test_initial_state() {
        let sv = Statevector::new(2);
        assert!(approx_eq(sv.amplitudes[0], Complex64::new(1.0, 0.0)));
        assert!(approx_eq(sv.amplitudes[1], Complex64::new(0.0, 0.0)));
        assert!(approx_eq(sv.amplitudes[2], Complex64::new(0.0, 0.0)));
        assert!(approx_eq(sv.amplitudes[3], Complex64::new(0.0, 0.0)));
    }

    #[test]
    fn test_hadamard() {
        let mut sv = Statevector::new(1);
        sv.apply_h(0);

        let sqrt2_inv = 1.0 / 2.0_f64.sqrt();
        assert!(approx_eq(sv.amplitudes[0], Complex64::new(sqrt2_inv, 0.0)));
        assert!(approx_eq(sv.amplitudes[1], Complex64::new(sqrt2_inv, 0.0)));
    }

    #[test]
    fn test_bell_state() {
        let mut sv = Statevector::new(2);
        sv.apply_h(0);
        sv.apply_cx(0, 1);

        let sqrt2_inv = 1.0 / 2.0_f64.sqrt();
        assert!(approx_eq(sv.amplitudes[0], Complex64::new(sqrt2_inv, 0.0)));
        assert!(approx_eq(sv.amplitudes[1], Complex64::new(0.0, 0.0)));
        assert!(approx_eq(sv.amplitudes[2], Complex64::new(0.0, 0.0)));
        assert!(approx_eq(sv.amplitudes[3], Complex64::new(sqrt2_inv, 0.0)));
    }

    #[test]
    fn test_x_gate() {
        let mut sv = Statevector::new(1);
        sv.apply_x(0);

        assert!(approx_eq(sv.amplitudes[0], Complex64::new(0.0, 0.0)));
        assert!(approx_eq(sv.amplitudes[1], Complex64::new(1.0, 0.0)));
    }

    #[test]
    fn test_sample_deterministic() {
        // |1⟩ state should always sample to 1
        let mut sv = Statevector::new(1);
        sv.apply_x(0);

        let mut rng = rand::thread_rng();
        for _ in 0..100 {
            assert_eq!(sv.sample(&mut rng), 1);
        }
    }

    #[test]
    fn test_reset_minus_state() {
        // Reset of |−⟩ = (|0⟩−|1⟩)/√2 must yield a normalized |0⟩ state
        // (up to global phase) for ANY measurement outcome — regression test:
        // the old implementation summed the amplitudes and annihilated the
        // state.
        for r in [0.0, 0.3, 0.7, 0.999] {
            let mut sv = Statevector::new(1);
            sv.apply_x(0);
            sv.apply_h(0); // |−⟩
            sv.reset(0, r);
            assert!(
                (sv.amplitudes[0].norm() - 1.0).abs() < 1e-10,
                "r={r}: |0⟩ amplitude should have norm 1, got {:?}",
                sv.amplitudes[0]
            );
            assert!(sv.amplitudes[1].norm() < 1e-10);
        }
    }

    #[test]
    fn test_reset_entangled_bell() {
        // Reset q0 of a Bell state (|00⟩+|11⟩)/√2. `r < p1` selects outcome
        // |1⟩: the state collapses to |11⟩ and q0 is flipped back → index 2
        // (q1=1, q0=0). Outcome |0⟩ collapses to |00⟩ → index 0.
        for (r, expect_idx) in [(0.1, 2usize), (0.9, 0usize)] {
            let mut sv = Statevector::new(2);
            sv.apply_h(0);
            sv.apply_cx(0, 1); // (|00⟩+|11⟩)/√2
            sv.reset(0, r);
            // Norm must be 1 and the q0 bit must be 0 in all support states.
            let total: f64 = sv
                .amplitudes
                .iter()
                .map(num_complex::Complex64::norm_sqr)
                .sum();
            assert!((total - 1.0).abs() < 1e-10, "norm {total}");
            assert!((sv.amplitudes[expect_idx].norm() - 1.0).abs() < 1e-10);
        }
    }
}
