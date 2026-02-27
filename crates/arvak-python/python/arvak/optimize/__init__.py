"""arvak.optimize — Variational QUBO solvers and graph decomposition tools.

Algorithms (derived from Divi/QoroQuantum, MIT License):
  - PCE (Pauli Correlation Encoding): variational QUBO solver that compresses
    n binary variables onto k = O(log n) or O(sqrt n) qubits.
  - Spectral graph partitioning: split large graphs into balanced subgraphs
    for parallel QAOA or MaxCut decomposition.

These are pure-Python implementations; no Divi dependency is required.
scipy and numpy are used for numerics; scikit-learn is optional (k-means
falls back to a pure-numpy implementation if unavailable).

Quick start::

    import numpy as np
    from arvak.optimize import BinaryQubo, PCESolver, spectral_partition

    # --- QUBO solver ---
    Q = np.array([[-1, 2, 0],
                  [ 0,-1, 2],
                  [ 0, 0,-1]], dtype=float)
    qubo = BinaryQubo.from_matrix(Q)
    solver = PCESolver(qubo, encoding="dense", shots=512, seed=42)
    result = solver.solve()
    print(result.solution, result.cost)

    # --- Graph partitioning ---
    edges = {(0,1): 1.0, (1,2): 1.0, (2,3): 1.0, (3,0): 1.0, (0,2): 0.5}
    parts = spectral_partition(edges, n_parts=2, n_nodes=4)
    print(parts)  # e.g. [[0, 2], [1, 3]]
"""

from ._backend import HalBackend, NoisyBackend
from ._encoding import DenseEncoding, Encoding, PolyEncoding
from ._partition import spectral_partition
from ._pce import Backend, PceResult, PCESolver
from ._problems import (
    decode_tsp,
    qubo_from_maxcut,
    qubo_from_portfolio,
    qubo_from_tsp,
    tsp_tour_length,
)
from ._qaoa import QaoaResult, QAOASolver
from ._qubo import BinaryQubo
from ._vqe import SparsePauliOp, VqeResult, VQESolver

__all__ = [
    # QUBO type
    "BinaryQubo",
    # Encoding strategies
    "DenseEncoding",
    "PolyEncoding",
    "Encoding",
    # PCE solver
    "PCESolver",
    "PceResult",
    "Backend",
    # VQE
    "VQESolver",
    "VqeResult",
    "SparsePauliOp",
    # QAOA
    "QAOASolver",
    "QaoaResult",
    # HAL backend adapters
    "HalBackend",
    "NoisyBackend",
    # Graph partitioning
    "spectral_partition",
    # Problem encodings
    "qubo_from_maxcut",
    "qubo_from_tsp",
    "qubo_from_portfolio",
    "decode_tsp",
    "tsp_tour_length",
]
