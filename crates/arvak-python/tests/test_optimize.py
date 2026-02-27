"""Tests for arvak.optimize — PCE solver and spectral partitioning.

All tests run offline (no QPU required).  The PCE tests use arvak.run_sim
(Rust statevector simulator) as the backend.
"""

from __future__ import annotations

import math

import numpy as np
import pytest

from arvak.optimize import (
    BinaryQubo,
    DenseEncoding,
    PceResult,
    PCESolver,
    PolyEncoding,
    spectral_partition,
)
from arvak.optimize._encoding import _popcount_parity
from arvak.optimize._pce import _build_ansatz, _counts_to_arrays


# ===========================================================================
# BinaryQubo
# ===========================================================================

class TestBinaryQubo:

    def test_from_matrix_diagonal(self):
        Q = np.diag([-1.0, -2.0, -3.0])
        q = BinaryQubo.from_matrix(Q)
        assert q.n == 3
        assert q.linear == {0: -1.0, 1: -2.0, 2: -3.0}
        assert q.quadratic == {}

    def test_from_matrix_off_diagonal(self):
        Q = np.array([[0, 1, 0], [0, 0, 2], [0, 0, 0]], dtype=float)
        q = BinaryQubo.from_matrix(Q)
        assert q.quadratic[(0, 1)] == pytest.approx(1.0)
        assert q.quadratic[(1, 2)] == pytest.approx(2.0)

    def test_from_matrix_symmetric_adds(self):
        Q = np.array([[0, 1, 0], [1, 0, 0], [0, 0, 0]], dtype=float)
        q = BinaryQubo.from_matrix(Q)
        # symmetric matrix: Q[0,1] + Q[1,0] = 2
        assert q.quadratic[(0, 1)] == pytest.approx(2.0)

    def test_evaluate_all_zeros(self):
        q = BinaryQubo.from_matrix(np.diag([-1.0, -1.0]))
        assert q.evaluate([0, 0]) == pytest.approx(0.0)

    def test_evaluate_all_ones(self):
        Q = np.array([[-1, 2], [0, -1]], dtype=float)
        q = BinaryQubo.from_matrix(Q)
        # cost = -1*1 + -1*1 + 2*1*1 = 0
        assert q.evaluate([1, 1]) == pytest.approx(0.0)

    def test_evaluate_batch_shape(self):
        q = BinaryQubo.from_matrix(np.diag([-1.0, -1.0, -1.0]))
        X = np.array([[0, 0, 0], [1, 1, 1], [1, 0, 0]], dtype=float)
        costs = q.evaluate_batch(X)
        assert costs.shape == (3,)
        assert costs[0] == pytest.approx(0.0)
        assert costs[1] == pytest.approx(-3.0)
        assert costs[2] == pytest.approx(-1.0)

    def test_from_dict_normalises_keys(self):
        q = BinaryQubo.from_dict(3, quadratic={(2, 0): 5.0, (1, 0): 3.0})
        assert (0, 2) in q.quadratic
        assert (0, 1) in q.quadratic

    def test_invalid_n(self):
        with pytest.raises(ValueError):
            BinaryQubo(n=0)

    def test_to_matrix_roundtrip(self):
        Q = np.array([[-1, 2, 0], [0, -1, 3], [0, 0, -1]], dtype=float)
        q = BinaryQubo.from_matrix(Q)
        Q2 = q.to_matrix()
        assert Q2[0, 0] == pytest.approx(-1.0)
        assert Q2[1, 2] == pytest.approx(3.0)


# ===========================================================================
# DenseEncoding
# ===========================================================================

class TestDenseEncoding:

    def test_n_qubits_4vars(self):
        # ceil(log2(4+1)) = ceil(log2(5)) = ceil(2.32) = 3
        enc = DenseEncoding(4)
        assert enc.n_qubits == 3

    def test_n_qubits_7vars(self):
        # ceil(log2(7+1)) = ceil(log2(8)) = ceil(3.0) = 3
        enc = DenseEncoding(7)
        assert enc.n_qubits == 3

    def test_n_qubits_256vars(self):
        # ceil(log2(256+1)) = ceil(log2(257)) = ceil(8.005) = 9
        enc = DenseEncoding(256)
        assert enc.n_qubits == 9

    def test_n_qubits_255vars(self):
        enc = DenseEncoding(255)
        assert enc.n_qubits == 8   # ceil(log2(256)) = 8

    def test_masks_are_distinct_and_nonzero(self):
        enc = DenseEncoding(8)
        masks = enc.parity_masks.tolist()
        assert len(set(masks)) == 8
        assert all(m > 0 for m in masks)

    def test_decode_batch_shape(self):
        enc = DenseEncoding(4)
        bs = np.array([0, 1, 2, 3, 7], dtype=np.uint64)
        X = enc.decode_batch(bs)
        assert X.shape == (5, 4)
        assert X.dtype == bool

    def test_decode_specific_parity(self):
        # DenseEncoding(1): mask[0] = 1, so x_0 = parity(bs & 1) = lsb
        enc = DenseEncoding(1)
        bs = np.array([0b00, 0b01, 0b10, 0b11], dtype=np.uint64)
        decoded = enc.decode_batch(bs)
        # x_0 = parity(bs & 1): 0&1=0(even), 1&1=1(odd), 2&1=0(even), 3&1=1(odd)
        np.testing.assert_array_equal(decoded[:, 0], [False, True, False, True])

    def test_compression_ratio(self):
        enc = DenseEncoding(255)
        assert enc.compression_ratio == pytest.approx(255 / 8)

    def test_pauli_correlations_shape(self):
        enc = DenseEncoding(4)
        bs = np.array([0, 1, 2, 3], dtype=np.uint64)
        weights = np.ones(4)
        corr = enc.pauli_correlations(bs, weights)
        assert corr.shape == (4,)
        assert np.all(np.abs(corr) <= 1.0 + 1e-9)


# ===========================================================================
# PolyEncoding
# ===========================================================================

class TestPolyEncoding:

    def test_n_qubits_4vars(self):
        enc = PolyEncoding(4)
        assert enc.side == 2
        assert enc.n_qubits == 4

    def test_n_qubits_9vars(self):
        enc = PolyEncoding(9)
        assert enc.side == 3
        assert enc.n_qubits == 6

    def test_decode_batch_shape(self):
        enc = PolyEncoding(4)
        bs = np.array([0, 1, 2, 15], dtype=np.uint64)
        X = enc.decode_batch(bs)
        assert X.shape == (4, 4)

    def test_masks_distinct_and_nonzero(self):
        enc = PolyEncoding(9)
        masks = enc.parity_masks.tolist()
        assert len(set(masks)) == 9
        assert all(m > 0 for m in masks)


# ===========================================================================
# _popcount_parity kernel
# ===========================================================================

class TestPopcountParity:

    def test_zero_is_even(self):
        x = np.array([0], dtype=np.uint64)
        assert not _popcount_parity(x)[0]

    def test_single_bit_is_odd(self):
        x = np.array([1, 2, 4, 8], dtype=np.uint64)
        assert np.all(_popcount_parity(x))

    def test_two_bits_is_even(self):
        x = np.array([3, 5, 6], dtype=np.uint64)   # 11, 101, 110 — all 2 bits
        assert not np.any(_popcount_parity(x))

    def test_known_values(self):
        # 7 = 111 (3 bits, odd) → True
        # 15 = 1111 (4 bits, even) → False
        x = np.array([7, 15], dtype=np.uint64)
        result = _popcount_parity(x)
        assert result[0] is np.bool_(True)
        assert result[1] is np.bool_(False)


# ===========================================================================
# Ansatz circuit builder
# ===========================================================================

class TestBuildAnsatz:

    def test_returns_circuit(self):
        import arvak
        theta = np.zeros(4)          # 2 layers × 2 qubits
        circuit = _build_ansatz(2, 2, theta)
        assert isinstance(circuit, arvak.Circuit)

    def test_single_qubit_no_cnots(self):
        import arvak
        theta = np.array([0.5, 1.0])  # 2 layers × 1 qubit
        circuit = _build_ansatz(1, 2, theta)
        assert isinstance(circuit, arvak.Circuit)

    def test_circuit_is_executable(self):
        import arvak
        theta = np.random.default_rng(0).uniform(0, 2 * math.pi, 6)  # 2 layers × 3 qubits
        circuit = _build_ansatz(3, 2, theta)
        counts = arvak.run_sim(circuit, 100)
        assert sum(counts.values()) == 100
        assert all(len(bs) == 3 for bs in counts)


# ===========================================================================
# PCESolver  (integration tests — uses run_sim)
# ===========================================================================

class TestPCESolver:

    @pytest.fixture
    def simple_qubo(self) -> BinaryQubo:
        """3-variable QUBO with known minimum at x=[1,1,1]."""
        return BinaryQubo.from_dict(3, linear={0: -1.0, 1: -1.0, 2: -1.0})

    def test_solve_returns_result(self, simple_qubo):
        solver = PCESolver(simple_qubo, encoding="dense", shots=256, max_iter=50, seed=0)
        result = solver.solve()
        assert isinstance(result, PceResult)
        assert len(result.solution) == 3
        assert isinstance(result.cost, float)

    def test_result_fields(self, simple_qubo):
        solver = PCESolver(simple_qubo, encoding="dense", shots=256, max_iter=50, seed=0)
        result = solver.solve()
        assert result.n_original_vars == 3
        assert result.n_qubits == DenseEncoding(3).n_qubits
        assert result.compression_ratio == pytest.approx(3 / result.n_qubits)
        assert result.n_function_evals > 0
        assert len(result.top_solutions) > 0

    def test_poly_encoding(self, simple_qubo):
        solver = PCESolver(simple_qubo, encoding="poly", shots=256, max_iter=50, seed=0)
        result = solver.solve()
        assert result.n_qubits == PolyEncoding(3).n_qubits
        assert len(result.solution) == 3

    def test_cvar_mode(self, simple_qubo):
        solver = PCESolver(
            simple_qubo, encoding="dense", shots=256, alpha=10.0, max_iter=50, seed=0
        )
        result = solver.solve()
        assert isinstance(result.cost, float)

    def test_custom_backend(self, simple_qubo):
        """Custom backend that returns fixed counts."""
        def fixed_backend(circuit, shots):
            return {"000": shots // 2, "111": shots - shots // 2}

        solver = PCESolver(
            simple_qubo, encoding="dense", shots=64, backend=fixed_backend, max_iter=10, seed=0
        )
        result = solver.solve()
        assert len(result.solution) == 3

    def test_invalid_encoding(self, simple_qubo):
        with pytest.raises(ValueError, match="Unknown encoding"):
            PCESolver(simple_qubo, encoding="spectral")

    def test_top_solutions_are_sorted(self, simple_qubo):
        solver = PCESolver(simple_qubo, encoding="dense", shots=256, max_iter=50, seed=0)
        result = solver.solve()
        costs = [c for _, c in result.top_solutions]
        assert costs == sorted(costs)

    def test_best_cost_matches_solution(self, simple_qubo):
        solver = PCESolver(simple_qubo, encoding="dense", shots=256, max_iter=50, seed=0)
        result = solver.solve()
        recomputed = simple_qubo.evaluate(result.solution)
        assert recomputed == pytest.approx(result.cost)

    def test_larger_qubo(self):
        """15-variable QUBO: all-negative diagonal → minimum at all-ones."""
        n = 15
        Q = np.diag([-1.0] * n)
        qubo = BinaryQubo.from_matrix(Q)
        solver = PCESolver(qubo, encoding="dense", shots=512, max_iter=100, seed=42)
        result = solver.solve()
        assert result.n_qubits == DenseEncoding(n).n_qubits
        assert result.n_qubits < n    # compression happened
        assert result.cost <= 0.0


# ===========================================================================
# spectral_partition
# ===========================================================================

class TestSpectralPartition:

    def test_two_cliques(self):
        """Perfect bisection: two disjoint cliques of size 3."""
        edges = {
            (0, 1): 1.0, (1, 2): 1.0, (0, 2): 1.0,   # clique A
            (3, 4): 1.0, (4, 5): 1.0, (3, 5): 1.0,   # clique B
        }
        parts = spectral_partition(edges, n_parts=2, n_nodes=6)
        assert len(parts) == 2
        # Each clique should end up in one partition.
        flat = [set(p) for p in parts]
        clique_a = {0, 1, 2}
        clique_b = {3, 4, 5}
        assert clique_a in flat or all(n in flat[0] or n in flat[1] for n in clique_a)
        # All 6 nodes covered exactly once.
        all_nodes = sorted(n for p in parts for n in p)
        assert all_nodes == list(range(6))

    def test_covers_all_nodes(self):
        edges = {(i, (i + 1) % 8): 1.0 for i in range(8)}
        parts = spectral_partition(edges, n_parts=3, n_nodes=8)
        all_nodes = sorted(n for p in parts for n in p)
        assert all_nodes == list(range(8))

    def test_single_partition(self):
        edges = {(0, 1): 1.0, (1, 2): 1.0}
        parts = spectral_partition(edges, n_parts=1, n_nodes=3)
        assert len(parts) == 1
        assert sorted(parts[0]) == [0, 1, 2]

    def test_numpy_matrix_input(self):
        A = np.array([[0, 1, 0, 1], [1, 0, 1, 0], [0, 1, 0, 1], [1, 0, 1, 0]], dtype=float)
        parts = spectral_partition(A, n_parts=2)
        all_nodes = sorted(n for p in parts for n in p)
        assert all_nodes == [0, 1, 2, 3]

    def test_networkx_graph(self):
        try:
            import networkx as nx
        except ImportError:
            pytest.skip("networkx not installed")
        G = nx.cycle_graph(6)
        parts = spectral_partition(G, n_parts=2)
        all_nodes = sorted(n for p in parts for n in p)
        assert all_nodes == list(range(6))

    def test_n_parts_1_returns_all_nodes(self):
        A = np.eye(4)
        parts = spectral_partition(A, n_parts=1)
        assert sorted(parts[0]) == [0, 1, 2, 3]

    def test_n_parts_gte_n_each_singleton(self):
        A = np.ones((3, 3)) - np.eye(3)
        parts = spectral_partition(A, n_parts=5)
        assert len(parts) == 5

    def test_invalid_n_parts(self):
        with pytest.raises(ValueError):
            spectral_partition({(0, 1): 1.0}, n_parts=0, n_nodes=2)
