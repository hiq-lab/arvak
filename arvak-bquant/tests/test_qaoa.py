"""Tests for QAOA circuit generation."""

import numpy as np
import pytest

from arvak_bquant.qaoa import qaoa_circuit_qasm3
from arvak_bquant.qubo import QUBOProblem


def _simple_qubo(n: int = 3) -> QUBOProblem:
    """Create a simple QUBO with off-diagonal couplings."""
    Q = np.zeros((n, n))
    for i in range(n - 1):
        Q[i, i + 1] = 1.0
    np.fill_diagonal(Q, -1.0)
    return QUBOProblem(Q=Q)


class TestQAOACircuit:
    def test_valid_qasm3_header(self):
        qubo = _simple_qubo(3)
        qasm = qaoa_circuit_qasm3(qubo, p=1)
        assert qasm.startswith("OPENQASM 3.0;")
        assert 'include "stdgates.inc";' in qasm
        assert "qubit[3] q;" in qasm
        assert "bit[3] c;" in qasm

    def test_contains_hadamard_init(self):
        qubo = _simple_qubo(3)
        qasm = qaoa_circuit_qasm3(qubo, p=1)
        for i in range(3):
            assert f"h q[{i}];" in qasm

    def test_contains_measurements(self):
        qubo = _simple_qubo(3)
        qasm = qaoa_circuit_qasm3(qubo, p=1)
        for i in range(3):
            assert f"c[{i}] = measure q[{i}];" in qasm

    def test_contains_cost_unitary(self):
        qubo = _simple_qubo(3)
        qasm = qaoa_circuit_qasm3(qubo, p=1)
        # Should contain CX gates for ZZ interactions
        assert "cx" in qasm
        assert "rz(" in qasm

    def test_contains_mixer(self):
        qubo = _simple_qubo(3)
        qasm = qaoa_circuit_qasm3(qubo, p=1)
        assert "rx(" in qasm

    def test_multi_layer(self):
        qubo = _simple_qubo(3)
        qasm = qaoa_circuit_qasm3(qubo, p=3)
        assert qasm.count("// Layer") == 3

    def test_explicit_parameters(self):
        qubo = _simple_qubo(2)
        gamma = [0.5]
        beta = [0.3]
        qasm = qaoa_circuit_qasm3(qubo, p=1, gamma=gamma, beta=beta)
        assert "rx(0.6)" in qasm  # 2 * 0.3

    def test_parameter_length_mismatch_raises(self):
        qubo = _simple_qubo(2)
        with pytest.raises(ValueError, match="length p=2"):
            qaoa_circuit_qasm3(qubo, p=2, gamma=[0.5], beta=[0.3, 0.2])

    def test_single_variable_qubo(self):
        Q = np.array([[1.0]])
        qubo = QUBOProblem(Q=Q)
        qasm = qaoa_circuit_qasm3(qubo, p=1)
        assert "qubit[1] q;" in qasm
        assert "h q[0];" in qasm
        assert "c[0] = measure q[0];" in qasm
