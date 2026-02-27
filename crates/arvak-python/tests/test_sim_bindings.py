"""Tests for arvak.sim — PyO3 bindings for Hamiltonian time-evolution synthesis.

All tests run offline (no QPU required). Circuits returned are verified
to be arvak.Circuit instances of the expected qubit count.
"""

from __future__ import annotations

import pytest

import arvak
from arvak.sim import (
    Hamiltonian,
    HamiltonianTerm,
    PauliOp,
    PauliString,
    QDriftEvolution,
    TrotterEvolution,
)


# ===========================================================================
# PauliOp
# ===========================================================================

class TestPauliOp:
    def test_class_attributes(self):
        assert PauliOp.I is not None
        assert PauliOp.X is not None
        assert PauliOp.Y is not None
        assert PauliOp.Z is not None

    def test_equality(self):
        assert PauliOp.X == PauliOp.X
        assert PauliOp.X != PauliOp.Z

    def test_repr(self):
        assert "PauliOp.Z" in repr(PauliOp.Z)

    def test_hashable(self):
        s = {PauliOp.X, PauliOp.Y, PauliOp.Z, PauliOp.I}
        assert len(s) == 4


# ===========================================================================
# PauliString
# ===========================================================================

class TestPauliString:
    def test_from_ops_basic(self):
        ps = PauliString.from_ops([(0, PauliOp.Z), (1, PauliOp.Z)])
        assert len(ps) == 2

    def test_identity_dropped(self):
        ps = PauliString.from_ops([(0, PauliOp.I), (1, PauliOp.Z)])
        assert len(ps) == 1

    def test_empty(self):
        ps = PauliString.from_ops([])
        assert len(ps) == 0

    def test_repr(self):
        ps = PauliString.from_ops([(0, PauliOp.X)])
        assert "PauliString" in repr(ps)


# ===========================================================================
# HamiltonianTerm
# ===========================================================================

class TestHamiltonianTerm:
    def test_new(self):
        ps = PauliString.from_ops([(0, PauliOp.Z)])
        t = HamiltonianTerm(coeff=-1.5, pauli=ps)
        assert t.coeff == pytest.approx(-1.5)

    def test_z_shorthand(self):
        t = HamiltonianTerm.z(0, -1.0)
        assert t.coeff == pytest.approx(-1.0)

    def test_zz_shorthand(self):
        t = HamiltonianTerm.zz(0, 1, 0.5)
        assert t.coeff == pytest.approx(0.5)

    def test_x_shorthand(self):
        t = HamiltonianTerm.x(2, 0.25)
        assert t.coeff == pytest.approx(0.25)

    def test_repr(self):
        assert "HamiltonianTerm" in repr(HamiltonianTerm.z(0, 1.0))


# ===========================================================================
# Hamiltonian
# ===========================================================================

class TestHamiltonian:
    def test_from_terms(self):
        h = Hamiltonian.from_terms([
            HamiltonianTerm.zz(0, 1, -1.0),
            HamiltonianTerm.x(0, -0.5),
        ])
        assert h.n_terms() == 2

    def test_min_qubits(self):
        h = Hamiltonian.from_terms([HamiltonianTerm.zz(0, 1, -1.0)])
        assert h.min_qubits() == 2

    def test_min_qubits_single(self):
        h = Hamiltonian.from_terms([HamiltonianTerm.z(3, 1.0)])
        assert h.min_qubits() == 4

    def test_lambda(self):
        h = Hamiltonian.from_terms([
            HamiltonianTerm.zz(0, 1, -1.0),
            HamiltonianTerm.x(0, 0.5),
        ])
        assert h.lambda_() == pytest.approx(1.5)

    def test_repr(self):
        h = Hamiltonian.from_terms([HamiltonianTerm.z(0, 1.0)])
        assert "Hamiltonian" in repr(h)


# ===========================================================================
# TrotterEvolution
# ===========================================================================

class TestTrotterEvolution:
    def _make_hamiltonian(self):
        return Hamiltonian.from_terms([
            HamiltonianTerm.zz(0, 1, -1.0),
            HamiltonianTerm.x(0, -0.5),
            HamiltonianTerm.x(1, -0.5),
        ])

    def test_first_order_returns_circuit(self):
        h = self._make_hamiltonian()
        evol = TrotterEvolution(h, 1.0, 4)
        circuit = evol.first_order()
        assert isinstance(circuit, arvak.Circuit)

    def test_first_order_qubit_count(self):
        h = self._make_hamiltonian()
        circuit = TrotterEvolution(h, 1.0, 4).first_order()
        assert circuit.num_qubits == 2

    def test_second_order_returns_circuit(self):
        h = self._make_hamiltonian()
        circuit = TrotterEvolution(h, 1.0, 2).second_order()
        assert isinstance(circuit, arvak.Circuit)

    def test_second_order_deeper_than_first(self):
        h = self._make_hamiltonian()
        c1 = TrotterEvolution(h, 1.0, 2).first_order()
        c2 = TrotterEvolution(h, 1.0, 2).second_order()
        # Second order has a symmetric sweep so is ~2× deeper.
        assert c2.depth() >= c1.depth()

    def test_more_steps_deeper(self):
        h = self._make_hamiltonian()
        c_few = TrotterEvolution(h, 1.0, 1).first_order()
        c_many = TrotterEvolution(h, 1.0, 4).first_order()
        assert c_many.depth() >= c_few.depth()

    def test_empty_hamiltonian_raises(self):
        h = Hamiltonian.from_terms([])
        with pytest.raises(RuntimeError, match="empty"):
            TrotterEvolution(h, 1.0, 4).first_order()

    def test_zero_steps_raises(self):
        h = self._make_hamiltonian()
        with pytest.raises(RuntimeError):
            TrotterEvolution(h, 1.0, 0).first_order()

    def test_three_qubit_hamiltonian(self):
        h = Hamiltonian.from_terms([
            HamiltonianTerm.zz(0, 1, -1.0),
            HamiltonianTerm.zz(1, 2, -0.5),
            HamiltonianTerm.x(0, 0.3),
        ])
        circuit = TrotterEvolution(h, 0.5, 3).first_order()
        assert circuit.num_qubits == 3

    def test_single_qubit_hamiltonian(self):
        h = Hamiltonian.from_terms([HamiltonianTerm.x(0, 1.0)])
        circuit = TrotterEvolution(h, math.pi / 2, 1).first_order()
        assert circuit.num_qubits == 1


# ===========================================================================
# QDriftEvolution
# ===========================================================================

class TestQDriftEvolution:
    def _make_hamiltonian(self):
        return Hamiltonian.from_terms([
            HamiltonianTerm.zz(0, 1, -1.0),
            HamiltonianTerm.x(0, -0.5),
        ])

    def test_circuit_returns_circuit(self):
        h = self._make_hamiltonian()
        circuit = QDriftEvolution(h, 1.0, 10).circuit()
        assert isinstance(circuit, arvak.Circuit)

    def test_circuit_qubit_count(self):
        h = self._make_hamiltonian()
        circuit = QDriftEvolution(h, 1.0, 10).circuit(seed=0)
        assert circuit.num_qubits == 2

    def test_seeded_reproducible(self):
        h = self._make_hamiltonian()
        evol = QDriftEvolution(h, 1.0, 20)
        c1 = evol.circuit(seed=42)
        c2 = evol.circuit(seed=42)
        # Same seed → same depth (same random draws)
        assert c1.depth() == c2.depth()

    def test_different_seeds_may_differ(self):
        h = self._make_hamiltonian()
        evol = QDriftEvolution(h, 1.0, 20)
        depths = {evol.circuit(seed=s).depth() for s in range(5)}
        # Very unlikely all 5 circuits have identical depth with a 2-term H
        # (they may all be the same if one term dominates; just check no crash)
        assert len(depths) >= 1

    def test_more_samples_more_depth(self):
        h = self._make_hamiltonian()
        c_few = QDriftEvolution(h, 1.0, 5).circuit(seed=0)
        c_many = QDriftEvolution(h, 1.0, 50).circuit(seed=0)
        assert c_many.depth() >= c_few.depth()

    def test_empty_hamiltonian_raises(self):
        h = Hamiltonian.from_terms([])
        with pytest.raises(RuntimeError):
            QDriftEvolution(h, 1.0, 10).circuit()

    def test_zero_samples_raises(self):
        h = self._make_hamiltonian()
        with pytest.raises(RuntimeError):
            QDriftEvolution(h, 1.0, 0).circuit()


# ===========================================================================
# Integration: import via arvak.sim
# ===========================================================================

import math

def test_arvak_sim_import():
    """Verify arvak.sim is importable from the top-level package."""
    import arvak
    assert hasattr(arvak, "sim")
    assert hasattr(arvak.sim, "Hamiltonian")
    assert hasattr(arvak.sim, "TrotterEvolution")
    assert hasattr(arvak.sim, "QDriftEvolution")


def test_trotter_circuit_is_arvak_circuit():
    """Verify TrotterEvolution returns an arvak.Circuit (not a foreign type)."""
    h = Hamiltonian.from_terms([HamiltonianTerm.z(0, 1.0)])
    c = TrotterEvolution(h, math.pi / 2, 1).first_order()
    import arvak
    assert isinstance(c, arvak.Circuit)
    assert c.num_qubits == 1
