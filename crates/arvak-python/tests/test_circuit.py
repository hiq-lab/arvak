"""Tests for the hiq Circuit class."""

import pytest

import arvak
from arvak import Circuit, QubitId, ClbitId


class TestCircuitBasics:
    """Basic circuit creation and properties."""

    def test_create_empty_circuit(self):
        """Test creating an empty circuit."""
        qc = Circuit("test")
        assert qc.name == "test"
        assert qc.num_qubits == 0
        assert qc.num_clbits == 0
        assert qc.depth() == 0

    def test_create_circuit_with_size(self):
        """Test creating a circuit with initial size."""
        qc = Circuit("test", num_qubits=3, num_clbits=2)
        assert qc.num_qubits == 3
        assert qc.num_clbits == 2

    def test_add_qubits(self):
        """Test adding qubits to a circuit."""
        qc = Circuit("test")
        q0 = qc.add_qubit()
        q1 = qc.add_qubit()
        assert qc.num_qubits == 2
        assert q0.index == 0
        assert q1.index == 1

    def test_add_quantum_register(self):
        """Test adding a quantum register."""
        qc = Circuit("test")
        qreg = qc.add_qreg("q", 4)
        assert len(qreg) == 4
        assert qc.num_qubits == 4

    def test_add_classical_register(self):
        """Test adding a classical register."""
        qc = Circuit("test")
        creg = qc.add_creg("c", 3)
        assert len(creg) == 3
        assert qc.num_clbits == 3


class TestGates:
    """Test gate application."""

    def test_hadamard_gate(self):
        """Test applying Hadamard gate."""
        qc = Circuit("test", num_qubits=1)
        qc.h(0)
        assert qc.depth() == 1

    def test_pauli_gates(self):
        """Test Pauli gates."""
        qc = Circuit("test", num_qubits=1)
        qc.x(0).y(0).z(0)
        assert qc.depth() == 3

    def test_rotation_gates(self):
        """Test rotation gates."""
        import math
        qc = Circuit("test", num_qubits=1)
        qc.rx(math.pi / 2, 0).ry(math.pi / 4, 0).rz(math.pi, 0)
        assert qc.depth() == 3

    def test_cnot_gate(self):
        """Test CNOT gate."""
        qc = Circuit("test", num_qubits=2)
        qc.cx(0, 1)
        assert qc.depth() == 1

    def test_two_qubit_gates(self):
        """Test various two-qubit gates."""
        qc = Circuit("test", num_qubits=2)
        qc.cx(0, 1).cy(0, 1).cz(0, 1).swap(0, 1)
        assert qc.depth() == 4

    def test_three_qubit_gates(self):
        """Test three-qubit gates."""
        qc = Circuit("test", num_qubits=3)
        qc.ccx(0, 1, 2).cswap(0, 1, 2)
        assert qc.depth() == 2

    def test_fluent_api(self):
        """Test fluent API chaining."""
        qc = Circuit("test", num_qubits=2, num_clbits=2)
        qc.h(0).cx(0, 1).measure(0, 0).measure(1, 1)
        assert qc.depth() == 3  # H, CX, parallel measures


class TestQubitId:
    """Test QubitId class."""

    def test_create_qubit_id(self):
        """Test creating a QubitId."""
        q = QubitId(5)
        assert q.index == 5

    def test_qubit_id_str(self):
        """Test QubitId string representation."""
        q = QubitId(3)
        assert str(q) == "q3"

    def test_qubit_id_repr(self):
        """Test QubitId repr."""
        q = QubitId(2)
        assert repr(q) == "QubitId(2)"

    def test_qubit_id_equality(self):
        """Test QubitId equality."""
        q1 = QubitId(1)
        q2 = QubitId(1)
        q3 = QubitId(2)
        assert q1 == q2
        assert q1 != q3

    def test_qubit_id_as_int(self):
        """Test QubitId to int conversion."""
        q = QubitId(7)
        assert int(q) == 7


class TestClbitId:
    """Test ClbitId class."""

    def test_create_clbit_id(self):
        """Test creating a ClbitId."""
        c = ClbitId(3)
        assert c.index == 3

    def test_clbit_id_str(self):
        """Test ClbitId string representation."""
        c = ClbitId(2)
        assert str(c) == "c2"


class TestPrebuiltCircuits:
    """Test pre-built circuit factories."""

    def test_bell_state(self):
        """Test Bell state circuit."""
        qc = Circuit.bell()
        assert qc.num_qubits == 2
        assert qc.num_clbits == 2

    def test_ghz_state(self):
        """Test GHZ state circuit."""
        qc = Circuit.ghz(5)
        assert qc.num_qubits == 5
        assert qc.num_clbits == 5

    def test_qft_circuit(self):
        """Test QFT circuit."""
        qc = Circuit.qft(4)
        assert qc.num_qubits == 4
        assert qc.num_clbits == 0  # QFT doesn't add measurements


class TestMeasurements:
    """Test measurement operations."""

    def test_single_measure(self):
        """Test measuring a single qubit."""
        qc = Circuit("test", num_qubits=1, num_clbits=1)
        qc.measure(0, 0)
        assert qc.depth() == 1

    def test_measure_all(self):
        """Test measuring all qubits."""
        qc = Circuit("test", num_qubits=3)
        qc.h(0).cx(0, 1).cx(1, 2)
        qc.measure_all()
        assert qc.num_clbits == 3  # Classical bits added automatically


class TestQASM:
    """Test QASM I/O."""

    def test_parse_simple_qasm(self):
        """Test parsing simple QASM."""
        qasm = """
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
"""
        qc = arvak.from_qasm(qasm)
        assert qc.num_qubits == 2

    def test_emit_qasm(self):
        """Test emitting QASM."""
        qc = Circuit("test", num_qubits=2)
        qc.h(0).cx(0, 1)
        qasm = arvak.to_qasm(qc)
        assert "OPENQASM 3.0" in qasm
        assert "h q[0]" in qasm
        assert "cx" in qasm

    def test_qasm_roundtrip(self):
        """Test QASM roundtrip."""
        original = """
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
"""
        qc = arvak.from_qasm(original)
        output = arvak.to_qasm(qc)
        qc2 = arvak.from_qasm(output)
        assert qc2.num_qubits == 2


class TestErrors:
    """Test error handling."""

    def test_invalid_qubit(self):
        """Test error on invalid qubit."""
        qc = Circuit("test", num_qubits=2)
        with pytest.raises(RuntimeError):
            qc.h(5)  # Qubit 5 doesn't exist

    def test_invalid_qasm(self):
        """Test error on invalid QASM."""
        with pytest.raises(RuntimeError):
            arvak.from_qasm("not valid qasm")


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
