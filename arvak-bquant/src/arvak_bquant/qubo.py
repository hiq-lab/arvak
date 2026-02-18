"""QUBO and Ising problem representations with conversion utilities."""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np


@dataclass
class QUBOProblem:
    """Quadratic Unconstrained Binary Optimization problem.

    Minimize  x^T Q x + offset
    where x_i in {0, 1}.

    Parameters
    ----------
    Q : np.ndarray
        Upper-triangular (or symmetric) cost matrix of shape (n, n).
    offset : float
        Constant energy offset.
    """

    Q: np.ndarray
    offset: float = 0.0

    @property
    def num_variables(self) -> int:
        return self.Q.shape[0]

    def evaluate(self, x: np.ndarray) -> float:
        """Evaluate the objective for a binary assignment vector."""
        return float(x @ self.Q @ x) + self.offset


@dataclass
class IsingProblem:
    """Ising model: H = sum_{i<j} J_ij s_i s_j + sum_i h_i s_i + offset.

    Spins s_i in {+1, -1}.

    Parameters
    ----------
    J : dict[tuple[int, int], float]
        Coupling terms (i < j).
    h : dict[int, float]
        Linear bias terms.
    offset : float
        Constant energy offset.
    num_qubits : int
        Number of spin variables.
    """

    J: dict[tuple[int, int], float] = field(default_factory=dict)
    h: dict[int, float] = field(default_factory=dict)
    offset: float = 0.0
    num_qubits: int = 0

    def evaluate(self, spins: np.ndarray) -> float:
        """Evaluate the Ising energy for a spin assignment (+1/-1)."""
        energy = self.offset
        for i, hi in self.h.items():
            energy += hi * spins[i]
        for (i, j), jij in self.J.items():
            energy += jij * spins[i] * spins[j]
        return float(energy)


def qubo_to_ising(qubo: QUBOProblem) -> IsingProblem:
    """Convert a QUBO problem to an Ising problem.

    Uses the substitution  x_i = (1 - s_i) / 2.

    For binary x_i, x_i^2 = x_i, so diagonal terms are linear.

    The QUBO energy is:  sum_{i,j} Q_{ij} x_i x_j + offset
    With the substitution this becomes an Ising Hamiltonian
    H = sum_{i<j} J_{ij} s_i s_j + sum_i h_i s_i + const.
    """
    n = qubo.num_variables
    Q = qubo.Q

    # Symmetrize Q
    Q_sym = (Q + Q.T) / 2.0

    J: dict[tuple[int, int], float] = {}
    h_vec = np.zeros(n)
    offset = qubo.offset

    # Off-diagonal terms: the effective coefficient for x_i x_j (i<j) in
    # x^T Q x is 2*Q_sym[i,j] (because Q[i,j] and Q[j,i] both contribute).
    # Substituting x_i x_j = (1 - s_i)(1 - s_j)/4:
    #   J[i,j] += 2*Q_sym[i,j] / 4 = Q_sym[i,j] / 2
    #   h[i]   -= Q_sym[i,j] / 2
    #   h[j]   -= Q_sym[i,j] / 2
    #   offset += Q_sym[i,j] / 2
    for i in range(n):
        for j in range(i + 1, n):
            qij = Q_sym[i, j]
            if qij != 0.0:
                J[(i, j)] = qij / 2.0
                h_vec[i] -= qij / 2.0
                h_vec[j] -= qij / 2.0
                offset += qij / 2.0

    # Diagonal terms: Q_sym[i,i] * x_i  (since x_i^2 = x_i for binary)
    # x_i = (1 - s_i)/2
    # Contributes:
    #   h[i]   -= Q_sym[i,i] / 2
    #   offset += Q_sym[i,i] / 2
    for i in range(n):
        qii = Q_sym[i, i]
        h_vec[i] -= qii / 2.0
        offset += qii / 2.0

    # Build h dict (skip zeros)
    h: dict[int, float] = {}
    for i in range(n):
        if h_vec[i] != 0.0:
            h[i] = h_vec[i]

    return IsingProblem(J=J, h=h, offset=offset, num_qubits=n)
