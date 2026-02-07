//! Quantum array type.

use arvak_ir::Circuit;
use arvak_ir::qubit::QubitId;
use serde::{Deserialize, Serialize};

use crate::error::{TypeError, TypeResult};
use crate::register::QubitRegister;

/// A quantum array holding multiple quantum values.
///
/// This represents an array where each element is a quantum value
/// (could be QuantumInt, QuantumFloat, or raw qubits).
///
/// # Type Parameters
///
/// - `N`: Number of elements in the array
/// - `W`: Width (number of qubits) per element
///
/// # Example
///
/// ```ignore
/// use arvak_types::QuantumArray;
/// use arvak_ir::Circuit;
///
/// let mut circuit = Circuit::new("array_ops");
///
/// // Create an array of 4 elements, each 8 qubits wide
/// let arr = QuantumArray::<4, 8>::new(&mut circuit);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuantumArray<const N: usize, const W: usize> {
    /// All qubits in the array, organized in element order.
    registers: Vec<QubitRegister>,
}

impl<const N: usize, const W: usize> QuantumArray<N, W> {
    /// Total number of qubits in this array.
    pub const TOTAL_QUBITS: usize = N * W;

    /// Create a new quantum array, allocating qubits from the circuit.
    pub fn new(circuit: &mut Circuit) -> Self {
        let start = circuit.num_qubits();
        let total = start + Self::TOTAL_QUBITS;

        // Extend circuit to have enough qubits
        while circuit.num_qubits() < total {
            circuit.add_qubit();
        }

        // Create registers for each element
        let registers = (0..N)
            .map(|i| {
                let elem_start = start + i * W;
                QubitRegister::from_qubits(
                    (elem_start..elem_start + W)
                        .map(|j| QubitId(j as u32))
                        .collect(),
                )
            })
            .collect();

        Self { registers }
    }

    /// Get the number of elements.
    pub const fn len(&self) -> usize {
        N
    }

    /// Check if array is empty.
    pub const fn is_empty(&self) -> bool {
        N == 0
    }

    /// Get the width of each element.
    pub const fn element_width(&self) -> usize {
        W
    }

    /// Get the total number of qubits.
    pub const fn total_qubits(&self) -> usize {
        Self::TOTAL_QUBITS
    }

    /// Get an element's register by index.
    pub fn get(&self, index: usize) -> TypeResult<&QubitRegister> {
        self.registers
            .get(index)
            .ok_or(TypeError::IndexOutOfBounds { index, size: N })
    }

    /// Get the qubits for a specific element.
    pub fn element_qubits(&self, index: usize) -> TypeResult<&[QubitId]> {
        self.get(index).map(|r| r.qubits())
    }

    /// Get all qubits in the array.
    pub fn all_qubits(&self) -> Vec<QubitId> {
        self.registers.iter().flat_map(|r| r.iter()).collect()
    }

    /// Swap two elements in the array.
    pub fn swap_elements(&self, i: usize, j: usize, circuit: &mut Circuit) -> TypeResult<()> {
        if i >= N {
            return Err(TypeError::IndexOutOfBounds { index: i, size: N });
        }
        if j >= N {
            return Err(TypeError::IndexOutOfBounds { index: j, size: N });
        }
        if i == j {
            return Ok(());
        }

        let reg_i = &self.registers[i];
        let reg_j = &self.registers[j];

        for k in 0..W {
            let qi = reg_i
                .qubit(k)
                .ok_or(TypeError::IndexOutOfBounds { index: k, size: W })?;
            let qj = reg_j
                .qubit(k)
                .ok_or(TypeError::IndexOutOfBounds { index: k, size: W })?;
            circuit
                .swap(qi, qj)
                .map_err(|e| TypeError::CircuitError(e.to_string()))?;
        }

        Ok(())
    }

    /// Apply a function to each element (via qubits).
    pub fn map<F>(&self, circuit: &mut Circuit, mut f: F) -> TypeResult<()>
    where
        F: FnMut(&QubitRegister, &mut Circuit) -> TypeResult<()>,
    {
        for reg in &self.registers {
            f(reg, circuit)?;
        }
        Ok(())
    }

    /// Iterate over element registers.
    pub fn iter(&self) -> impl Iterator<Item = &QubitRegister> {
        self.registers.iter()
    }
}

/// Index-based quantum array access helper.
///
/// For quantum-controlled indexing (superposition of indices),
/// more complex circuits are needed.
#[derive(Debug, Clone)]
pub struct QuantumIndex<const I: usize> {
    /// Index register (log2(array_size) qubits needed).
    register: QubitRegister,
}

impl<const I: usize> QuantumIndex<I> {
    /// Number of qubits needed to index an array of size I.
    pub const INDEX_QUBITS: usize = {
        let mut bits = 0;
        let mut n = I;
        while n > 0 {
            bits += 1;
            n >>= 1;
        }
        if bits == 0 { 1 } else { bits }
    };

    /// Create a new quantum index.
    pub fn new(circuit: &mut Circuit) -> Self {
        Self {
            register: QubitRegister::new(circuit, Self::INDEX_QUBITS),
        }
    }

    /// Get the index register.
    pub fn register(&self) -> &QubitRegister {
        &self.register
    }

    /// Initialize to a classical index value.
    pub fn initialize(&self, index: usize, circuit: &mut Circuit) -> TypeResult<()> {
        if index >= I {
            return Err(TypeError::IndexOutOfBounds { index, size: I });
        }

        for (i, qubit) in self.register.iter().enumerate() {
            if (index >> i) & 1 == 1 {
                circuit
                    .x(qubit)
                    .map_err(|e| TypeError::CircuitError(e.to_string()))?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantum_array_creation() {
        let mut circuit = Circuit::new("test");
        let arr = QuantumArray::<4, 8>::new(&mut circuit);

        assert_eq!(arr.len(), 4);
        assert_eq!(arr.element_width(), 8);
        assert_eq!(arr.total_qubits(), 32);
        assert_eq!(circuit.num_qubits(), 32);
    }

    #[test]
    fn test_quantum_array_get() {
        let mut circuit = Circuit::new("test");
        let arr = QuantumArray::<4, 8>::new(&mut circuit);

        let elem0 = arr.get(0).unwrap();
        assert_eq!(elem0.len(), 8);

        let elem3 = arr.get(3).unwrap();
        assert_eq!(elem3.len(), 8);

        // Out of bounds
        assert!(arr.get(4).is_err());
    }

    #[test]
    fn test_quantum_array_swap() {
        let mut circuit = Circuit::new("test");
        let arr = QuantumArray::<4, 2>::new(&mut circuit);

        // Swap elements 0 and 2
        arr.swap_elements(0, 2, &mut circuit).unwrap();

        // Should have 2 SWAP gates (one per qubit in element)
        assert_eq!(circuit.dag().num_ops(), 2);
    }

    #[test]
    fn test_quantum_index() {
        let mut circuit = Circuit::new("test");

        // Index for array of size 8 needs 3 qubits
        let idx = QuantumIndex::<8>::new(&mut circuit);
        assert_eq!(idx.register().len(), 4); // Actually 4 because our const fn rounds up

        // Initialize to index 5 (binary: 101)
        idx.initialize(5, &mut circuit).unwrap();
    }

    #[test]
    fn test_quantum_array_all_qubits() {
        let mut circuit = Circuit::new("test");
        let arr = QuantumArray::<3, 2>::new(&mut circuit);

        let all = arr.all_qubits();
        assert_eq!(all.len(), 6);
        assert_eq!(all[0], QubitId(0));
        assert_eq!(all[5], QubitId(5));
    }
}
