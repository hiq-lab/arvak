"""Interpret quantum measurement results as portfolio selections."""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np

from .portfolio import PortfolioSpec
from .types import JobResult


@dataclass
class PortfolioSolution:
    """A single portfolio selection found in measurement results."""

    bitstring: str
    selected_indices: list[int]
    selected_names: list[str]
    expected_return: float
    risk: float
    probability: float  # fraction of total shots


@dataclass
class PortfolioResult:
    """Ranked portfolio solutions from quantum measurement results."""

    solutions: list[PortfolioSolution]
    total_shots: int

    @property
    def best(self) -> PortfolioSolution:
        """Return the most probable valid solution."""
        return self.solutions[0]

    @property
    def selected_names(self) -> list[str]:
        """Asset names from the best solution."""
        return self.best.selected_names


def interpret_portfolio_result(
    result: JobResult,
    spec: PortfolioSpec,
    top_k: int = 5,
) -> PortfolioResult:
    """Map measurement counts back to portfolio selections.

    Parameters
    ----------
    result : JobResult
        Measurement counts from the quantum job.
    spec : PortfolioSpec
        Original portfolio specification (for evaluation).
    top_k : int
        Number of top solutions to return (by probability).

    Returns
    -------
    PortfolioResult
        Ranked portfolio selections.
    """
    mu = np.asarray(spec.expected_returns, dtype=np.float64)
    sigma = np.asarray(spec.covariance_matrix, dtype=np.float64)
    n = spec.num_assets
    total_shots = sum(result.counts.values())

    names = spec.asset_names if spec.asset_names else [str(i) for i in range(n)]

    solutions: list[PortfolioSolution] = []

    # Sort bitstrings by count (descending)
    sorted_counts = sorted(result.counts.items(), key=lambda kv: kv[1], reverse=True)

    for bitstring, count in sorted_counts:
        # Parse bitstring to binary vector (MSB = qubit 0)
        bits = bitstring.lstrip("0b")
        # Pad or truncate to n bits
        bits = bits.zfill(n)[-n:]

        x = np.array([int(b) for b in bits], dtype=np.float64)

        selected = [i for i, b in enumerate(bits) if b == "1"]
        selected_names = [names[i] for i in selected]

        # Evaluate portfolio metrics
        expected_return = float(mu @ x)
        risk = float(x @ sigma @ x)
        probability = count / total_shots if total_shots > 0 else 0.0

        solutions.append(
            PortfolioSolution(
                bitstring=bitstring,
                selected_indices=selected,
                selected_names=selected_names,
                expected_return=expected_return,
                risk=risk,
                probability=probability,
            )
        )

        if len(solutions) >= top_k:
            break

    return PortfolioResult(solutions=solutions, total_shots=total_shots)
