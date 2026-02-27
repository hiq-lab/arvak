"""Tests for arvak.optimize problem encodings.

All tests run offline (no QPU required).
"""

from __future__ import annotations

import numpy as np
import pytest

from arvak.optimize import (
    BinaryQubo,
    PCESolver,
    decode_tsp,
    qubo_from_maxcut,
    qubo_from_portfolio,
    qubo_from_tsp,
    tsp_tour_length,
)


# ===========================================================================
# qubo_from_maxcut
# ===========================================================================

class TestMaxCut:

    def test_two_node_graph(self):
        # Single edge (0,1) weight 1. MaxCut = 1, achieved by x=[0,1] or [1,0].
        qubo = qubo_from_maxcut({(0, 1): 1.0})
        assert qubo.n == 2
        assert qubo.evaluate([0, 1]) == pytest.approx(-1.0)
        assert qubo.evaluate([1, 0]) == pytest.approx(-1.0)
        assert qubo.evaluate([0, 0]) == pytest.approx(0.0)
        assert qubo.evaluate([1, 1]) == pytest.approx(0.0)

    def test_triangle_max_cut_is_2(self):
        # Triangle: MaxCut = 2 (any one vertex in S, two in S̄).
        edges = {(0, 1): 1.0, (1, 2): 1.0, (0, 2): 1.0}
        qubo = qubo_from_maxcut(edges)
        # Best solutions cut 2 edges → cost -2.
        best_cost = min(
            qubo.evaluate([x0, x1, x2])
            for x0 in (0, 1) for x1 in (0, 1) for x2 in (0, 1)
        )
        assert best_cost == pytest.approx(-2.0)

    def test_4node_cycle_max_cut(self):
        # 4-cycle: MaxCut = 4, achieved by alternating assignment.
        edges = {(0,1):1.0, (1,2):1.0, (2,3):1.0, (3,0):1.0}
        qubo = qubo_from_maxcut(edges)
        assert qubo.evaluate([0, 1, 0, 1]) == pytest.approx(-4.0)
        assert qubo.evaluate([1, 0, 1, 0]) == pytest.approx(-4.0)

    def test_weighted_edges(self):
        edges = {(0, 1): 3.0, (1, 2): 1.0}
        qubo = qubo_from_maxcut(edges)
        # Best: x=[1,0,0] or x=[0,1,0] cuts edge (0,1) only → -3
        # or x=[0,0,1] or x=[1,1,0] cuts edge (1,2) only → -1
        # or x=[1,0,1] cuts both → -4
        assert qubo.evaluate([1, 0, 1]) == pytest.approx(-4.0)

    def test_numpy_matrix_input(self):
        A = np.array([[0, 1, 0], [1, 0, 1], [0, 1, 0]], dtype=float)
        qubo = qubo_from_maxcut(A)
        assert qubo.n == 3

    def test_networkx_graph_input(self):
        nx = pytest.importorskip("networkx")
        G = nx.cycle_graph(4)
        qubo = qubo_from_maxcut(G)
        assert qubo.n == 4
        assert qubo.evaluate([0, 1, 0, 1]) == pytest.approx(-4.0)

    def test_qubo_n_matches_node_count(self):
        edges = {(0, 3): 1.0, (1, 2): 1.0}
        qubo = qubo_from_maxcut(edges)
        assert qubo.n == 4

    def test_empty_graph_raises(self):
        # Empty graph → n=0 → BinaryQubo rejects it.
        with pytest.raises(ValueError):
            qubo_from_maxcut({})

    def test_pce_finds_good_maxcut(self):
        """PCESolver should find the optimum on a small complete graph."""
        edges = {(i, j): 1.0 for i in range(4) for j in range(i+1, 4)}
        qubo = qubo_from_maxcut(edges)
        solver = PCESolver(qubo, encoding="dense", shots=512, max_iter=100, seed=0)
        result = solver.solve()
        # K4 MaxCut = 4, optimal QUBO cost = -4.
        assert result.cost <= -3.0  # find at least cut-3 (tolerance for heuristic)


# ===========================================================================
# qubo_from_tsp
# ===========================================================================

class TestTSP:

    @pytest.fixture
    def triangle_distances(self):
        return np.array([[0, 1, 2], [1, 0, 1], [2, 1, 0]], dtype=float)

    def test_variable_count(self, triangle_distances):
        qubo = qubo_from_tsp(triangle_distances)
        assert qubo.n == 9   # 3 cities × 3 time steps

    def test_4city_variable_count(self):
        D = np.ones((4, 4)) - np.eye(4)
        qubo = qubo_from_tsp(D)
        assert qubo.n == 16

    def test_non_square_raises(self):
        with pytest.raises(ValueError, match="square"):
            qubo_from_tsp(np.ones((3, 4)))

    def test_single_city_raises(self):
        with pytest.raises(ValueError, match="at least 2"):
            qubo_from_tsp(np.array([[0.0]]))

    def test_feasible_tour_is_low_cost(self, triangle_distances):
        qubo = qubo_from_tsp(triangle_distances, penalty=10.0)
        # Tour 0→1→2→0: x[0,0]=1, x[1,1]=1, x[2,2]=1 → all others 0.
        x = [0] * 9
        x[0 * 3 + 0] = 1   # city 0 at t=0
        x[1 * 3 + 1] = 1   # city 1 at t=1
        x[2 * 3 + 2] = 1   # city 2 at t=2
        cost_feasible = qubo.evaluate(x)

        # Infeasible: all zeros — both constraint terms fire for each city/time.
        x_infeasible = [0] * 9
        cost_infeasible = qubo.evaluate(x_infeasible)

        # Feasible tour should have lower QUBO cost than all-zeros.
        assert cost_feasible < cost_infeasible

    def test_custom_penalty(self, triangle_distances):
        qubo = qubo_from_tsp(triangle_distances, penalty=100.0)
        assert qubo.n == 9

    def test_default_penalty_is_set(self, triangle_distances):
        qubo1 = qubo_from_tsp(triangle_distances)
        qubo2 = qubo_from_tsp(triangle_distances, penalty=float(np.max(triangle_distances)) * 3)
        # Both should give the same QUBO structure.
        assert qubo1.n == qubo2.n


# ===========================================================================
# decode_tsp + tsp_tour_length
# ===========================================================================

class TestDecodeTSP:

    def test_valid_tour(self):
        # 3 cities: tour 0→1→2
        x = [0] * 9
        x[0 * 3 + 0] = 1   # city 0 at t=0
        x[1 * 3 + 1] = 1   # city 1 at t=1
        x[2 * 3 + 2] = 1   # city 2 at t=2
        tour = decode_tsp(x, 3)
        assert tour == [0, 1, 2]

    def test_alternative_valid_tour(self):
        x = [0] * 9
        x[2 * 3 + 0] = 1   # city 2 at t=0
        x[0 * 3 + 1] = 1   # city 0 at t=1
        x[1 * 3 + 2] = 1   # city 1 at t=2
        tour = decode_tsp(x, 3)
        assert tour == [2, 0, 1]

    def test_infeasible_two_cities_same_time(self):
        x = [0] * 9
        x[0 * 3 + 0] = 1
        x[1 * 3 + 0] = 1   # two cities at t=0
        x[2 * 3 + 2] = 1
        assert decode_tsp(x, 3) is None

    def test_infeasible_duplicate_city(self):
        x = [0] * 9
        x[0 * 3 + 0] = 1
        x[0 * 3 + 1] = 1   # city 0 twice
        x[1 * 3 + 2] = 1
        # t=1 has city 0; t=2 has city 1; city 2 never visited
        assert decode_tsp(x, 3) is None

    def test_wrong_length_raises(self):
        with pytest.raises(ValueError):
            decode_tsp([0, 1], 3)

    def test_tour_length(self):
        D = np.array([[0, 1, 2], [1, 0, 1], [2, 1, 0]], dtype=float)
        tour = [0, 1, 2]
        # 0→1: 1, 1→2: 1, 2→0: 2 = 4
        assert tsp_tour_length(tour, D) == pytest.approx(4.0)

    def test_tour_length_closed(self):
        D = np.array([[0, 1, 10], [1, 0, 1], [10, 1, 0]], dtype=float)
        # Shortest tour: 0→1→2→0 = 1+1+10=12, or 0→2→1→0 = 10+1+1=12
        tour = [0, 1, 2]
        assert tsp_tour_length(tour, D) == pytest.approx(12.0)


# ===========================================================================
# qubo_from_portfolio
# ===========================================================================

class TestPortfolio:

    @pytest.fixture
    def simple_portfolio(self):
        r = np.array([0.10, 0.15, 0.08, 0.12])
        cov = np.diag([0.02, 0.04, 0.01, 0.03])
        return r, cov

    def test_variable_count(self, simple_portfolio):
        r, cov = simple_portfolio
        qubo = qubo_from_portfolio(r, cov)
        assert qubo.n == 4

    def test_mismatched_covariance_raises(self):
        r = np.array([0.1, 0.2, 0.3])
        cov = np.eye(2)
        with pytest.raises(ValueError, match="covariance"):
            qubo_from_portfolio(r, cov)

    def test_high_return_asset_preferred_no_risk_penalty(self):
        # Two assets: high return / high risk vs low return / no risk.
        # With risk_factor=0 (pure return maximisation), pick both.
        r = np.array([0.20, 0.05])
        cov = np.diag([0.10, 0.001])
        qubo = qubo_from_portfolio(r, cov, risk_factor=0.0)
        # Minimum cost = selecting both assets (most return)
        assert qubo.evaluate([1, 1]) < qubo.evaluate([0, 0])
        assert qubo.evaluate([1, 1]) < qubo.evaluate([0, 1])

    def test_risk_factor_increases_cost_of_risky_asset(self):
        r = np.array([0.20, 0.20])          # equal returns
        cov = np.diag([0.10, 0.001])        # asset 0 very risky
        qubo_low_risk = qubo_from_portfolio(r, cov, risk_factor=10.0)
        # With high risk penalty, asset 0 should be penalised.
        assert qubo_low_risk.evaluate([0, 1]) < qubo_low_risk.evaluate([1, 0])

    def test_budget_constraint_penalises_wrong_count(self, simple_portfolio):
        r, cov = simple_portfolio
        qubo = qubo_from_portfolio(r, cov, risk_factor=1.0, budget=2, budget_penalty=100.0)
        # Selecting exactly 2 assets should be cheaper than selecting 0 or 4.
        cost_0 = qubo.evaluate([0, 0, 0, 0])
        cost_2 = qubo.evaluate([0, 1, 1, 0])
        cost_4 = qubo.evaluate([1, 1, 1, 1])
        assert cost_2 < cost_0
        assert cost_2 < cost_4

    def test_returns_binary_qubo(self, simple_portfolio):
        r, cov = simple_portfolio
        qubo = qubo_from_portfolio(r, cov)
        assert isinstance(qubo, BinaryQubo)

    def test_pce_finds_best_portfolio(self):
        """PCESolver should prefer the high-return, low-risk asset."""
        r = np.array([0.01, 0.20, 0.01, 0.01])
        cov = np.diag([0.001, 0.001, 0.001, 0.001])
        qubo = qubo_from_portfolio(r, cov, risk_factor=0.5)
        solver = PCESolver(qubo, encoding="dense", shots=512, max_iter=100, seed=0)
        result = solver.solve()
        # Asset 1 (highest return) should appear in the top solution.
        assert result.solution[1] is True or any(
            sol[1] for sol, _ in result.top_solutions[:3]
        )

    def test_full_workflow_maxcut(self):
        """End-to-end: qubo_from_maxcut → PCESolver → solution."""
        edges = {(0, 1): 1.0, (1, 2): 1.0, (2, 0): 1.0}
        qubo = qubo_from_maxcut(edges)
        solver = PCESolver(qubo, encoding="dense", shots=256, max_iter=50, seed=7)
        result = solver.solve()
        assert result.cost <= 0.0
        assert len(result.solution) == 3

    def test_full_workflow_tsp(self):
        """End-to-end: qubo_from_tsp → PCESolver → decode."""
        D = np.array([[0,1,2],[1,0,1],[2,1,0]], dtype=float)
        qubo = qubo_from_tsp(D, penalty=10.0)
        solver = PCESolver(qubo, encoding="dense", shots=512, max_iter=100, seed=3)
        result = solver.solve()
        # Not guaranteed feasible with few iterations, but solution should exist.
        assert len(result.solution) == 9
        tour = decode_tsp(result.solution, 3)
        # If feasible, verify it's a valid permutation.
        if tour is not None:
            assert sorted(tour) == [0, 1, 2]
