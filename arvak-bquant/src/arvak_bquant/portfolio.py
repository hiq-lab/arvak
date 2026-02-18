"""Portfolio optimization — Markowitz mean-variance encoding as QUBO."""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np

from .qubo import QUBOProblem


@dataclass
class PortfolioSpec:
    """Specification of a portfolio optimization problem.

    Parameters
    ----------
    expected_returns : np.ndarray
        Expected return for each asset (length n).
    covariance_matrix : np.ndarray
        Covariance matrix of asset returns (n x n).
    risk_aversion : float
        Trade-off parameter lambda. Higher = more risk-averse.
    budget : int
        Number of assets to select (cardinality constraint).
    asset_names : list[str] | None
        Optional human-readable asset labels.
    """

    expected_returns: np.ndarray
    covariance_matrix: np.ndarray
    risk_aversion: float = 0.5
    budget: int = 1
    asset_names: list[str] = field(default_factory=list)

    @property
    def num_assets(self) -> int:
        return len(self.expected_returns)


def portfolio_to_qubo(
    spec: PortfolioSpec,
    penalty: float | None = None,
) -> QUBOProblem:
    """Encode the Markowitz portfolio problem as a QUBO.

    Objective (to minimize):
        risk_aversion * x^T Sigma x  -  mu^T x  +  penalty * (sum(x) - budget)^2

    Parameters
    ----------
    spec : PortfolioSpec
        Portfolio specification.
    penalty : float | None
        Penalty weight for the budget constraint. If None, auto-set to
        ``max(|mu|) + risk_aversion * max(|Sigma|)`` so the penalty
        dominates the objective.

    Returns
    -------
    QUBOProblem
        The encoded QUBO.
    """
    n = spec.num_assets
    mu = np.asarray(spec.expected_returns, dtype=np.float64)
    sigma = np.asarray(spec.covariance_matrix, dtype=np.float64)
    lam = spec.risk_aversion
    k = spec.budget

    if penalty is None:
        penalty = float(np.max(np.abs(mu)) + lam * np.max(np.abs(sigma))) + 1.0

    Q = np.zeros((n, n), dtype=np.float64)

    # Risk term: lam * Sigma
    Q += lam * sigma

    # Return term: -mu on the diagonal
    for i in range(n):
        Q[i, i] -= mu[i]

    # Budget penalty: penalty * (sum(x) - k)^2
    # Expand: penalty * (sum_i x_i)^2 - 2k * sum_i x_i + k^2)
    # Quadratic: penalty * x_i * x_j for all i,j
    # Linear (diagonal): -2 * penalty * k * x_i  (plus penalty * x_i^2 = penalty * x_i for binary)
    for i in range(n):
        Q[i, i] += penalty * (1 - 2 * k)
        for j in range(i + 1, n):
            Q[i, j] += 2 * penalty

    offset = penalty * k * k

    return QUBOProblem(Q=Q, offset=offset)
