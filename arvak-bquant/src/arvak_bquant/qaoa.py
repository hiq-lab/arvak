"""QAOA circuit generation — QUBO to OpenQASM 3 without external dependencies.

Mirrors the structure of ``demos/src/circuits/qaoa.rs``:
  1. Hadamard init (uniform superposition)
  2. For each of *p* layers:
     a. Cost unitary  — CX-RZ-CX for ZZ terms, RZ for Z terms
     b. Mixer unitary — RX on every qubit
  3. Measure all qubits

Parameter initialization uses a Trotterized adiabatic schedule.
"""

from __future__ import annotations

import math

from .qubo import IsingProblem, QUBOProblem, qubo_to_ising


def _trotterized_params(p: int) -> tuple[list[float], list[float]]:
    """Trotterized adiabatic parameter initialization."""
    dt = 1.0 / (p + 1)
    gamma = [i * dt * math.pi / 2.0 * dt for i in range(1, p + 1)]
    beta = [(1.0 - i * dt) * math.pi / 2.0 * dt for i in range(1, p + 1)]
    return gamma, beta


def qaoa_circuit_qasm3(
    qubo: QUBOProblem,
    p: int = 1,
    gamma: list[float] | None = None,
    beta: list[float] | None = None,
) -> str:
    """Generate an OpenQASM 3 string for a QAOA circuit targeting *qubo*.

    Parameters
    ----------
    qubo : QUBOProblem
        The combinatorial optimization problem.
    p : int
        Number of QAOA layers (default 1).
    gamma, beta : list[float] | None
        Optional explicit parameters (length *p* each).  If ``None``,
        Trotterized adiabatic initialization is used.

    Returns
    -------
    str
        OpenQASM 3 source.
    """
    ising = qubo_to_ising(qubo)

    if gamma is None or beta is None:
        gamma, beta = _trotterized_params(p)

    if len(gamma) != p or len(beta) != p:
        raise ValueError(f"gamma and beta must have length p={p}")

    n = ising.num_qubits
    lines: list[str] = [
        'OPENQASM 3.0;',
        'include "stdgates.inc";',
        f'qubit[{n}] q;',
        f'bit[{n}] c;',
        '',
    ]

    # Step 1: Hadamard on all qubits
    for i in range(n):
        lines.append(f'h q[{i}];')
    lines.append('')

    # Step 2: p layers
    for layer in range(p):
        lines.append(f'// Layer {layer + 1}')

        # Cost unitary: ZZ interactions via CX-RZ-CX decomposition
        for (i, j), jij in ising.J.items():
            angle = gamma[layer] * jij
            lines.append(f'cx q[{i}], q[{j}];')
            lines.append(f'rz({angle}) q[{j}];')
            lines.append(f'cx q[{i}], q[{j}];')

        # Cost unitary: Z terms via RZ
        for i, hi in ising.h.items():
            angle = gamma[layer] * hi
            lines.append(f'rz({angle}) q[{i}];')

        # Mixer unitary: RX(2*beta) on all qubits
        rx_angle = 2.0 * beta[layer]
        for i in range(n):
            lines.append(f'rx({rx_angle}) q[{i}];')

        lines.append('')

    # Step 3: Measure
    for i in range(n):
        lines.append(f'c[{i}] = measure q[{i}];')

    return '\n'.join(lines) + '\n'
