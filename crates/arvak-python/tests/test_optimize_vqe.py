"""Tests for arvak.optimize._vqe — VQESolver and SparsePauliOp.

All tests run offline using the local statevector simulator.
"""

from __future__ import annotations

import numpy as np
import pytest

from arvak.optimize import SparsePauliOp, VqeResult, VQESolver


# ===========================================================================
# SparsePauliOp
# ===========================================================================

class TestSparsePauliOp:
    def test_basic_construction(self):
        h = SparsePauliOp([(-1.0, {0: 'Z', 1: 'Z'})])
        assert len(h.terms) == 1

    def test_identity_dropped(self):
        h = SparsePauliOp([(1.0, {0: 'I', 1: 'Z'})])
        # Identity on qubit 0 is dropped; only Z on qubit 1 remains
        _, ops = h.terms[0]
        assert 0 not in ops
        assert 1 in ops

    def test_n_qubits(self):
        h = SparsePauliOp([(-1.0, {0: 'Z', 1: 'Z'})])
        assert h.n_qubits() == 2

    def test_n_qubits_single(self):
        h = SparsePauliOp([(0.5, {3: 'X'})])
        assert h.n_qubits() == 4

    def test_n_qubits_empty(self):
        h = SparsePauliOp([])
        assert h.n_qubits() == 0

    def test_uppercase_normalisation(self):
        h = SparsePauliOp([(1.0, {0: 'z'})])
        _, ops = h.terms[0]
        assert ops[0] == 'Z'

    def test_repr(self):
        h = SparsePauliOp([(-1.0, {0: 'Z'})])
        assert "SparsePauliOp" in repr(h)

    def test_multiple_terms(self):
        h = SparsePauliOp([
            (-1.0, {0: 'Z', 1: 'Z'}),
            (0.5, {0: 'X'}),
            (0.5, {1: 'X'}),
        ])
        assert len(h.terms) == 3


# ===========================================================================
# VQESolver — basic functionality
# ===========================================================================

class TestVQESolver:
    def test_returns_vqe_result(self):
        h = SparsePauliOp([(-1.0, {0: 'Z', 1: 'Z'})])
        solver = VQESolver(h, n_qubits=2, n_layers=1, shots=256, seed=0, max_iter=50)
        result = solver.solve()
        assert isinstance(result, VqeResult)

    def test_result_fields(self):
        h = SparsePauliOp([(-1.0, {0: 'Z', 1: 'Z'})])
        result = VQESolver(h, n_qubits=2, n_layers=1, shots=256, seed=0, max_iter=50).solve()
        assert isinstance(result.energy, float)
        assert isinstance(result.params, np.ndarray)
        assert isinstance(result.n_iters, int)
        assert isinstance(result.converged, bool)
        assert isinstance(result.energy_history, list)

    def test_history_nonempty(self):
        h = SparsePauliOp([(-1.0, {0: 'Z', 1: 'Z'})])
        result = VQESolver(h, n_qubits=2, n_layers=1, shots=256, seed=0, max_iter=30).solve()
        assert len(result.energy_history) > 0

    def test_params_shape(self):
        n_qubits, n_layers = 2, 2
        h = SparsePauliOp([(-1.0, {0: 'Z', 1: 'Z'})])
        result = VQESolver(h, n_qubits=n_qubits, n_layers=n_layers, shots=256, seed=0, max_iter=30).solve()
        assert result.params.shape == (n_layers * n_qubits,)


# ===========================================================================
# VQESolver — physics correctness
# ===========================================================================

class TestVQEPhysics:
    def test_zz_ground_state_negative(self):
        """H = -ZZ has ground state energy -1. VQE should find negative energy."""
        h = SparsePauliOp([(-1.0, {0: 'Z', 1: 'Z'})])
        result = VQESolver(
            h, n_qubits=2, n_layers=2, shots=1024, seed=0, max_iter=200
        ).solve()
        assert result.energy < 0.0

    def test_single_qubit_z(self):
        """H = -Z has ground state -1 (|0⟩ state). VQE energy should be ≤ 0."""
        h = SparsePauliOp([(-1.0, {0: 'Z'})])
        result = VQESolver(h, n_qubits=1, n_layers=1, shots=512, seed=42, max_iter=100).solve()
        assert result.energy <= 0.05  # Allow small statistical noise

    def test_single_qubit_x(self):
        """H = -X has ground state -1 (|+⟩ state). VQE with RY ansatz should find it."""
        h = SparsePauliOp([(-1.0, {0: 'X'})])
        result = VQESolver(h, n_qubits=1, n_layers=2, shots=1024, seed=7, max_iter=200).solve()
        assert result.energy < 0.0

    def test_ising_model_two_qubits(self):
        """Transverse-field Ising: H = -ZZ - 0.5*X0 - 0.5*X1. Ground state ≤ -1."""
        h = SparsePauliOp([
            (-1.0, {0: 'Z', 1: 'Z'}),
            (-0.5, {0: 'X'}),
            (-0.5, {1: 'X'}),
        ])
        result = VQESolver(
            h, n_qubits=2, n_layers=3, shots=1024, seed=0, max_iter=300
        ).solve()
        # Ground state energy ≈ -√2 ≈ -1.41 for equal coupling
        assert result.energy < -0.5


# ===========================================================================
# VQESolver — reproducibility and backend protocol
# ===========================================================================

class TestVQESolverReproducibility:
    def test_seeded_same_result(self):
        h = SparsePauliOp([(-1.0, {0: 'Z', 1: 'Z'})])
        kwargs = dict(n_qubits=2, n_layers=1, shots=256, seed=99, max_iter=20)
        r1 = VQESolver(h, **kwargs).solve()
        r2 = VQESolver(h, **kwargs).solve()
        assert r1.energy == pytest.approx(r2.energy, abs=1e-10)

    def test_custom_backend(self):
        """VQE should work with a custom backend callable."""
        import arvak
        calls = []

        def counting_backend(circuit, shots):
            calls.append(shots)
            return arvak.run_sim(circuit, shots)

        h = SparsePauliOp([(-1.0, {0: 'Z'})])
        VQESolver(h, n_qubits=1, n_layers=1, shots=64, seed=0,
                  max_iter=5, backend=counting_backend).solve()
        assert len(calls) > 0
        assert all(s == 64 for s in calls)


# ===========================================================================
# NoisyBackend wrapping
# ===========================================================================

def test_noisy_backend_wraps_on_noise_model():
    """VQESolver with noise_model= should wrap backend in NoisyBackend."""
    from arvak.optimize import NoisyBackend

    import arvak
    call_log = []

    def fake_backend(circuit, shots, **kwargs):
        call_log.append(kwargs)
        return arvak.run_sim(circuit, shots)

    class FakeNoise:
        pass

    h = SparsePauliOp([(-1.0, {0: 'Z'})])
    solver = VQESolver(
        h, n_qubits=1, n_layers=1, shots=64, seed=0, max_iter=5,
        backend=fake_backend, noise_model=FakeNoise()
    )
    assert isinstance(solver._backend, NoisyBackend)
    # Running the solver should not crash (NoisyBackend falls back when TypeError).
    solver.solve()
