"""QAOA — Quantum Approximate Optimisation Algorithm for QUBO problems.

Builds a QAOA ansatz directly from a BinaryQubo via the arvak.Circuit API
(no arvak-sim dependency) and uses COBYLA to optimise the γ/β angles.

QAOA ansatz construction:
  1. H on all qubits → uniform superposition.
  2. For each layer in range(p):
     - Problem unitary U_C(γ):
       * Quadratic (i,j): cx(i,j); rz(2γw, j); cx(i,j)
       * Linear    (i):   rz(2γh, i)
     - Mixer U_B(β): rx(2β, i) on each qubit.
  3. Measure all.

Cost function: CVaR of sampled QUBO values (top 10% by default), minimised
by COBYLA.

Quick start::

    from arvak.optimize import BinaryQubo, QAOASolver, qubo_from_maxcut

    Q = qubo_from_maxcut({(0, 1): 1.0, (1, 2): 1.0, (2, 0): 1.0})
    solver = QAOASolver(Q, p=1, shots=1024, seed=42)
    result = solver.solve()
    print(result.solution, result.cost)
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

import numpy as np
from scipy.optimize import OptimizeResult, minimize

import arvak

from ._pce import _default_backend
from ._qubo import BinaryQubo

if TYPE_CHECKING:
    pass


# ---------------------------------------------------------------------------
# Result type
# ---------------------------------------------------------------------------

@dataclass
class QaoaResult:
    """Result of a QAOA solve."""

    solution: list[bool]
    """Best binary assignment found."""

    cost: float
    """QUBO cost of the best solution."""

    gamma: list[float]
    """Optimal gamma (problem) angles, one per layer."""

    beta: list[float]
    """Optimal beta (mixer) angles, one per layer."""

    n_iters: int
    """Number of COBYLA iterations."""

    converged: bool
    """Whether scipy reported success."""

    top_solutions: list[tuple[list[bool], float]] = field(default_factory=list)
    """Up to 10 best (solution, cost) pairs from the final sampling pass."""


# ---------------------------------------------------------------------------
# QAOASolver
# ---------------------------------------------------------------------------

class QAOASolver:
    """QAOA solver for binary QUBO problems.

    Args:
        qubo:       Problem to solve (BinaryQubo).
        p:          QAOA depth (number of γ/β layers). Higher p → better
                    approximation ratio, deeper circuit.
        shots:      Shots per circuit evaluation.
        backend:    Callable (circuit, shots) → dict[str, int].
                    Defaults to the local statevector simulator.
        noise_model: Optional noise model; wraps backend in NoisyBackend.
        max_iter:   Maximum COBYLA iterations.
        seed:       RNG seed for reproducible initial parameters.
        cvar_top:   CVaR fraction for cost function (0.0–1.0, default 0.1).

    Example::

        from arvak.optimize import BinaryQubo, QAOASolver

        Q = BinaryQubo.from_matrix([[-1, 2], [0, -1]])
        result = QAOASolver(Q, p=2, shots=1024, seed=0).solve()
    """

    def __init__(
        self,
        qubo: BinaryQubo,
        p: int = 1,
        *,
        shots: int = 1024,
        backend=None,
        noise_model=None,
        max_iter: int = 300,
        seed: int | None = None,
        cvar_top: float = 0.1,
    ) -> None:
        if p < 1:
            raise ValueError(f"p must be >= 1, got {p}")
        self.qubo = qubo
        self.p = p
        self.shots = shots
        self.max_iter = max_iter
        self.cvar_top = max(0.01, min(1.0, cvar_top))
        self._rng = np.random.default_rng(seed)

        if noise_model is not None:
            from ._backend import NoisyBackend
            self._backend = NoisyBackend(backend or _default_backend, noise_model)
        else:
            self._backend = backend or _default_backend

    def solve(self) -> QaoaResult:
        """Run QAOA optimisation and return the best solution found."""
        # 2p parameters: [γ_0, ..., γ_{p-1}, β_0, ..., β_{p-1}]
        theta0 = np.concatenate([
            self._rng.uniform(0.0, 0.1, self.p),   # gamma — small initial angles
            self._rng.uniform(0.0, 0.1, self.p),   # beta
        ])

        opt: OptimizeResult = minimize(
            self._cost,
            theta0,
            method="COBYLA",
            options={"maxiter": self.max_iter, "rhobeg": 0.2},
        )

        gamma = opt.x[: self.p].tolist()
        beta = opt.x[self.p :].tolist()

        # Final high-shot sampling with best parameters.
        final_shots = max(self.shots, 4096)
        counts = self._sample(opt.x, final_shots)

        # Evaluate all sampled bitstrings.
        assignments, costs = self._evaluate_counts(counts)
        best_idx = int(np.argmin(costs))
        order = np.argsort(costs)
        top = [(assignments[i], float(costs[i])) for i in order[:10]]

        return QaoaResult(
            solution=assignments[best_idx],
            cost=float(costs[best_idx]),
            gamma=gamma,
            beta=beta,
            n_iters=int(opt.nfev),
            converged=bool(opt.success),
            top_solutions=top,
        )

    # ------------------------------------------------------------------
    # Cost function
    # ------------------------------------------------------------------

    def _cost(self, theta: np.ndarray) -> float:
        """CVaR of QUBO costs over sampled bitstrings."""
        counts = self._sample(theta, self.shots)
        _, costs = self._evaluate_counts(counts)
        n_keep = max(1, int(len(costs) * self.cvar_top))
        return float(np.sort(costs)[:n_keep].mean())

    def _sample(self, theta: np.ndarray, shots: int) -> dict[str, int]:
        gamma = theta[: self.p]
        beta = theta[self.p :]
        circuit = _build_qaoa_circuit(self.qubo, gamma, beta)
        return self._backend(circuit, shots)

    def _evaluate_counts(
        self, counts: dict[str, int]
    ) -> tuple[list[list[bool]], np.ndarray]:
        """Decode bitstrings to assignments and evaluate QUBO costs."""
        n = self.qubo.n
        assignments: list[list[bool]] = []
        costs: list[float] = []
        for bitstring, count in counts.items():
            bs = bitstring.zfill(n)
            # Bit ordering: bitstring[0] = most significant bit → qubit n-1
            # We want assignment[i] = bit for variable i = qubit i
            assignment = [bool(int(bs[n - 1 - i])) for i in range(n)]
            qubo_cost = _eval_qubo(self.qubo, assignment)
            # Repeat for each count
            for _ in range(count):
                assignments.append(assignment)
                costs.append(qubo_cost)
        if not assignments:
            return [], np.array([])
        return assignments, np.array(costs)


# ---------------------------------------------------------------------------
# QAOA circuit builder
# ---------------------------------------------------------------------------

def _build_qaoa_circuit(
    qubo: BinaryQubo,
    gamma: np.ndarray,
    beta: np.ndarray,
) -> arvak.Circuit:
    """Build the QAOA circuit for a QUBO problem.

    Structure:
      H on all qubits
      For each layer l in [0, p):
        U_C(gamma[l]): QUBO cost unitary
        U_B(beta[l]):  Mixer unitary
      Measure all

    The QASM circuit is assembled as a string and parsed by arvak.from_qasm.
    """
    n = qubo.n
    p = len(gamma)

    lines: list[str] = [
        "OPENQASM 3.0;",
        'include "stdgates.inc";',
        f"qubit[{n}] q;",
        f"bit[{n}] c;",
    ]

    # Initial superposition
    for i in range(n):
        lines.append(f"h q[{i}];")

    for layer in range(p):
        g = float(gamma[layer])
        b = float(beta[layer])

        # --- Problem unitary U_C(gamma) ---
        # Quadratic terms: cx(i,j); rz(2γw, j); cx(i,j)
        for (i, j), w in qubo.quadratic.items():
            angle = 2.0 * g * float(w)
            lines.append(f"cx q[{i}], q[{j}];")
            lines.append(f"rz({angle}) q[{j}];")
            lines.append(f"cx q[{i}], q[{j}];")

        # Linear terms: rz(2γh, i)
        for i, h in qubo.linear.items():
            angle = 2.0 * g * float(h)
            lines.append(f"rz({angle}) q[{i}];")

        # --- Mixer U_B(beta) ---
        for i in range(n):
            lines.append(f"rx({2.0 * b}) q[{i}];")

    lines.append("c = measure q;")
    return arvak.from_qasm("\n".join(lines))


# ---------------------------------------------------------------------------
# QUBO evaluator
# ---------------------------------------------------------------------------

def _eval_qubo(qubo: BinaryQubo, assignment: list[bool]) -> float:
    """Evaluate QUBO cost for a binary assignment."""
    x = assignment
    cost = 0.0
    for i, h in qubo.linear.items():
        if x[i]:
            cost += h
    for (i, j), w in qubo.quadratic.items():
        if x[i] and x[j]:
            cost += w
    return cost
