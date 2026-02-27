"""VQE — Variational Quantum Eigensolver for SparsePauliOp Hamiltonians.

Estimates the ground-state energy of a Hamiltonian expressed as a sum of
weighted Pauli strings via a hardware-efficient variational ansatz + COBYLA.

Algorithm:
  1. For each Pauli term P_k: build a measurement circuit = ansatz + basis
     rotations (X→H, Y→Sdg+H) + measure_all.
  2. Run through backend → shot counts → parity sum → ⟨P_k⟩.
  3. ⟨H⟩ = Σ c_k ⟨P_k⟩.
  4. COBYLA minimises ⟨H⟩ over the ansatz parameters.

Commuting terms sharing the same qubit basis are grouped to reduce circuit
count.

Quick start::

    from arvak.optimize import VQESolver, SparsePauliOp

    # H = -ZZ  (ground state energy = -1)
    h = SparsePauliOp([(-1.0, {0: 'Z', 1: 'Z'})])
    solver = VQESolver(h, n_qubits=2, n_layers=2, shots=2048, seed=42)
    result = solver.solve()
    print(result.energy)   # ≈ -1.0
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

import numpy as np
from scipy.optimize import OptimizeResult, minimize

import arvak

from ._pce import _build_ansatz, _default_backend

if TYPE_CHECKING:
    pass


# ---------------------------------------------------------------------------
# SparsePauliOp
# ---------------------------------------------------------------------------

class SparsePauliOp:
    """Lightweight sparse Pauli operator for VQE.

    Represents H = Σ_k c_k · P_k where each P_k is a Pauli string given
    as a dict mapping qubit index → 'X', 'Y', or 'Z'.

    Args:
        terms: List of (coeff, ops) pairs where ops is dict[int, str].

    Example::

        # H = -1.0 * Z0 Z1 + 0.5 * X0
        h = SparsePauliOp([
            (-1.0, {0: 'Z', 1: 'Z'}),
            ( 0.5, {0: 'X'}),
        ])
    """

    def __init__(self, terms: list[tuple[float, dict[int, str]]]) -> None:
        self.terms: list[tuple[float, dict[int, str]]] = []
        for coeff, ops in terms:
            # Normalise operator letters to uppercase; drop identity (empty) terms.
            norm_ops = {int(q): str(p).upper() for q, p in ops.items() if str(p).upper() != 'I'}
            self.terms.append((float(coeff), norm_ops))

    @classmethod
    def from_hamiltonian(cls, hamiltonian) -> "SparsePauliOp":
        """Convert an ``arvak.sim.Hamiltonian`` to SparsePauliOp.

        Args:
            hamiltonian: An ``arvak.sim.Hamiltonian`` object.

        Returns:
            SparsePauliOp equivalent.
        """
        from arvak._native import sim as _sim
        terms = []
        for i in range(hamiltonian.n_terms()):
            # Access each term via the Rust repr — we use Hamiltonian.from_terms
            # round-trip is not available, so we build from scratch via repr parsing.
            # Instead, expose directly through the Rust API.
            pass
        # Fallback: ask user to pass terms explicitly.
        raise NotImplementedError(
            "from_hamiltonian is not yet implemented. "
            "Construct SparsePauliOp directly with terms=[(coeff, {qubit: 'X'/'Y'/'Z'}), ...]"
        )

    def n_qubits(self) -> int:
        """Minimum number of qubits required."""
        max_q = -1
        for _, ops in self.terms:
            for q in ops:
                if q > max_q:
                    max_q = q
        return max_q + 1 if max_q >= 0 else 0

    def __repr__(self) -> str:
        return f"SparsePauliOp(n_terms={len(self.terms)}, n_qubits={self.n_qubits()})"


# ---------------------------------------------------------------------------
# Result type
# ---------------------------------------------------------------------------

@dataclass
class VqeResult:
    """Result of a VQE solve."""

    energy: float
    """Ground-state energy estimate."""

    params: np.ndarray
    """Optimal ansatz parameters."""

    n_iters: int
    """Number of COBYLA iterations (nfev)."""

    converged: bool
    """Whether scipy reported success."""

    energy_history: list[float] = field(default_factory=list)
    """Energy value at each cost function evaluation."""


# ---------------------------------------------------------------------------
# VQESolver
# ---------------------------------------------------------------------------

class VQESolver:
    """Variational Quantum Eigensolver.

    Estimates the ground-state energy of a Hamiltonian via a hardware-efficient
    RY + CNOT-ring ansatz and COBYLA optimisation.

    Args:
        hamiltonian: SparsePauliOp describing H.
        n_qubits:    Number of qubits in the ansatz.
        n_layers:    Ansatz depth (RY + CNOT layers).
        shots:       Shots per circuit evaluation.
        backend:     Callable (circuit, shots) → dict[str, int].
                     Defaults to the local statevector simulator.
        noise_model: Optional noise model; wraps backend in NoisyBackend.
        max_iter:    Maximum COBYLA iterations.
        seed:        RNG seed for reproducible initial parameters.

    Example::

        from arvak.optimize import VQESolver, SparsePauliOp

        h = SparsePauliOp([(-1.0, {0: 'Z', 1: 'Z'})])
        result = VQESolver(h, n_qubits=2, n_layers=2, seed=0).solve()
        print(result.energy)
    """

    def __init__(
        self,
        hamiltonian: SparsePauliOp,
        *,
        n_qubits: int,
        n_layers: int = 2,
        shots: int = 1024,
        backend=None,
        noise_model=None,
        max_iter: int = 300,
        seed: int | None = None,
    ) -> None:
        self.hamiltonian = hamiltonian
        self.n_qubits = n_qubits
        self.n_layers = n_layers
        self.shots = shots
        self.max_iter = max_iter
        self._rng = np.random.default_rng(seed)
        self._history: list[float] = []

        if noise_model is not None:
            from ._backend import NoisyBackend
            self._backend = NoisyBackend(backend or _default_backend, noise_model)
        else:
            self._backend = backend or _default_backend

        # Group terms by qubit basis (set of qubit→pauli mappings) to
        # minimise circuit count.
        self._groups = _group_by_basis(hamiltonian.terms)

    def solve(self) -> VqeResult:
        """Run VQE and return the ground-state energy estimate."""
        n_params = self.n_layers * self.n_qubits
        theta0 = self._rng.uniform(0.0, 2.0 * math.pi, n_params)
        self._history = []

        opt: OptimizeResult = minimize(
            self._cost,
            theta0,
            method="COBYLA",
            options={"maxiter": self.max_iter, "rhobeg": 0.3},
        )

        return VqeResult(
            energy=float(opt.fun),
            params=opt.x,
            n_iters=int(opt.nfev),
            converged=bool(opt.success),
            energy_history=list(self._history),
        )

    # ------------------------------------------------------------------
    # Cost function
    # ------------------------------------------------------------------

    def _cost(self, theta: np.ndarray) -> float:
        """Evaluate ⟨H⟩ = Σ c_k ⟨P_k⟩ for the given ansatz parameters."""
        energy = 0.0
        for basis, term_list in self._groups.items():
            # Build measurement circuit: ansatz + basis rotations + measure_all
            circuit = _build_measurement_circuit(
                self.n_qubits, self.n_layers, theta, basis
            )
            counts = self._backend(circuit, self.shots)
            total = sum(counts.values())
            if total == 0:
                continue

            for coeff, ops in term_list:
                # ⟨P_k⟩ = Σ_bitstring (-1)^parity(bitstring, ops) * count / total
                exp_val = _parity_expectation(counts, ops, self.n_qubits, total)
                energy += coeff * exp_val

        self._history.append(energy)
        return energy


# ---------------------------------------------------------------------------
# Circuit construction
# ---------------------------------------------------------------------------

def _build_measurement_circuit(
    n_qubits: int,
    n_layers: int,
    theta: np.ndarray,
    basis: frozenset[tuple[int, str]],
) -> arvak.Circuit:
    """Build ansatz + basis rotations + measure circuit for a Pauli basis.

    Basis rotations:
      - X measurement: H gate (rotate X basis → Z basis)
      - Y measurement: Sdg + H gate (rotate Y basis → Z basis)
      - Z measurement: no rotation needed
    """
    lines: list[str] = [
        "OPENQASM 3.0;",
        'include "stdgates.inc";',
        f"qubit[{n_qubits}] q;",
        f"bit[{n_qubits}] c;",
    ]

    # Ansatz layers (same as PCESolver ansatz but without final RY layer)
    for layer in range(n_layers):
        offset = layer * n_qubits
        for i in range(n_qubits):
            angle = float(theta[offset + i])
            lines.append(f"ry({angle}) q[{i}];")
        if n_qubits > 1:
            for i in range(n_qubits - 1):
                lines.append(f"cx q[{i}], q[{i + 1}];")
            lines.append(f"cx q[{n_qubits - 1}], q[0];")

    # Basis rotations
    basis_dict = dict(basis)
    for q in range(n_qubits):
        op = basis_dict.get(q, 'Z')
        if op == 'X':
            lines.append(f"h q[{q}];")
        elif op == 'Y':
            lines.append(f"sdg q[{q}];")
            lines.append(f"h q[{q}];")

    lines.append("c = measure q;")
    return arvak.from_qasm("\n".join(lines))


# ---------------------------------------------------------------------------
# Expectation value helpers
# ---------------------------------------------------------------------------

def _parity_expectation(
    counts: dict[str, int],
    ops: dict[int, str],
    n_qubits: int,
    total: int,
) -> float:
    """Compute ⟨P⟩ via parity of Z-basis measurements.

    After basis rotation, each Pauli is measured in the Z basis.
    ⟨P⟩ = Σ_{bitstring} (-1)^{parity of measured qubits} * count / total.
    """
    qubits = sorted(ops.keys())
    exp_val = 0.0
    for bitstring, count in counts.items():
        # Pad short bitstrings to n_qubits
        bs = bitstring.zfill(n_qubits)
        parity = 0
        for q in qubits:
            # Qiskit/Arvak bit ordering: bitstring[0] = most significant bit
            # qubit q → position (n_qubits - 1 - q)
            bit_pos = n_qubits - 1 - q
            if 0 <= bit_pos < len(bs):
                parity ^= int(bs[bit_pos])
        exp_val += ((-1) ** parity) * count
    return exp_val / total


def _group_by_basis(
    terms: list[tuple[float, dict[int, str]]]
) -> dict[frozenset[tuple[int, str]], list[tuple[float, dict[int, str]]]]:
    """Group Pauli terms by their qubit basis (qubit, pauli_type) sets.

    Terms with the same basis can share a single circuit execution.
    A 'basis' here is defined by the (qubit, op) pairs — commuting terms
    that require identical measurement circuits are batched together.
    """
    groups: dict[frozenset[tuple[int, str]], list[tuple[float, dict[int, str]]]] = {}
    for coeff, ops in terms:
        key = frozenset(ops.items())
        if key not in groups:
            groups[key] = []
        groups[key].append((coeff, ops))
    return groups
