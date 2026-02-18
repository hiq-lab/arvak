"""Tests for portfolio result interpretation."""

import numpy as np

from arvak_bquant.portfolio import PortfolioSpec
from arvak_bquant.result import interpret_portfolio_result
from arvak_bquant.types import JobResult


class TestInterpretResult:
    def _make_spec(self) -> PortfolioSpec:
        return PortfolioSpec(
            expected_returns=np.array([0.05, 0.10, 0.03, 0.08]),
            covariance_matrix=np.eye(4) * 0.01,
            risk_aversion=0.5,
            budget=2,
            asset_names=["AAPL", "MSFT", "GOOG", "AMZN"],
        )

    def test_basic_interpretation(self):
        spec = self._make_spec()
        result = JobResult(
            job_id="test-123",
            counts={"0110": 500, "1001": 300, "1100": 200},
            shots=1000,
        )
        portfolio = interpret_portfolio_result(result, spec, top_k=3)

        assert portfolio.total_shots == 1000
        assert len(portfolio.solutions) == 3

        # Most probable solution
        best = portfolio.best
        assert best.bitstring == "0110"
        assert best.probability == 0.5
        # Bits: 0110 -> assets at index 1,2 selected
        assert best.selected_indices == [1, 2]
        assert best.selected_names == ["MSFT", "GOOG"]

    def test_expected_return_calculation(self):
        spec = self._make_spec()
        result = JobResult(
            job_id="test",
            counts={"1010": 100},
            shots=100,
        )
        portfolio = interpret_portfolio_result(result, spec)

        # 1010 -> assets 0, 2 selected
        sol = portfolio.best
        # mu = [0.05, 0.10, 0.03, 0.08], x = [1, 0, 1, 0]
        assert sol.expected_return == 0.05 + 0.03

    def test_risk_calculation(self):
        spec = self._make_spec()
        result = JobResult(
            job_id="test",
            counts={"1010": 100},
            shots=100,
        )
        portfolio = interpret_portfolio_result(result, spec)
        sol = portfolio.best
        # x = [1, 0, 1, 0], sigma = 0.01 * I
        # risk = x^T sigma x = 0.01 + 0.01 = 0.02
        assert abs(sol.risk - 0.02) < 1e-10

    def test_top_k_limits(self):
        spec = self._make_spec()
        result = JobResult(
            job_id="test",
            counts={f"{i:04b}": 10 for i in range(16)},
            shots=160,
        )
        portfolio = interpret_portfolio_result(result, spec, top_k=3)
        assert len(portfolio.solutions) == 3

    def test_selected_names_shortcut(self):
        spec = self._make_spec()
        result = JobResult(
            job_id="test",
            counts={"0101": 100},
            shots=100,
        )
        portfolio = interpret_portfolio_result(result, spec)
        assert portfolio.selected_names == ["MSFT", "AMZN"]

    def test_no_asset_names_uses_indices(self):
        spec = PortfolioSpec(
            expected_returns=np.array([0.05, 0.10]),
            covariance_matrix=np.eye(2) * 0.01,
            budget=1,
        )
        result = JobResult(
            job_id="test",
            counts={"01": 100},
            shots=100,
        )
        portfolio = interpret_portfolio_result(result, spec)
        assert portfolio.best.selected_names == ["1"]
