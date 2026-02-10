"""Tests for the arvak.run_sim() PyO3 binding.

These tests verify that the Rust statevector simulator is correctly
exposed to Python via the run_sim function.
"""

import pytest
import arvak


class TestRunSimBasic:
    """Basic tests for run_sim."""

    def test_run_sim_returns_dict(self):
        """Test that run_sim returns a dict of bitstring -> count."""
        circuit = arvak.Circuit.bell()
        result = arvak.run_sim(circuit, 100)

        assert isinstance(result, dict)
        assert len(result) > 0

    def test_run_sim_total_shots(self):
        """Test that total counts sum to requested shots."""
        circuit = arvak.Circuit.bell()
        shots = 1000
        result = arvak.run_sim(circuit, shots)

        total = sum(result.values())
        assert total == shots

    def test_run_sim_default_shots(self):
        """Test that default shots is 1024."""
        circuit = arvak.Circuit.bell()
        result = arvak.run_sim(circuit)

        total = sum(result.values())
        assert total == 1024

    def test_run_sim_zero_shots_raises(self):
        """Test that 0 shots raises ValueError."""
        circuit = arvak.Circuit.bell()
        with pytest.raises(ValueError, match="shots must be > 0"):
            arvak.run_sim(circuit, 0)

    def test_run_sim_bitstring_format(self):
        """Test that keys are bitstrings of correct length."""
        circuit = arvak.Circuit.bell()
        result = arvak.run_sim(circuit, 100)

        for bitstring in result.keys():
            assert isinstance(bitstring, str)
            assert all(c in '01' for c in bitstring)
            assert len(bitstring) == 2  # Bell state has 2 qubits


class TestRunSimBellState:
    """Test Bell state simulation results."""

    def test_bell_state_outcomes(self):
        """Bell state should only produce 00 and 11."""
        circuit = arvak.Circuit.bell()
        result = arvak.run_sim(circuit, 1000)

        # Only 00 and 11 should appear
        for bitstring in result.keys():
            assert bitstring in ('00', '11'), f"Unexpected outcome: {bitstring}"

    def test_bell_state_roughly_equal(self):
        """Bell state should have ~50/50 split."""
        circuit = arvak.Circuit.bell()
        result = arvak.run_sim(circuit, 10000)

        count_00 = result.get('00', 0)
        count_11 = result.get('11', 0)

        # Each should be roughly 5000 ± 500 (5σ tolerance)
        assert 3500 < count_00 < 6500, f"00 count out of range: {count_00}"
        assert 3500 < count_11 < 6500, f"11 count out of range: {count_11}"


class TestRunSimGHZ:
    """Test GHZ state simulation results."""

    def test_ghz3_outcomes(self):
        """GHZ-3 should only produce 000 and 111."""
        circuit = arvak.Circuit.ghz(3)
        result = arvak.run_sim(circuit, 1000)

        for bitstring in result.keys():
            assert bitstring in ('000', '111'), f"Unexpected outcome: {bitstring}"

    def test_ghz3_total_shots(self):
        """GHZ-3 total shots should match."""
        circuit = arvak.Circuit.ghz(3)
        shots = 2000
        result = arvak.run_sim(circuit, shots)

        assert sum(result.values()) == shots


class TestRunSimSingleQubit:
    """Test single-qubit circuits."""

    def test_hadamard_outcomes(self):
        """H|0⟩ should produce ~50/50 split of 0 and 1."""
        # Create single-qubit circuit with H gate
        qasm = """OPENQASM 3.0;
qubit[1] q;
bit[1] c;
h q[0];
c[0] = measure q[0];"""
        circuit = arvak.from_qasm(qasm)
        result = arvak.run_sim(circuit, 10000)

        count_0 = result.get('0', 0)
        count_1 = result.get('1', 0)

        # Each should be roughly 5000 ± 500
        assert 3500 < count_0 < 6500, f"0 count out of range: {count_0}"
        assert 3500 < count_1 < 6500, f"1 count out of range: {count_1}"

    def test_x_gate_deterministic(self):
        """X|0⟩ = |1⟩, should always measure 1."""
        qasm = """OPENQASM 3.0;
qubit[1] q;
bit[1] c;
x q[0];
c[0] = measure q[0];"""
        circuit = arvak.from_qasm(qasm)
        result = arvak.run_sim(circuit, 100)

        assert result.get('1', 0) == 100
        assert result.get('0', 0) == 0

    def test_identity_deterministic(self):
        """|0⟩ with no gates should always measure 0."""
        qasm = """OPENQASM 3.0;
qubit[1] q;
bit[1] c;
c[0] = measure q[0];"""
        circuit = arvak.from_qasm(qasm)
        result = arvak.run_sim(circuit, 100)

        assert result.get('0', 0) == 100


class TestRunSimFromQASM:
    """Test run_sim with circuits built from QASM."""

    def test_qasm_bell_state(self):
        """Test Bell state built from QASM."""
        qasm = """OPENQASM 3.0;
qubit[2] q;
bit[2] c;
h q[0];
cx q[0], q[1];
c[0] = measure q[0];
c[1] = measure q[1];"""

        circuit = arvak.from_qasm(qasm)
        result = arvak.run_sim(circuit, 1000)

        assert sum(result.values()) == 1000
        for bitstring in result.keys():
            assert bitstring in ('00', '11')

    def test_qasm_multiple_gates(self):
        """Test circuit with multiple different gates."""
        qasm = """OPENQASM 3.0;
qubit[3] q;
bit[3] c;
h q[0];
cx q[0], q[1];
cx q[1], q[2];
c[0] = measure q[0];
c[1] = measure q[1];
c[2] = measure q[2];"""

        circuit = arvak.from_qasm(qasm)
        result = arvak.run_sim(circuit, 1000)

        assert sum(result.values()) == 1000
        # GHZ-3: only 000 and 111
        for bitstring in result.keys():
            assert bitstring in ('000', '111')


if __name__ == '__main__':
    pytest.main([__file__, '-v'])
