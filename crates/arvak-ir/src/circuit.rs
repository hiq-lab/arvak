//! High-level circuit builder API.

use crate::dag::CircuitDag;
use crate::error::IrResult;
use crate::gate::{Gate, StandardGate};
use crate::instruction::Instruction;
use crate::parameter::ParameterExpression;
use crate::qubit::{Clbit, ClbitId, Qubit, QubitId};

/// A quantum circuit.
///
/// This provides a high-level API for building quantum circuits,
/// with convenient methods for common gates and operations.
pub struct Circuit {
    /// Name of the circuit.
    name: String,
    /// Qubits in the circuit.
    qubits: Vec<Qubit>,
    /// Classical bits in the circuit.
    clbits: Vec<Clbit>,
    /// The underlying DAG representation.
    dag: CircuitDag,
    /// Counter for generating qubit IDs.
    next_qubit_id: u32,
    /// Counter for generating classical bit IDs.
    next_clbit_id: u32,
}

impl Circuit {
    /// Create a new empty circuit.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            qubits: vec![],
            clbits: vec![],
            dag: CircuitDag::new(),
            next_qubit_id: 0,
            next_clbit_id: 0,
        }
    }

    /// Create a circuit with a given number of qubits and classical bits.
    pub fn with_size(name: impl Into<String>, num_qubits: u32, num_clbits: u32) -> Self {
        let mut circuit = Self::new(name);
        for _ in 0..num_qubits {
            circuit.add_qubit();
        }
        for _ in 0..num_clbits {
            circuit.add_clbit();
        }
        circuit
    }

    /// Add a single qubit to the circuit.
    pub fn add_qubit(&mut self) -> QubitId {
        let id = QubitId(self.next_qubit_id);
        self.next_qubit_id += 1;
        let qubit = Qubit::new(id);
        self.qubits.push(qubit);
        self.dag.add_qubit(id);
        id
    }

    /// Add a quantum register with multiple qubits.
    pub fn add_qreg(&mut self, name: impl Into<String>, size: u32) -> Vec<QubitId> {
        let name = name.into();
        let mut ids = vec![];
        for i in 0..size {
            let id = QubitId(self.next_qubit_id);
            self.next_qubit_id += 1;
            let qubit = Qubit::with_register(id, &name, i);
            self.qubits.push(qubit);
            self.dag.add_qubit(id);
            ids.push(id);
        }
        ids
    }

    /// Add a single classical bit to the circuit.
    pub fn add_clbit(&mut self) -> ClbitId {
        let id = ClbitId(self.next_clbit_id);
        self.next_clbit_id += 1;
        let clbit = Clbit::new(id);
        self.clbits.push(clbit);
        self.dag.add_clbit(id);
        id
    }

    /// Add a classical register with multiple bits.
    pub fn add_creg(&mut self, name: impl Into<String>, size: u32) -> Vec<ClbitId> {
        let name = name.into();
        let mut ids = vec![];
        for i in 0..size {
            let id = ClbitId(self.next_clbit_id);
            self.next_clbit_id += 1;
            let clbit = Clbit::with_register(id, &name, i);
            self.clbits.push(clbit);
            self.dag.add_clbit(id);
            ids.push(id);
        }
        ids
    }

    // =========================================================================
    // Single-qubit gates
    // =========================================================================

    /// Apply Hadamard gate.
    pub fn h(&mut self, qubit: QubitId) -> IrResult<&mut Self> {
        self.dag
            .apply(Instruction::single_qubit_gate(StandardGate::H, qubit))?;
        Ok(self)
    }

    /// Apply Pauli-X gate.
    pub fn x(&mut self, qubit: QubitId) -> IrResult<&mut Self> {
        self.dag
            .apply(Instruction::single_qubit_gate(StandardGate::X, qubit))?;
        Ok(self)
    }

    /// Apply Pauli-Y gate.
    pub fn y(&mut self, qubit: QubitId) -> IrResult<&mut Self> {
        self.dag
            .apply(Instruction::single_qubit_gate(StandardGate::Y, qubit))?;
        Ok(self)
    }

    /// Apply Pauli-Z gate.
    pub fn z(&mut self, qubit: QubitId) -> IrResult<&mut Self> {
        self.dag
            .apply(Instruction::single_qubit_gate(StandardGate::Z, qubit))?;
        Ok(self)
    }

    /// Apply S gate.
    pub fn s(&mut self, qubit: QubitId) -> IrResult<&mut Self> {
        self.dag
            .apply(Instruction::single_qubit_gate(StandardGate::S, qubit))?;
        Ok(self)
    }

    /// Apply S-dagger gate.
    pub fn sdg(&mut self, qubit: QubitId) -> IrResult<&mut Self> {
        self.dag
            .apply(Instruction::single_qubit_gate(StandardGate::Sdg, qubit))?;
        Ok(self)
    }

    /// Apply T gate.
    pub fn t(&mut self, qubit: QubitId) -> IrResult<&mut Self> {
        self.dag
            .apply(Instruction::single_qubit_gate(StandardGate::T, qubit))?;
        Ok(self)
    }

    /// Apply T-dagger gate.
    pub fn tdg(&mut self, qubit: QubitId) -> IrResult<&mut Self> {
        self.dag
            .apply(Instruction::single_qubit_gate(StandardGate::Tdg, qubit))?;
        Ok(self)
    }

    /// Apply sqrt(X) gate.
    pub fn sx(&mut self, qubit: QubitId) -> IrResult<&mut Self> {
        self.dag
            .apply(Instruction::single_qubit_gate(StandardGate::SX, qubit))?;
        Ok(self)
    }

    /// Apply sqrt(X)-dagger gate.
    pub fn sxdg(&mut self, qubit: QubitId) -> IrResult<&mut Self> {
        self.dag
            .apply(Instruction::single_qubit_gate(StandardGate::SXdg, qubit))?;
        Ok(self)
    }

    /// Apply Rx rotation gate.
    pub fn rx(
        &mut self,
        theta: impl Into<ParameterExpression>,
        qubit: QubitId,
    ) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::single_qubit_gate(
            StandardGate::Rx(theta.into()),
            qubit,
        ))?;
        Ok(self)
    }

    /// Apply Ry rotation gate.
    pub fn ry(
        &mut self,
        theta: impl Into<ParameterExpression>,
        qubit: QubitId,
    ) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::single_qubit_gate(
            StandardGate::Ry(theta.into()),
            qubit,
        ))?;
        Ok(self)
    }

    /// Apply Rz rotation gate.
    pub fn rz(
        &mut self,
        theta: impl Into<ParameterExpression>,
        qubit: QubitId,
    ) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::single_qubit_gate(
            StandardGate::Rz(theta.into()),
            qubit,
        ))?;
        Ok(self)
    }

    /// Apply phase gate.
    pub fn p(
        &mut self,
        theta: impl Into<ParameterExpression>,
        qubit: QubitId,
    ) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::single_qubit_gate(
            StandardGate::P(theta.into()),
            qubit,
        ))?;
        Ok(self)
    }

    /// Apply universal U gate.
    pub fn u(
        &mut self,
        theta: impl Into<ParameterExpression>,
        phi: impl Into<ParameterExpression>,
        lambda: impl Into<ParameterExpression>,
        qubit: QubitId,
    ) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::single_qubit_gate(
            StandardGate::U(theta.into(), phi.into(), lambda.into()),
            qubit,
        ))?;
        Ok(self)
    }

    // =========================================================================
    // Two-qubit gates
    // =========================================================================

    /// Apply CNOT (CX) gate.
    pub fn cx(&mut self, control: QubitId, target: QubitId) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::two_qubit_gate(
            StandardGate::CX,
            control,
            target,
        ))?;
        Ok(self)
    }

    /// Apply CY gate.
    pub fn cy(&mut self, control: QubitId, target: QubitId) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::two_qubit_gate(
            StandardGate::CY,
            control,
            target,
        ))?;
        Ok(self)
    }

    /// Apply CZ gate.
    pub fn cz(&mut self, control: QubitId, target: QubitId) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::two_qubit_gate(
            StandardGate::CZ,
            control,
            target,
        ))?;
        Ok(self)
    }

    /// Apply SWAP gate.
    pub fn swap(&mut self, q1: QubitId, q2: QubitId) -> IrResult<&mut Self> {
        self.dag
            .apply(Instruction::two_qubit_gate(StandardGate::Swap, q1, q2))?;
        Ok(self)
    }

    /// Apply iSWAP gate.
    pub fn iswap(&mut self, q1: QubitId, q2: QubitId) -> IrResult<&mut Self> {
        self.dag
            .apply(Instruction::two_qubit_gate(StandardGate::ISwap, q1, q2))?;
        Ok(self)
    }

    /// Apply controlled-Rz gate.
    pub fn crz(
        &mut self,
        theta: impl Into<ParameterExpression>,
        control: QubitId,
        target: QubitId,
    ) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::two_qubit_gate(
            StandardGate::CRz(theta.into()),
            control,
            target,
        ))?;
        Ok(self)
    }

    /// Apply controlled-phase gate.
    pub fn cp(
        &mut self,
        theta: impl Into<ParameterExpression>,
        control: QubitId,
        target: QubitId,
    ) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::two_qubit_gate(
            StandardGate::CP(theta.into()),
            control,
            target,
        ))?;
        Ok(self)
    }

    /// Apply controlled-Hadamard gate.
    pub fn ch(&mut self, control: QubitId, target: QubitId) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::two_qubit_gate(
            StandardGate::CH,
            control,
            target,
        ))?;
        Ok(self)
    }

    /// Apply controlled-Rx gate.
    pub fn crx(
        &mut self,
        theta: impl Into<ParameterExpression>,
        control: QubitId,
        target: QubitId,
    ) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::two_qubit_gate(
            StandardGate::CRx(theta.into()),
            control,
            target,
        ))?;
        Ok(self)
    }

    /// Apply controlled-Ry gate.
    pub fn cry(
        &mut self,
        theta: impl Into<ParameterExpression>,
        control: QubitId,
        target: QubitId,
    ) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::two_qubit_gate(
            StandardGate::CRy(theta.into()),
            control,
            target,
        ))?;
        Ok(self)
    }

    /// Apply RXX (XX rotation) gate.
    pub fn rxx(
        &mut self,
        theta: impl Into<ParameterExpression>,
        q1: QubitId,
        q2: QubitId,
    ) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::two_qubit_gate(
            StandardGate::RXX(theta.into()),
            q1,
            q2,
        ))?;
        Ok(self)
    }

    /// Apply RYY (YY rotation) gate.
    pub fn ryy(
        &mut self,
        theta: impl Into<ParameterExpression>,
        q1: QubitId,
        q2: QubitId,
    ) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::two_qubit_gate(
            StandardGate::RYY(theta.into()),
            q1,
            q2,
        ))?;
        Ok(self)
    }

    /// Apply RZZ (ZZ rotation) gate.
    pub fn rzz(
        &mut self,
        theta: impl Into<ParameterExpression>,
        q1: QubitId,
        q2: QubitId,
    ) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::two_qubit_gate(
            StandardGate::RZZ(theta.into()),
            q1,
            q2,
        ))?;
        Ok(self)
    }

    // =========================================================================
    // IQM native gates
    // =========================================================================

    /// Apply phased RX gate (IQM native).
    pub fn prx(
        &mut self,
        theta: impl Into<ParameterExpression>,
        phi: impl Into<ParameterExpression>,
        qubit: QubitId,
    ) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::single_qubit_gate(
            StandardGate::PRX(theta.into(), phi.into()),
            qubit,
        ))?;
        Ok(self)
    }

    // =========================================================================
    // Three-qubit gates
    // =========================================================================

    /// Apply Toffoli (CCX) gate.
    pub fn ccx(&mut self, c1: QubitId, c2: QubitId, target: QubitId) -> IrResult<&mut Self> {
        self.dag
            .apply(Instruction::gate(StandardGate::CCX, [c1, c2, target]))?;
        Ok(self)
    }

    /// Apply Fredkin (CSWAP) gate.
    pub fn cswap(&mut self, control: QubitId, t1: QubitId, t2: QubitId) -> IrResult<&mut Self> {
        self.dag
            .apply(Instruction::gate(StandardGate::CSwap, [control, t1, t2]))?;
        Ok(self)
    }

    // =========================================================================
    // Other operations
    // =========================================================================

    /// Apply a custom gate.
    pub fn gate(
        &mut self,
        gate: impl Into<Gate>,
        qubits: impl IntoIterator<Item = QubitId>,
    ) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::gate(gate, qubits))?;
        Ok(self)
    }

    /// Measure a qubit to a classical bit.
    pub fn measure(&mut self, qubit: QubitId, clbit: ClbitId) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::measure(qubit, clbit))?;
        Ok(self)
    }

    /// Measure all qubits to corresponding classical bits.
    pub fn measure_all(&mut self) -> IrResult<&mut Self> {
        // Ensure we have enough classical bits
        while self.clbits.len() < self.qubits.len() {
            self.add_clbit();
        }

        let qubits: Vec<_> = self.qubits.iter().map(|q| q.id).collect();
        let clbits: Vec<_> = self
            .clbits
            .iter()
            .map(|c| c.id)
            .take(qubits.len())
            .collect();

        self.dag.apply(Instruction::measure_all(qubits, clbits))?;
        Ok(self)
    }

    /// Reset a qubit to |0⟩.
    pub fn reset(&mut self, qubit: QubitId) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::reset(qubit))?;
        Ok(self)
    }

    /// Apply a barrier to specified qubits.
    pub fn barrier(&mut self, qubits: impl IntoIterator<Item = QubitId>) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::barrier(qubits))?;
        Ok(self)
    }

    /// Apply a barrier to all qubits.
    pub fn barrier_all(&mut self) -> IrResult<&mut Self> {
        let qubits: Vec<_> = self.qubits.iter().map(|q| q.id).collect();
        self.dag.apply(Instruction::barrier(qubits))?;
        Ok(self)
    }

    /// Apply a delay to a qubit.
    pub fn delay(&mut self, qubit: QubitId, duration: u64) -> IrResult<&mut Self> {
        self.dag.apply(Instruction::delay(qubit, duration))?;
        Ok(self)
    }

    // =========================================================================
    // Accessors
    // =========================================================================

    /// Get the circuit name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the number of qubits.
    pub fn num_qubits(&self) -> usize {
        self.qubits.len()
    }

    /// Get the number of classical bits.
    pub fn num_clbits(&self) -> usize {
        self.clbits.len()
    }

    /// Get the circuit depth.
    pub fn depth(&self) -> usize {
        self.dag.depth()
    }

    /// Get a reference to the underlying DAG.
    pub fn dag(&self) -> &CircuitDag {
        &self.dag
    }

    /// Get a mutable reference to the underlying DAG.
    pub fn dag_mut(&mut self) -> &mut CircuitDag {
        &mut self.dag
    }

    /// Consume the circuit and return the DAG.
    pub fn into_dag(self) -> CircuitDag {
        self.dag
    }

    /// Create a circuit from a DAG.
    pub fn from_dag(dag: CircuitDag) -> Self {
        let num_qubits = dag.num_qubits() as u32;
        let num_clbits = dag.num_clbits() as u32;

        let qubits: Vec<_> = dag.qubits().map(Qubit::new).collect();
        let clbits: Vec<_> = dag.clbits().map(Clbit::new).collect();

        Self {
            name: "circuit".into(),
            qubits,
            clbits,
            dag,
            next_qubit_id: num_qubits,
            next_clbit_id: num_clbits,
        }
    }

    /// Get the qubits in the circuit.
    pub fn qubits(&self) -> &[Qubit] {
        &self.qubits
    }

    /// Get the classical bits in the circuit.
    pub fn clbits(&self) -> &[Clbit] {
        &self.clbits
    }

    // =========================================================================
    // Pre-built circuits
    // =========================================================================

    /// Create a Bell state circuit.
    pub fn bell() -> IrResult<Self> {
        let mut circuit = Self::with_size("bell", 2, 2);
        let q0 = QubitId(0);
        let q1 = QubitId(1);
        let c0 = ClbitId(0);
        let c1 = ClbitId(1);

        circuit
            .h(q0)?
            .cx(q0, q1)?
            .measure(q0, c0)?
            .measure(q1, c1)?;

        Ok(circuit)
    }

    /// Create a GHZ state circuit.
    pub fn ghz(n: u32) -> IrResult<Self> {
        if n == 0 {
            return Ok(Self::new("ghz_0"));
        }

        let mut circuit = Self::with_size("ghz", n, n);

        // H on first qubit
        circuit.h(QubitId(0))?;

        // CNOT chain
        for i in 0..n - 1 {
            circuit.cx(QubitId(i), QubitId(i + 1))?;
        }

        // Measure all
        for i in 0..n {
            circuit.measure(QubitId(i), ClbitId(i))?;
        }

        Ok(circuit)
    }

    /// Create a QFT circuit (without measurements).
    pub fn qft(n: u32) -> IrResult<Self> {
        use std::f64::consts::PI;

        if n == 0 {
            return Ok(Self::new("qft_0"));
        }

        let mut circuit = Self::with_size("qft", n, 0);

        for i in 0..n {
            // Hadamard on qubit i
            circuit.h(QubitId(i))?;

            // Controlled rotations
            for j in (i + 1)..n {
                let k = j - i;
                let angle = PI / (1 << k) as f64;
                circuit.cp(angle, QubitId(j), QubitId(i))?;
            }
        }

        // Swap qubits for bit reversal
        for i in 0..n / 2 {
            circuit.swap(QubitId(i), QubitId(n - 1 - i))?;
        }

        Ok(circuit)
    }
}

impl Clone for Circuit {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            qubits: self.qubits.clone(),
            clbits: self.clbits.clone(),
            dag: self.dag.clone(),
            next_qubit_id: self.next_qubit_id,
            next_clbit_id: self.next_clbit_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn test_new_circuit() {
        let circuit = Circuit::new("test");
        assert_eq!(circuit.name(), "test");
        assert_eq!(circuit.num_qubits(), 0);
        assert_eq!(circuit.num_clbits(), 0);
    }

    #[test]
    fn test_circuit_with_size() {
        let circuit = Circuit::with_size("test", 3, 2);
        assert_eq!(circuit.num_qubits(), 3);
        assert_eq!(circuit.num_clbits(), 2);
    }

    #[test]
    fn test_add_registers() {
        let mut circuit = Circuit::new("test");
        let qreg = circuit.add_qreg("q", 4);
        let creg = circuit.add_creg("c", 4);

        assert_eq!(qreg.len(), 4);
        assert_eq!(creg.len(), 4);
        assert_eq!(circuit.num_qubits(), 4);
        assert_eq!(circuit.num_clbits(), 4);
    }

    #[test]
    fn test_bell_state() {
        let circuit = Circuit::bell().unwrap();
        assert_eq!(circuit.num_qubits(), 2);
        assert_eq!(circuit.num_clbits(), 2);
        assert_eq!(circuit.depth(), 3); // H, CX, parallel measures
    }

    #[test]
    fn test_ghz_state() {
        let circuit = Circuit::ghz(5).unwrap();
        assert_eq!(circuit.num_qubits(), 5);
        assert_eq!(circuit.num_clbits(), 5);
    }

    #[test]
    fn test_parameterized_gate() {
        let mut circuit = Circuit::with_size("test", 1, 0);
        circuit.rx(PI / 2.0, QubitId(0)).unwrap();
        circuit
            .ry(ParameterExpression::symbol("theta"), QubitId(0))
            .unwrap();

        assert_eq!(circuit.depth(), 2);
    }

    #[test]
    fn test_fluent_api() {
        let mut circuit = Circuit::with_size("test", 2, 2);
        circuit
            .h(QubitId(0))
            .unwrap()
            .cx(QubitId(0), QubitId(1))
            .unwrap()
            .measure(QubitId(0), ClbitId(0))
            .unwrap()
            .measure(QubitId(1), ClbitId(1))
            .unwrap();

        assert_eq!(circuit.depth(), 3); // H, CX, parallel measures
    }
}
