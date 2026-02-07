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
    pub fn apply(&mut self, instruction: &Instruction) {
        match &instruction.kind {
            InstructionKind::Gate(gate) => {
                let qubits: Vec<_> = instruction.qubits.iter().map(|q| q.0 as usize).collect();
                self.apply_gate(&gate.kind, &qubits);
            }
            InstructionKind::Reset => {
                // Simplified reset: collapse to |0⟩ on the qubit
                let qubit = instruction.qubits[0].0 as usize;
                self.reset(qubit);
            }
            InstructionKind::Measure
            | InstructionKind::Barrier
            | InstructionKind::Delay { .. }
            | InstructionKind::Shuttle { .. } => {
                // These don't modify the statevector in simulation
            }
        }
    }

    /// Apply a gate to specific qubits.
    fn apply_gate(&mut self, gate: &GateKind, qubits: &[usize]) {
        match gate {
            GateKind::Standard(std_gate) => {
                self.apply_standard_gate(std_gate, qubits);
            }
            GateKind::Custom(_) => {
                // Custom gates would need matrix multiplication
                // For now, skip
            }
        }
    }

    /// Apply a standard gate.
    fn apply_standard_gate(&mut self, gate: &StandardGate, qubits: &[usize]) {
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
                if let Some(t) = theta.as_f64() {
                    self.apply_rx(qubits[0], t);
                }
            }
            StandardGate::Ry(theta) => {
                if let Some(t) = theta.as_f64() {
                    self.apply_ry(qubits[0], t);
                }
            }
            StandardGate::Rz(theta) => {
                if let Some(t) = theta.as_f64() {
                    self.apply_rz(qubits[0], t);
                }
            }
            StandardGate::P(theta) => {
                if let Some(t) = theta.as_f64() {
                    self.apply_phase(qubits[0], t);
                }
            }
            StandardGate::U(theta, phi, lambda) => {
                if let (Some(t), Some(p), Some(l)) = (theta.as_f64(), phi.as_f64(), lambda.as_f64())
                {
                    self.apply_u(qubits[0], t, p, l);
                }
            }
            StandardGate::PRX(theta, phi) => {
                if let (Some(t), Some(p)) = (theta.as_f64(), phi.as_f64()) {
                    self.apply_prx(qubits[0], t, p);
                }
            }

            // Two-qubit gates
            StandardGate::CX => self.apply_cx(qubits[0], qubits[1]),
            StandardGate::CY => self.apply_cy(qubits[0], qubits[1]),
            StandardGate::CZ => self.apply_cz(qubits[0], qubits[1]),
            StandardGate::CH => self.apply_ch(qubits[0], qubits[1]),
            StandardGate::Swap => self.apply_swap(qubits[0], qubits[1]),
            StandardGate::ISwap => self.apply_iswap(qubits[0], qubits[1]),
            StandardGate::CRz(theta) => {
                if let Some(t) = theta.as_f64() {
                    self.apply_crz(qubits[0], qubits[1], t);
                }
            }
            StandardGate::CP(theta) => {
                if let Some(t) = theta.as_f64() {
                    self.apply_cp(qubits[0], qubits[1], t);
                }
            }

            // Three-qubit gates
            StandardGate::CCX => self.apply_ccx(qubits[0], qubits[1], qubits[2]),
            StandardGate::CSwap => self.apply_cswap(qubits[0], qubits[1], qubits[2]),

            _ => {
                // Other gates not yet implemented
            }
        }
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

    fn reset(&mut self, qubit: usize) {
        // Simplified reset: project to |0⟩ and renormalize
        let mask = 1 << qubit;
        let mut norm_sq = 0.0;
        for i in 0..(1 << self.num_qubits) {
            if i & mask != 0 {
                let j = i & !mask;
                // Store the value first to avoid borrow conflict
                let val = self.amplitudes[i];
                self.amplitudes[j] += val;
                self.amplitudes[i] = Complex64::new(0.0, 0.0);
            }
            norm_sq += self.amplitudes[i].norm_sqr();
        }
        // Renormalize
        let norm = norm_sq.sqrt();
        if norm > 0.0 {
            for amp in &mut self.amplitudes {
                *amp /= norm;
            }
        }
    }

    /// Sample a measurement outcome.
    pub fn sample(&self) -> usize {
        use rand::Rng;
        let mut rng = rand::thread_rng();
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

    /// Convert measurement outcome to bitstring.
    pub fn outcome_to_bitstring(&self, outcome: usize) -> String {
        format!("{:0width$b}", outcome, width = self.num_qubits)
            .chars()
            .rev()
            .collect()
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

        for _ in 0..100 {
            assert_eq!(sv.sample(), 1);
        }
    }
}
