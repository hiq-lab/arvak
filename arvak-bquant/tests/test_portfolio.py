"""Tests for portfolio-to-QUBO encoding."""

import numpy as np
import pytest

from arvak_bquant.portfolio import PortfolioSpec, portfolio_to_qubo


class TestPortfolioToQubo:
    def _make_spec(self, n: int = 3, budget: int = 1) -> PortfolioSpec:
        rng = np.random.default_rng(42)
        mu = rng.uniform(0.01, 0.10, n)
        cov = rng.uniform(0, 0.01, (n, n))
        cov = (cov + cov.T) / 2  # symmetrize
        np.fill_diagonal(cov, rng.uniform(0.01, 0.05, n))  # positive diagonal
        return PortfolioSpec(
            expected_returns=mu,
            covariance_matrix=cov,
            risk_aversion=0.5,
            budget=budget,
            asset_names=[f"A{i}" for i in range(n)],
        )

    def test_qubo_shape(self):
        spec = self._make_spec(5)
        qubo = portfolio_to_qubo(spec)
        assert qubo.Q.shape == (5, 5)

    def test_budget_constraint_penalizes_wrong_selections(self):
        """Selections violating the budget constraint should have higher energy."""
        spec = self._make_spec(4, budget=2)
        qubo = portfolio_to_qubo(spec)

        # Try all 2-asset selections vs. all 3-asset selections
        energies_valid = []
        energies_invalid = []
        for bits in range(16):
            x = np.array([(bits >> i) & 1 for i in range(4)], dtype=np.float64)
            energy = qubo.evaluate(x)
            if int(x.sum()) == 2:
                energies_valid.append(energy)
            elif int(x.sum()) == 3:
                energies_invalid.append(energy)

        # Best valid should beat best invalid (penalty dominates)
        assert min(energies_valid) < min(energies_invalid)

    def test_empty_portfolio_energy(self):
        """x = [0,0,...,0] should give offset + penalty * budget^2."""
        spec = self._make_spec(3, budget=2)
        penalty = float(
            np.max(np.abs(spec.expected_returns))
            + spec.risk_aversion * np.max(np.abs(spec.covariance_matrix))
        ) + 1.0
        qubo = portfolio_to_qubo(spec)
        x = np.zeros(3)
        energy = qubo.evaluate(x)
        assert energy == pytest.approx(penalty * 4, abs=1e-10)  # penalty * budget^2

    def test_custom_penalty(self):
        spec = self._make_spec(3, budget=1)
        qubo = portfolio_to_qubo(spec, penalty=100.0)
        # offset should be penalty * budget^2 = 100
        assert qubo.offset == pytest.approx(100.0)
