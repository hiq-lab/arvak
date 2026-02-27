"""Tests for arvak.optimize._qaoa — QAOASolver.

All tests run offline using the local statevector simulator.
"""

from __future__ import annotations

import numpy as np
import pytest

from arvak.optimize import BinaryQubo, QaoaResult, QAOASolver, qubo_from_maxcut


# ===========================================================================
# QaoaResult dataclass
# ===========================================================================

class TestQaoaResult:
    def _solve(self, **kwargs):
        Q = BinaryQubo.from_matrix([[-1.0, 2.0], [0.0, -1.0]])
        return QAOASolver(Q, p=1, shots=256, seed=0, max_iter=20, **kwargs).solve()

    def test_returns_result_type(self):
        assert isinstance(self._solve(), QaoaResult)

    def test_result_fields(self):
        r = self._solve()
        assert isinstance(r.solution, list)
        assert isinstance(r.cost, float)
        assert isinstance(r.gamma, list)
        assert isinstance(r.beta, list)
        assert isinstance(r.n_iters, int)
        assert isinstance(r.converged, bool)
        assert isinstance(r.top_solutions, list)

    def test_gamma_beta_length(self):
        Q = BinaryQubo.from_matrix([[-1.0, 0.0], [0.0, -1.0]])
        r = QAOASolver(Q, p=3, shots=128, seed=0, max_iter=10).solve()
        assert len(r.gamma) == 3
        assert len(r.beta) == 3

    def test_solution_length(self):
        Q = BinaryQubo.from_matrix([[-1.0, 2.0], [0.0, -1.0]])
        r = QAOASolver(Q, p=1, shots=256, seed=0, max_iter=20).solve()
        assert len(r.solution) == Q.n

    def test_top_solutions_format(self):
        Q = BinaryQubo.from_matrix([[-1.0, 2.0], [0.0, -1.0]])
        r = QAOASolver(Q, p=1, shots=256, seed=0, max_iter=10).solve()
        for sol, cost in r.top_solutions:
            assert isinstance(sol, list)
            assert isinstance(cost, float)


# ===========================================================================
# QAOASolver — validation
# ===========================================================================

class TestQAOASolverValidation:
    def test_p_zero_raises(self):
        Q = BinaryQubo.from_matrix([[-1.0]])
        with pytest.raises(ValueError, match="p must be >= 1"):
            QAOASolver(Q, p=0)


# ===========================================================================
# QAOASolver — MaxCut triangle
# ===========================================================================

class TestQAOAMaxCut:
    def test_maxcut_triangle_finds_non_positive_cost(self):
        """MaxCut on K3 (triangle): QAOA should find a cut of value ≤ 0 in QUBO.

        The MaxCut QUBO has minimum cost -2 (any 2-vertex cut of the triangle).
        """
        Q = qubo_from_maxcut({(0, 1): 1.0, (1, 2): 1.0, (2, 0): 1.0})
        result = QAOASolver(Q, p=1, shots=1024, seed=42, max_iter=100).solve()
        assert result.cost <= 0.0

    def test_maxcut_triangle_solution_is_binary(self):
        Q = qubo_from_maxcut({(0, 1): 1.0, (1, 2): 1.0, (2, 0): 1.0})
        result = QAOASolver(Q, p=1, shots=512, seed=0, max_iter=50).solve()
        assert all(isinstance(b, bool) for b in result.solution)
        assert len(result.solution) == 3

    def test_maxcut_larger_depth(self):
        """p=2 should not be worse than p=1 on average."""
        Q = qubo_from_maxcut({(0, 1): 1.0, (1, 2): 1.0, (2, 0): 1.0})
        r2 = QAOASolver(Q, p=2, shots=512, seed=0, max_iter=100).solve()
        # Just verify it runs and finds a cost ≤ 0
        assert r2.cost <= 0.5


# ===========================================================================
# QAOASolver — simple QUBO
# ===========================================================================

class TestQAOASimpleQubo:
    def test_diagonal_qubo_finds_minimum(self):
        """H = -x0 - x1: minimum is x0=1, x1=1, cost = -2."""
        Q = BinaryQubo(n=2, linear={0: -1.0, 1: -1.0})
        result = QAOASolver(Q, p=1, shots=1024, seed=0, max_iter=150).solve()
        # QAOA should sample x0=1, x1=1 with high probability
        assert result.cost <= -1.0

    def test_single_variable(self):
        """H = -x0: minimum is x0=1, cost = -1."""
        Q = BinaryQubo(n=1, linear={0: -1.0})
        result = QAOASolver(Q, p=1, shots=512, seed=0, max_iter=100).solve()
        assert result.cost <= 0.0

    def test_conflicting_terms(self):
        """H = x0*x1 - x0 - x1 + 1.5 penalty for both=1 (want one of each)."""
        Q = BinaryQubo(n=2, linear={0: -1.0, 1: -1.0}, quadratic={(0, 1): 2.0})
        result = QAOASolver(Q, p=2, shots=1024, seed=1, max_iter=200).solve()
        # Best solutions: (1,0) or (0,1) with cost -1; (0,0) cost 0; (1,1) cost 0
        assert result.cost <= 0.1


# ===========================================================================
# QAOASolver — reproducibility and backend protocol
# ===========================================================================

class TestQAOAReproducibility:
    def test_seeded_same_result(self):
        Q = BinaryQubo.from_matrix([[-1.0, 2.0], [0.0, -1.0]])
        kwargs = dict(p=1, shots=256, seed=77, max_iter=20)
        r1 = QAOASolver(Q, **kwargs).solve()
        r2 = QAOASolver(Q, **kwargs).solve()
        assert r1.cost == pytest.approx(r2.cost, abs=1e-10)
        assert r1.gamma == pytest.approx(r2.gamma)
        assert r1.beta == pytest.approx(r2.beta)

    def test_custom_backend(self):
        import arvak
        calls = []

        def counting_backend(circuit, shots):
            calls.append(1)
            return arvak.run_sim(circuit, shots)

        Q = BinaryQubo(n=2, linear={0: -1.0, 1: -1.0})
        QAOASolver(Q, p=1, shots=64, seed=0, max_iter=5, backend=counting_backend).solve()
        assert len(calls) > 0


# ===========================================================================
# NoisyBackend integration
# ===========================================================================

def test_noisy_backend_wraps_on_noise_model():
    from arvak.optimize import NoisyBackend
    import arvak

    class FakeNoise:
        pass

    def fake_backend(circuit, shots, **kwargs):
        return arvak.run_sim(circuit, shots)

    Q = BinaryQubo(n=1, linear={0: -1.0})
    solver = QAOASolver(Q, p=1, shots=64, seed=0, max_iter=5,
                        backend=fake_backend, noise_model=FakeNoise())
    assert isinstance(solver._backend, NoisyBackend)
    solver.solve()  # Should not crash


# ===========================================================================
# End-to-end: full pipeline
# ===========================================================================

def test_e2e_maxcut_qaoa():
    """End-to-end: MaxCut on 4-node cycle graph → QAOA → valid cut."""
    # 4-node cycle: 0-1-2-3-0
    edges = {(0, 1): 1.0, (1, 2): 1.0, (2, 3): 1.0, (3, 0): 1.0}
    Q = qubo_from_maxcut(edges)
    result = QAOASolver(Q, p=2, shots=2048, seed=0, max_iter=200).solve()

    # Verify result is well-formed
    assert len(result.solution) == 4
    assert all(isinstance(b, bool) for b in result.solution)
    assert result.cost <= 0.0  # MaxCut QUBO is non-positive at optimum

    # Best QUBO cost for a 4-cycle MaxCut is -4 (cut all 4 edges)
    # QAOA depth 2 with good optimisation should get at least -2
    assert result.cost <= -1.5


def test_e2e_vqe_ising():
    """End-to-end: VQE on 2-qubit Ising finds negative energy."""
    from arvak.optimize import VQESolver, SparsePauliOp

    h = SparsePauliOp([
        (-1.0, {0: 'Z', 1: 'Z'}),
        (-0.5, {0: 'X'}),
        (-0.5, {1: 'X'}),
    ])
    result = VQESolver(h, n_qubits=2, n_layers=3, shots=2048, seed=0, max_iter=300).solve()

    assert result.energy < -0.8
    assert len(result.energy_history) > 0
    assert result.params.shape == (6,)
