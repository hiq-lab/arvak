"""Binary QUBO type with numpy-backed sparse evaluation."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Sequence

import numpy as np


@dataclass
class BinaryQubo:
    """Sparse quadratic unconstrained binary optimisation problem.

    Represents: min  sum_i c_i x_i  +  sum_{i<j} c_ij x_i x_j
    where x_i in {0, 1}.

    Variables are 0-indexed integers in [0, n).
    """

    n: int
    linear: dict[int, float] = field(default_factory=dict)
    quadratic: dict[tuple[int, int], float] = field(default_factory=dict)

    def __post_init__(self) -> None:
        if self.n <= 0:
            raise ValueError(f"n must be positive, got {self.n}")
        for i in self.linear:
            if not (0 <= i < self.n):
                raise ValueError(f"linear key {i} out of range [0, {self.n})")
        for i, j in self.quadratic:
            if i >= j:
                raise ValueError(
                    f"quadratic key ({i},{j}) must satisfy i < j; "
                    "normalise with BinaryQubo.normalise()"
                )
            if not (0 <= i < self.n and 0 <= j < self.n):
                raise ValueError(f"quadratic key ({i},{j}) out of range")

    # ------------------------------------------------------------------
    # Construction helpers
    # ------------------------------------------------------------------

    @classmethod
    def from_matrix(cls, Q: np.ndarray) -> "BinaryQubo":
        """Build from an upper-triangular or symmetric n×n matrix."""
        Q = np.asarray(Q, dtype=float)
        if Q.ndim != 2 or Q.shape[0] != Q.shape[1]:
            raise ValueError("Q must be a square 2-D array")
        n = Q.shape[0]
        linear: dict[int, float] = {}
        quadratic: dict[tuple[int, int], float] = {}
        for i in range(n):
            v = Q[i, i]
            if v != 0.0:
                linear[i] = v
            for j in range(i + 1, n):
                w = Q[i, j] + Q[j, i]  # symmetrise
                if w != 0.0:
                    quadratic[(i, j)] = w
        return cls(n=n, linear=linear, quadratic=quadratic)

    @classmethod
    def from_dict(
        cls,
        n: int,
        linear: dict[int, float] | None = None,
        quadratic: dict[tuple[int, int], float] | None = None,
    ) -> "BinaryQubo":
        """Build from coefficient dicts (keys normalised to i < j for quadratic)."""
        raw_quad = quadratic or {}
        norm_quad: dict[tuple[int, int], float] = {}
        for (i, j), v in raw_quad.items():
            key = (min(i, j), max(i, j))
            norm_quad[key] = norm_quad.get(key, 0.0) + v
        return cls(n=n, linear=dict(linear or {}), quadratic=norm_quad)

    # ------------------------------------------------------------------
    # Evaluation
    # ------------------------------------------------------------------

    def evaluate(self, x: Sequence[bool] | np.ndarray) -> float:
        """Evaluate QUBO cost for assignment x in {0,1}^n."""
        x = np.asarray(x, dtype=float)
        if len(x) < self.n:
            raise ValueError(
                f"Assignment length {len(x)} is shorter than n={self.n}"
            )
        cost = sum(c * x[i] for i, c in self.linear.items())
        cost += sum(c * x[i] * x[j] for (i, j), c in self.quadratic.items())
        return float(cost)

    def evaluate_batch(self, X: np.ndarray) -> np.ndarray:
        """Vectorised evaluation for a batch of assignments.

        Args:
            X: (n_samples, n) bool/float array.

        Returns:
            (n_samples,) float64 cost array.
        """
        X = np.asarray(X, dtype=float)
        n_samples = X.shape[0]
        costs = np.zeros(n_samples, dtype=np.float64)

        for i, c in self.linear.items():
            costs += c * X[:, i]
        for (i, j), c in self.quadratic.items():
            costs += c * X[:, i] * X[:, j]

        return costs

    # ------------------------------------------------------------------
    # Utilities
    # ------------------------------------------------------------------

    def constant_offset(self) -> float:
        """Minimum possible shift from linear+quadratic signs (informational)."""
        return 0.0

    def to_matrix(self) -> np.ndarray:
        """Return upper-triangular Q matrix."""
        Q = np.zeros((self.n, self.n), dtype=np.float64)
        for i, c in self.linear.items():
            Q[i, i] = c
        for (i, j), c in self.quadratic.items():
            Q[i, j] = c
        return Q

    def __repr__(self) -> str:
        return (
            f"BinaryQubo(n={self.n}, "
            f"linear_terms={len(self.linear)}, "
            f"quadratic_terms={len(self.quadratic)})"
        )
