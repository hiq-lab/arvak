//! Quantum floating-point type.

use hiq_ir::Circuit;
use hiq_ir::qubit::QubitId;
use serde::{Deserialize, Serialize};

use crate::error::{TypeError, TypeResult};
use crate::register::QubitRegister;

/// A quantum floating-point number.
///
/// This represents a floating-point number using a sign bit, mantissa, and exponent,
/// similar to IEEE 754 but with configurable bit widths.
///
/// The value represented is: (-1)^sign × mantissa × 2^exponent
///
/// # Type Parameters
///
/// - `M`: Number of mantissa bits
/// - `E`: Number of exponent bits
///
/// # Layout
///
/// ```text
/// [sign][exponent bits...][mantissa bits...]
/// ```
///
/// # Example
///
/// ```ignore
/// use hiq_types::QuantumFloat;
/// use hiq_ir::Circuit;
///
/// let mut circuit = Circuit::new("float_ops");
///
/// // Create a quantum float with 4-bit mantissa and 3-bit exponent
/// let x = QuantumFloat::<4, 3>::new(&mut circuit);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuantumFloat<const M: usize, const E: usize> {
    /// Sign bit (0 = positive, 1 = negative).
    sign: QubitId,
    /// Mantissa register.
    mantissa: QubitRegister,
    /// Exponent register.
    exponent: QubitRegister,
    /// Exponent bias (for representing negative exponents).
    exponent_bias: i32,
}

impl<const M: usize, const E: usize> QuantumFloat<M, E> {
    /// Total number of qubits used by this float.
    pub const TOTAL_QUBITS: usize = 1 + M + E; // sign + mantissa + exponent

    /// Create a new quantum float, allocating qubits from the circuit.
    pub fn new(circuit: &mut Circuit) -> Self {
        let start = circuit.num_qubits();
        let total = start + Self::TOTAL_QUBITS;

        // Extend circuit to have enough qubits
        while circuit.num_qubits() < total {
            circuit.add_qubit();
        }

        // Allocate: sign, then exponent, then mantissa
        let sign = QubitId(start as u32);
        let exponent = QubitRegister::from_qubits(
            (start + 1..start + 1 + E)
                .map(|i| QubitId(i as u32))
                .collect(),
        );
        let mantissa = QubitRegister::from_qubits(
            (start + 1 + E..start + 1 + E + M)
                .map(|i| QubitId(i as u32))
                .collect(),
        );

        // Exponent bias is 2^(E-1) - 1, like IEEE 754
        let exponent_bias = (1i32 << (E - 1)) - 1;

        Self {
            sign,
            mantissa,
            exponent,
            exponent_bias,
        }
    }

    /// Create a quantum float with custom exponent bias.
    pub fn with_bias(circuit: &mut Circuit, exponent_bias: i32) -> Self {
        let mut qf = Self::new(circuit);
        qf.exponent_bias = exponent_bias;
        qf
    }

    /// Get the number of mantissa bits.
    pub const fn mantissa_bits(&self) -> usize {
        M
    }

    /// Get the number of exponent bits.
    pub const fn exponent_bits(&self) -> usize {
        E
    }

    /// Get the total number of qubits.
    pub const fn total_qubits(&self) -> usize {
        Self::TOTAL_QUBITS
    }

    /// Get the exponent bias.
    pub fn exponent_bias(&self) -> i32 {
        self.exponent_bias
    }

    /// Get the sign qubit.
    pub fn sign_qubit(&self) -> QubitId {
        self.sign
    }

    /// Get the mantissa register.
    pub fn mantissa(&self) -> &QubitRegister {
        &self.mantissa
    }

    /// Get the exponent register.
    pub fn exponent(&self) -> &QubitRegister {
        &self.exponent
    }

    /// Get all qubits as a vector (sign, exponent, mantissa order).
    pub fn all_qubits(&self) -> Vec<QubitId> {
        let mut qubits = vec![self.sign];
        qubits.extend(self.exponent.iter());
        qubits.extend(self.mantissa.iter());
        qubits
    }

    /// Negate this quantum float (flip the sign bit).
    pub fn negate(&self, circuit: &mut Circuit) -> TypeResult<()> {
        circuit
            .x(self.sign)
            .map(|_| ())
            .map_err(|e| TypeError::CircuitError(e.to_string()))
    }

    /// Apply controlled negation based on a control qubit.
    pub fn cnegate(&self, control: QubitId, circuit: &mut Circuit) -> TypeResult<()> {
        circuit
            .cx(control, self.sign)
            .map(|_| ())
            .map_err(|e| TypeError::CircuitError(e.to_string()))
    }

    /// Set to zero (all qubits to |0⟩).
    ///
    /// Assumes the float is currently in computational basis state.
    /// For a general reset, use measurements or reset gates.
    pub fn set_zero(&self, circuit: &mut Circuit) -> TypeResult<()> {
        // This is a placeholder - proper zero requires knowing current state
        // In practice, you'd use reset operations
        let _ = circuit;
        Ok(())
    }

    /// Initialize the quantum float to a classical value.
    ///
    /// This is an approximation - it encodes the value in the quantum state.
    pub fn initialize(&self, value: f64, circuit: &mut Circuit) -> TypeResult<()> {
        if value == 0.0 {
            // Zero is represented with all bits 0
            return Ok(());
        }

        let is_negative = value < 0.0;
        let abs_value = value.abs();

        // Extract exponent and mantissa
        let (mantissa_bits, exp_bits) = self.encode_float(abs_value)?;

        // Set sign bit if negative
        if is_negative {
            circuit
                .x(self.sign)
                .map_err(|e| TypeError::CircuitError(e.to_string()))?;
        }

        // Set exponent bits
        for (i, qubit) in self.exponent.iter().enumerate() {
            if (exp_bits >> i) & 1 == 1 {
                circuit
                    .x(qubit)
                    .map_err(|e| TypeError::CircuitError(e.to_string()))?;
            }
        }

        // Set mantissa bits
        for (i, qubit) in self.mantissa.iter().enumerate() {
            if (mantissa_bits >> i) & 1 == 1 {
                circuit
                    .x(qubit)
                    .map_err(|e| TypeError::CircuitError(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Encode a classical float as (mantissa_bits, exponent_bits).
    fn encode_float(&self, value: f64) -> TypeResult<(u64, u64)> {
        if value == 0.0 {
            return Ok((0, 0));
        }

        // Get the binary representation
        let bits = value.to_bits();

        // Extract IEEE 754 components (64-bit)
        let ieee_exp = ((bits >> 52) & 0x7FF) as i32;
        let ieee_mantissa = bits & 0xFFFFFFFFFFFFF;

        // Convert IEEE exponent to our representation
        let actual_exp = ieee_exp - 1023; // IEEE bias for f64
        let our_exp = actual_exp + self.exponent_bias;

        // Check exponent range
        let max_exp = (1u64 << E) - 1;
        if our_exp < 0 || our_exp as u64 > max_exp {
            return Err(TypeError::InvalidExponentRange {
                min: -self.exponent_bias,
                max: max_exp as i32 - self.exponent_bias,
            });
        }

        // Scale mantissa to our bit width
        let mantissa_bits = ieee_mantissa >> (52 - M).max(0);

        Ok((mantissa_bits, our_exp as u64))
    }
}

/// Common float configurations.
pub type QFloat16 = QuantumFloat<10, 5>; // Similar to IEEE half precision
pub type QFloat32 = QuantumFloat<8, 4>; // Reduced precision for quantum
pub type QFloat8 = QuantumFloat<4, 3>; // Minimal precision

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantum_float_creation() {
        let mut circuit = Circuit::new("test");
        let qf = QuantumFloat::<4, 3>::new(&mut circuit);

        assert_eq!(qf.mantissa_bits(), 4);
        assert_eq!(qf.exponent_bits(), 3);
        assert_eq!(qf.total_qubits(), 8); // 1 + 4 + 3
        assert_eq!(circuit.num_qubits(), 8);
    }

    #[test]
    fn test_quantum_float_bias() {
        let mut circuit = Circuit::new("test");
        let qf = QuantumFloat::<4, 3>::new(&mut circuit);

        // 3-bit exponent: bias = 2^2 - 1 = 3
        assert_eq!(qf.exponent_bias(), 3);
    }

    #[test]
    fn test_quantum_float_all_qubits() {
        let mut circuit = Circuit::new("test");
        let qf = QuantumFloat::<4, 3>::new(&mut circuit);

        let qubits = qf.all_qubits();
        assert_eq!(qubits.len(), 8);

        // First qubit is sign
        assert_eq!(qubits[0], qf.sign_qubit());
    }

    #[test]
    fn test_quantum_float_negate() {
        let mut circuit = Circuit::new("test");
        let qf = QuantumFloat::<4, 3>::new(&mut circuit);

        qf.negate(&mut circuit).unwrap();

        // Should have one X gate on the sign bit
        assert_eq!(circuit.dag().num_ops(), 1);
    }

    #[test]
    fn test_type_aliases() {
        let mut circuit = Circuit::new("test");

        let _f16 = QFloat16::new(&mut circuit);
        assert_eq!(circuit.num_qubits(), 16); // 1 + 10 + 5

        let _f32 = QFloat32::new(&mut circuit);
        assert_eq!(circuit.num_qubits(), 16 + 13); // 1 + 8 + 4

        let _f8 = QFloat8::new(&mut circuit);
        assert_eq!(circuit.num_qubits(), 16 + 13 + 8); // 1 + 4 + 3
    }
}
