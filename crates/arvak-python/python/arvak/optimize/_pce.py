"""PCE (Pauli Correlation Encoding) variational QUBO solver.

Algorithm (following Divi/QoroQuantum, MIT):
  1. Encode n binary variables onto k << n qubits via parity masks.
  2. Run a hardware-efficient ansatz circuit (RY layers + CNOT ring).
  3. Sample bitstrings; decode each to n binary variables via parity.
  4. Evaluate QUBO cost on decoded assignments.
  5. Classical optimiser (COBYLA) minimises expected or CVaR cost.
  6. After convergence, return the best decoded assignment found.

Two cost modes (controlled by alpha):
  - Smooth (alpha < 5.0):  x_i_soft = (1 - tanh(alpha * corr_i)) / 2
    where corr_i = E[(-1)^parity_i] is the signed parity expectation.
    QUBO is evaluated on these continuous [0,1] values — smooth landscape.

  - CVaR (alpha >= 5.0):  decode all bitstrings to binary {0,1},
    evaluate QUBO per bitstring, return mean of best cvar_top fraction.
    Focuses the optimiser on tail quality — useful in noisy settings.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Callable, Protocol

import numpy as np
from scipy.optimize import OptimizeResult, minimize

import arvak

from ._encoding import DenseEncoding, Encoding, PolyEncoding
from ._qubo import BinaryQubo


# ---------------------------------------------------------------------------
# Backend protocol
# ---------------------------------------------------------------------------

class Backend(Protocol):
    """Any callable that runs a circuit and returns shot counts."""

    def __call__(self, circuit: arvak.Circuit, shots: int) -> dict[str, int]:
        ...


# ---------------------------------------------------------------------------
# Result type
# ---------------------------------------------------------------------------

@dataclass
class PceResult:
    """Result of a PCE solve."""

    solution: list[bool]
    """Best decoded binary assignment."""

    cost: float
    """QUBO cost of best solution."""

    n_original_vars: int
    n_qubits: int
    compression_ratio: float

    n_function_evals: int
    """Number of circuit executions (scipy nfev)."""

    converged: bool
    """Whether scipy reported success."""

    top_solutions: list[tuple[list[bool], float]]
    """Up to 10 best (solution, cost) pairs from the final sampling pass."""


# ---------------------------------------------------------------------------
# PCE solver
# ---------------------------------------------------------------------------

class PCESolver:
    """Variational QUBO solver using Pauli Correlation Encoding.

    Example::

        from arvak.optimize import BinaryQubo, PCESolver

        qubo = BinaryQubo.from_matrix(Q)
        solver = PCESolver(qubo, encoding="dense", shots=1024)
        result = solver.solve()
        print(result.solution, result.cost)

    Args:
        qubo:       Problem to solve.
        encoding:   "dense" (k=ceil(log2(n+1))) or "poly" (k=2*ceil(sqrt(n))).
        n_layers:   Ansatz depth (RY+CNOT layers).  More layers = more expressive.
        shots:      Shots per circuit evaluation.
        alpha:      Cost mode parameter.  < 5.0 → smooth; >= 5.0 → CVaR.
        cvar_top:   Fraction of best samples used in CVaR mode (default 0.1).
        backend:    Optional callable(circuit, shots) -> dict[str,int].
                    Defaults to arvak.run_sim (local statevector simulator).
        max_iter:   Maximum scipy COBYLA iterations.
        seed:       RNG seed for reproducible initialisations.
    """

    def __init__(
        self,
        qubo: BinaryQubo,
        encoding: str = "dense",
        n_layers: int = 2,
        shots: int = 1024,
        alpha: float = 2.0,
        cvar_top: float = 0.1,
        backend: Backend | None = None,
        max_iter: int = 300,
        seed: int | None = None,
    ) -> None:
        self.qubo = qubo
        self.n_layers = n_layers
        self.shots = shots
        self.alpha = alpha
        self.cvar_top = max(0.01, min(1.0, cvar_top))
        self.max_iter = max_iter

        if encoding == "dense":
            self.enc: Encoding = DenseEncoding(qubo.n)
        elif encoding == "poly":
            self.enc = PolyEncoding(qubo.n)
        else:
            raise ValueError(f"Unknown encoding {encoding!r}; choose 'dense' or 'poly'")

        self._backend: Backend = backend or _default_backend
        self._rng = np.random.default_rng(seed)

    # ------------------------------------------------------------------
    # Public interface
    # ------------------------------------------------------------------

    def solve(self) -> PceResult:
        """Run PCE optimisation and return the best solution found."""
        k = self.enc.n_qubits
        n_params = self.n_layers * k
        theta0 = self._rng.uniform(0.0, 2.0 * math.pi, n_params)

        opt: OptimizeResult = minimize(
            self._cost,
            theta0,
            method="COBYLA",
            options={"maxiter": self.max_iter, "rhobeg": 0.5},
        )

        # Final high-shot sampling with best parameters.
        final_shots = max(self.shots, 4096)
        counts = self._sample(opt.x, final_shots)
        bitstrings, weights = _counts_to_arrays(counts)
        decoded = self.enc.decode_batch(bitstrings)
        costs = self.qubo.evaluate_batch(decoded)

        best_idx = int(np.argmin(costs))
        order = np.argsort(costs)
        top = [(decoded[i].tolist(), float(costs[i])) for i in order[:10]]

        return PceResult(
            solution=decoded[best_idx].tolist(),
            cost=float(costs[best_idx]),
            n_original_vars=self.qubo.n,
            n_qubits=k,
            compression_ratio=self.enc.compression_ratio,
            n_function_evals=opt.nfev,
            converged=bool(opt.success),
            top_solutions=top,
        )

    @property
    def encoding(self) -> Encoding:
        return self.enc

    # ------------------------------------------------------------------
    # Cost function
    # ------------------------------------------------------------------

    def _cost(self, theta: np.ndarray) -> float:
        counts = self._sample(theta, self.shots)
        bitstrings, weights = _counts_to_arrays(counts)

        if self.alpha >= 5.0:
            return self._cvar_cost(bitstrings, weights)
        return self._smooth_cost(bitstrings, weights)

    def _smooth_cost(self, bitstrings: np.ndarray, weights: np.ndarray) -> float:
        """Expected QUBO cost on continuous relaxation.

        x_i_soft = (1 - tanh(alpha * corr_i)) / 2
        where corr_i = E[(-1)^parity_i] in [-1, 1].
        """
        corr = self.enc.pauli_correlations(bitstrings, weights)
        x_soft = (1.0 - np.tanh(self.alpha * corr)) / 2.0
        return float(self.qubo.evaluate_batch(x_soft[None, :])[0])

    def _cvar_cost(self, bitstrings: np.ndarray, weights: np.ndarray) -> float:
        """Conditional Value at Risk: mean cost of the best top-fraction samples."""
        decoded = self.enc.decode_batch(bitstrings)
        costs = self.qubo.evaluate_batch(decoded)
        if len(costs) == 0:
            return 0.0
        n_keep = max(1, int(len(costs) * self.cvar_top))
        sorted_costs = np.sort(costs)
        return float(sorted_costs[:n_keep].mean())

    # ------------------------------------------------------------------
    # Circuit construction
    # ------------------------------------------------------------------

    def _sample(self, theta: np.ndarray, shots: int) -> dict[str, int]:
        circuit = _build_ansatz(self.enc.n_qubits, self.n_layers, theta)
        return self._backend(circuit, shots)


# ---------------------------------------------------------------------------
# Circuit builder (module-level — reusable outside PCESolver)
# ---------------------------------------------------------------------------

def _build_ansatz(n_qubits: int, n_layers: int, theta: np.ndarray) -> arvak.Circuit:
    """Hardware-efficient ansatz: alternating RY + CNOT-ring layers.

    Structure per layer::

        RY(theta_0) q[0]; RY(theta_1) q[1]; ... RY(theta_k-1) q[k-1];
        CNOT q[0]→q[1]; CNOT q[1]→q[2]; ... CNOT q[k-1]→q[0];

    Followed by a final RY layer and measurement.

    Args:
        n_qubits: Number of qubits k.
        n_layers: Number of RY+CNOT blocks.
        theta:    (n_layers * n_qubits,) parameter array.

    Returns:
        arvak.Circuit ready for run_sim or a real backend.
    """
    lines: list[str] = [
        "OPENQASM 3.0;",
        'include "stdgates.inc";',
        f"qubit[{n_qubits}] q;",
        f"bit[{n_qubits}] c;",
    ]

    for layer in range(n_layers):
        offset = layer * n_qubits
        for i in range(n_qubits):
            angle = float(theta[offset + i])
            lines.append(f"ry({angle}) q[{i}];")
        if n_qubits > 1:
            for i in range(n_qubits - 1):
                lines.append(f"cx q[{i}], q[{i + 1}];")
            lines.append(f"cx q[{n_qubits - 1}], q[0];")

    lines.append("c = measure q;")
    return arvak.from_qasm("\n".join(lines))


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _counts_to_arrays(counts: dict[str, int]) -> tuple[np.ndarray, np.ndarray]:
    """Convert counts dict to (bitstrings, weights) arrays."""
    bitstrings = np.array([int(bs, 2) for bs in counts.keys()], dtype=np.uint64)
    weights = np.array(list(counts.values()), dtype=np.float64)
    return bitstrings, weights


def _default_backend(circuit: arvak.Circuit, shots: int) -> dict[str, int]:
    return arvak.run_sim(circuit, shots)
