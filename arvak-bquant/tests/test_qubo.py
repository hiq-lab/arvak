"""Tests for QUBO / Ising conversion."""

import numpy as np
import pytest

from arvak_bquant.qubo import IsingProblem, QUBOProblem, qubo_to_ising


class TestQUBOProblem:
    def test_num_variables(self):
        Q = np.array([[1, 2], [0, 3]], dtype=np.float64)
        qubo = QUBOProblem(Q=Q)
        assert qubo.num_variables == 2

    def test_evaluate(self):
        Q = np.array([[1, 2], [0, 3]], dtype=np.float64)
        qubo = QUBOProblem(Q=Q, offset=5.0)
        x = np.array([1, 0])
        assert qubo.evaluate(x) == 1.0 + 5.0
        x = np.array([1, 1])
        assert qubo.evaluate(x) == 1 + 2 + 3 + 5.0

    def test_evaluate_zero(self):
        Q = np.array([[1, 2], [0, 3]], dtype=np.float64)
        qubo = QUBOProblem(Q=Q, offset=0.0)
        x = np.array([0, 0])
        assert qubo.evaluate(x) == 0.0


class TestIsingProblem:
    def test_evaluate(self):
        ising = IsingProblem(
            J={(0, 1): 1.0},
            h={0: 0.5, 1: -0.5},
            offset=2.0,
            num_qubits=2,
        )
        spins = np.array([1, -1])
        energy = ising.evaluate(spins)
        # J contribution: 1.0 * 1 * (-1) = -1.0
        # h contribution: 0.5 * 1 + (-0.5) * (-1) = 1.0
        # offset: 2.0
        assert energy == pytest.approx(2.0)


class TestQUBOToIsing:
    def test_roundtrip_energy(self):
        """Verify that the QUBO and Ising representations agree on all assignments."""
        Q = np.array([[1, 0.5], [0.5, -1]], dtype=np.float64)
        qubo = QUBOProblem(Q=Q)
        ising = qubo_to_ising(qubo)

        for x0 in [0, 1]:
            for x1 in [0, 1]:
                x = np.array([x0, x1], dtype=np.float64)
                qubo_energy = qubo.evaluate(x)

                # Map binary -> spin: s = 1 - 2*x
                s = 1 - 2 * x
                ising_energy = ising.evaluate(s)

                assert qubo_energy == pytest.approx(ising_energy, abs=1e-10), (
                    f"Mismatch for x={x}: QUBO={qubo_energy}, Ising={ising_energy}"
                )

    def test_3_variable(self):
        """Three-variable QUBO roundtrip."""
        Q = np.array([
            [2, 1, 0],
            [1, -1, 0.5],
            [0, 0.5, 3],
        ], dtype=np.float64)
        qubo = QUBOProblem(Q=Q, offset=1.0)
        ising = qubo_to_ising(qubo)

        for bits in range(8):
            x = np.array([(bits >> i) & 1 for i in range(3)], dtype=np.float64)
            s = 1 - 2 * x
            assert qubo.evaluate(x) == pytest.approx(ising.evaluate(s), abs=1e-10)
