//! Quantum integer type.

use arvak_ir::Circuit;
use arvak_ir::qubit::QubitId;
use serde::{Deserialize, Serialize};

use crate::error::{TypeError, TypeResult};
use crate::register::QubitRegister;

/// A quantum integer with configurable bit width.
///
/// Quantum integer register with configurable signedness. When unsigned, uses
/// standard binary representation; when signed, uses two's complement.
/// The qubits are ordered from LSB (index 0) to MSB (index N-1).
///
/// # Type Parameter
///
/// - `N`: The number of bits (qubits) used to represent the integer.
///
/// # Example
///
/// ```ignore
/// use arvak_types::QuantumInt;
/// use arvak_ir::Circuit;
///
/// let mut circuit = Circuit::new("addition");
/// let a = QuantumInt::<4>::new(&mut circuit);  // 4-bit integer [0, 15]
/// let b = QuantumInt::<4>::new(&mut circuit);
///
/// // Initialize a to |5⟩
/// a.initialize(5, &mut circuit)?;
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuantumInt<const N: usize> {
    /// The qubit register holding this integer.
    register: QubitRegister,
    /// Whether this integer is signed.
    signed: bool,
}

impl<const N: usize> QuantumInt<N> {
    /// Create a new quantum integer, allocating qubits from the circuit.
    ///
    /// The integer starts in the |0⟩ state.
    pub fn new(circuit: &mut Circuit) -> Self {
        Self {
            register: QubitRegister::new(circuit, N),
            signed: false,
        }
    }

    /// Create a signed quantum integer.
    pub fn new_signed(circuit: &mut Circuit) -> Self {
        Self {
            register: QubitRegister::new(circuit, N),
            signed: true,
        }
    }

    /// Create a quantum integer from an existing qubit register.
    pub fn from_register(register: QubitRegister, signed: bool) -> TypeResult<Self> {
        if register.len() != N {
            return Err(TypeError::BitWidthMismatch {
                expected: N,
                got: register.len(),
            });
        }
        Ok(Self { register, signed })
    }

    /// Get the bit width.
    pub const fn bit_width(&self) -> usize {
        N
    }

    /// Check if this is a signed integer.
    pub fn is_signed(&self) -> bool {
        self.signed
    }

    /// Get the maximum representable value.
    pub fn max_value(&self) -> u64 {
        debug_assert!(N >= 1 && N <= 63, "QuantumInt bit width must be 1..=63");
        if self.signed {
            (1u64 << (N - 1)) - 1
        } else {
            (1u64 << N) - 1
        }
    }

    /// Get the minimum representable value.
    pub fn min_value(&self) -> i64 {
        debug_assert!(N >= 1 && N <= 63, "QuantumInt bit width must be 1..=63");
        if self.signed { -(1i64 << (N - 1)) } else { 0 }
    }

    /// Get the underlying qubit register.
    pub fn register(&self) -> &QubitRegister {
        &self.register
    }

    /// Get the qubits as a slice.
    pub fn qubits(&self) -> &[QubitId] {
        self.register.qubits()
    }

    /// Get qubit at specific bit position.
    pub fn bit(&self, index: usize) -> Option<QubitId> {
        self.register.qubit(index)
    }

    /// Get the LSB qubit.
    ///
    /// # Panics
    ///
    /// Panics if the register is empty (N == 0), which violates the type invariant.
    pub fn lsb(&self) -> QubitId {
        self.register
            .lsb()
            .expect("QuantumInt register must not be empty")
    }

    /// Get the MSB qubit (sign bit for signed integers).
    ///
    /// # Panics
    ///
    /// Panics if the register is empty (N == 0), which violates the type invariant.
    pub fn msb(&self) -> QubitId {
        self.register
            .msb()
            .expect("QuantumInt register must not be empty")
    }

    /// Initialize the quantum integer to a classical value.
    ///
    /// This applies X gates to set the appropriate bits.
    pub fn initialize(&self, value: u64, circuit: &mut Circuit) -> TypeResult<()> {
        if value > self.max_value() {
            return Err(TypeError::Overflow);
        }

        for i in 0..N {
            if (value >> i) & 1 == 1 {
                let qubit = self
                    .register
                    .qubit(i)
                    .ok_or(TypeError::IndexOutOfBounds { index: i, size: N })?;
                circuit
                    .x(qubit)
                    .map_err(|e| TypeError::CircuitError(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Apply a bitwise NOT to this integer (flip all bits).
    pub fn not(&self, circuit: &mut Circuit) -> TypeResult<()> {
        for qubit in self.register.iter() {
            circuit
                .x(qubit)
                .map_err(|e| TypeError::CircuitError(e.to_string()))?;
        }
        Ok(())
    }

    /// Apply a controlled NOT based on a control qubit.
    pub fn cnot(&self, control: QubitId, circuit: &mut Circuit) -> TypeResult<()> {
        for qubit in self.register.iter() {
            circuit
                .cx(control, qubit)
                .map_err(|e| TypeError::CircuitError(e.to_string()))?;
        }
        Ok(())
    }

    /// Increment this integer by 1.
    ///
    /// Note: This delegates to `add_classical`, which performs bitwise XOR
    /// (bit-flip) on individual bits, NOT arithmetic addition. Proper quantum
    /// addition with carry propagation requires ancilla qubits and is not yet
    /// implemented.
    pub fn increment(&self, circuit: &mut Circuit) -> TypeResult<()> {
        // Increment using cascading X and CX gates
        // Add 1: flip LSB, then cascade carries
        self.add_classical(1, circuit)
    }

    /// Decrement this integer by 1.
    ///
    /// Note: This relies on `increment`, which performs bitwise XOR
    /// (bit-flip) on individual bits, NOT arithmetic addition. Proper quantum
    /// addition with carry propagation requires ancilla qubits and is not yet
    /// implemented.
    pub fn decrement(&self, circuit: &mut Circuit) -> TypeResult<()> {
        // Decrement = add (2^N - 1) for unsigned
        // For simplicity, we flip all bits, increment, flip all bits
        // This is equivalent to subtracting 1
        self.not(circuit)?;
        self.increment(circuit)?;
        self.not(circuit)?;
        Ok(())
    }

    /// Add a classical constant to this integer in-place.
    ///
    /// Note: This performs bitwise XOR (bit-flip) on individual bits, NOT
    /// arithmetic addition. Proper quantum addition with carry propagation
    /// requires ancilla qubits and is not yet implemented.
    pub fn add_classical(&self, value: u64, circuit: &mut Circuit) -> TypeResult<()> {
        // Simple implementation: for each bit of the constant that's 1,
        // we need to propagate a carry through.
        // This is a simplified version - a full implementation would use
        // proper quantum arithmetic circuits.

        for i in 0..N {
            if (value >> i) & 1 == 1 {
                // Add 1 at position i: flip bit i and propagate carry
                let qubit = self
                    .register
                    .qubit(i)
                    .ok_or(TypeError::IndexOutOfBounds { index: i, size: N })?;
                circuit
                    .x(qubit)
                    .map_err(|e| TypeError::CircuitError(e.to_string()))?;

                // Propagate carry to higher bits
                // Note: This is a simplified carry propagation
                // A complete implementation would use controlled operations
            }
        }

        Ok(())
    }

    /// Swap the contents with another quantum integer.
    pub fn swap(&self, other: &QuantumInt<N>, circuit: &mut Circuit) -> TypeResult<()> {
        for i in 0..N {
            let q1 = self
                .register
                .qubit(i)
                .ok_or(TypeError::IndexOutOfBounds { index: i, size: N })?;
            let q2 = other
                .register
                .qubit(i)
                .ok_or(TypeError::IndexOutOfBounds { index: i, size: N })?;
            circuit
                .swap(q1, q2)
                .map_err(|e| TypeError::CircuitError(e.to_string()))?;
        }
        Ok(())
    }
}

/// Create two `QuantumInts` that share no qubits (for safe operations).
pub fn create_pair<const N: usize>(circuit: &mut Circuit) -> (QuantumInt<N>, QuantumInt<N>) {
    let a = QuantumInt::new(circuit);
    let b = QuantumInt::new(circuit);
    (a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantum_int_creation() {
        let mut circuit = Circuit::new("test");
        let qi = QuantumInt::<4>::new(&mut circuit);

        assert_eq!(qi.bit_width(), 4);
        assert_eq!(qi.qubits().len(), 4);
        assert!(!qi.is_signed());
        assert_eq!(qi.max_value(), 15);
    }

    #[test]
    fn test_quantum_int_signed() {
        let mut circuit = Circuit::new("test");
        let qi = QuantumInt::<4>::new_signed(&mut circuit);

        assert!(qi.is_signed());
        assert_eq!(qi.max_value(), 7); // 2^3 - 1
        assert_eq!(qi.min_value(), -8); // -2^3
    }

    #[test]
    fn test_quantum_int_initialize() {
        let mut circuit = Circuit::new("test");
        let qi = QuantumInt::<4>::new(&mut circuit);

        // Initialize to 5 (binary: 0101)
        qi.initialize(5, &mut circuit).unwrap();

        // Circuit should have X gates on bits 0 and 2
        assert_eq!(circuit.dag().num_ops(), 2);
    }

    #[test]
    fn test_quantum_int_overflow() {
        let mut circuit = Circuit::new("test");
        let qi = QuantumInt::<4>::new(&mut circuit);

        // Try to initialize to 16 (exceeds 4-bit max)
        let result = qi.initialize(16, &mut circuit);
        assert!(result.is_err());
    }

    #[test]
    fn test_quantum_int_pair() {
        let mut circuit = Circuit::new("test");
        let (a, b) = create_pair::<4>(&mut circuit);

        assert_eq!(circuit.num_qubits(), 8);

        // Qubits should not overlap
        for qa in a.qubits() {
            for qb in b.qubits() {
                assert_ne!(qa, qb);
            }
        }
    }
}
