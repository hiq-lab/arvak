"""Parity encoding maps for PCE (Pauli Correlation Encoding).

Two strategies:
  - DenseEncoding:  k = ceil(log2(n+1)) qubits.  Best compression.
  - PolyEncoding:   k = 2*ceil(sqrt(n)) qubits.  Better locality.

In both cases variable i is represented as:
    x_i = parity(bitstring & mask_i)
         = popcount(bitstring & mask_i) mod 2

where mask_i is a k-bit integer stored in parity_masks[i].
"""

from __future__ import annotations

import math

import numpy as np


# ---------------------------------------------------------------------------
# Shared parity kernel
# ---------------------------------------------------------------------------

def _popcount_parity(x: np.ndarray) -> np.ndarray:
    """Compute popcount parity for each element of uint64 array.

    Returns a bool array with True where the number of set bits is odd.
    Uses XOR-folding which is branchless and vectorises well with numpy.
    """
    x = np.asarray(x, dtype=np.uint64)
    x = x ^ (x >> np.uint64(32))
    x = x ^ (x >> np.uint64(16))
    x = x ^ (x >> np.uint64(8))
    x = x ^ (x >> np.uint64(4))
    x = x ^ (x >> np.uint64(2))
    x = x ^ (x >> np.uint64(1))
    return (x & np.uint64(1)).astype(bool)


# ---------------------------------------------------------------------------
# Dense encoding  (k = ceil(log2(n+1)) qubits)
# ---------------------------------------------------------------------------

class DenseEncoding:
    """Map n binary variables to k = ceil(log2(n+1)) qubits via parity masks.

    Mask assignment: variable i uses mask = i+1 (1-indexed).
    This guarantees all n masks are distinct and non-zero.

    Compression ratio: n / k  ≈  n / log2(n).
    For n=256: k=8 qubits.  For n=1000: k=10 qubits.
    """

    def __init__(self, n_vars: int) -> None:
        if n_vars <= 0:
            raise ValueError(f"n_vars must be positive, got {n_vars}")
        self.n_vars = n_vars
        self.n_qubits = max(1, math.ceil(math.log2(n_vars + 1)))
        # parity_masks[i] = bitmask for variable i  (uint64)
        self.parity_masks = np.arange(1, n_vars + 1, dtype=np.uint64)

    def decode_batch(self, bitstrings: np.ndarray) -> np.ndarray:
        """Decode a batch of bitstrings to binary variable assignments.

        Args:
            bitstrings: (n_samples,) uint64 array, one integer per sample
                        where bit j represents qubit j's measurement.

        Returns:
            (n_samples, n_vars) bool array.
        """
        bs = np.asarray(bitstrings, dtype=np.uint64)
        # (n_samples, 1) & (1, n_vars)  →  (n_samples, n_vars)
        intersect = bs[:, None] & self.parity_masks[None, :]
        return _popcount_parity(intersect)

    def pauli_correlations(self, bitstrings: np.ndarray, weights: np.ndarray) -> np.ndarray:
        """Compute weighted parity expectations E[(-1)^parity] per variable.

        Returns:
            (n_vars,) float64 in [-1, 1].  +1 = all even parity, -1 = all odd.
        """
        bs = np.asarray(bitstrings, dtype=np.uint64)
        w = np.asarray(weights, dtype=np.float64)
        w_sum = w.sum()
        if w_sum == 0.0:
            return np.zeros(len(self.parity_masks), dtype=np.float64)
        w = w / w_sum
        parities = _popcount_parity(bs[:, None] & self.parity_masks[None, :]).astype(np.float64)
        # expectation of (-1)^parity:  +1 when parity=0, -1 when parity=1
        signed = 1.0 - 2.0 * parities      # shape (n_samples, n_vars)
        return (signed * w[:, None]).sum(axis=0)

    @property
    def compression_ratio(self) -> float:
        return self.n_vars / self.n_qubits

    def __repr__(self) -> str:
        return (
            f"DenseEncoding(n_vars={self.n_vars}, n_qubits={self.n_qubits}, "
            f"compression={self.compression_ratio:.1f}x)"
        )


# ---------------------------------------------------------------------------
# Polynomial encoding  (k = 2*ceil(sqrt(n)) qubits)
# ---------------------------------------------------------------------------

class PolyEncoding:
    """Map n binary variables to k = 2*ceil(sqrt(n)) qubits.

    Variables are arranged on a virtual grid (rows × cols).
    Variable at position (r, c) uses:
        mask = row_bit_r | col_bit_c
    where row and column qubits are disjoint sets.

    Lower compression than Dense but better locality: nearby variables
    in the grid share row/column qubits, which can benefit connectivity.

    For n=256: side=16, k=32 qubits.  For n=64: side=8, k=16 qubits.
    """

    def __init__(self, n_vars: int) -> None:
        if n_vars <= 0:
            raise ValueError(f"n_vars must be positive, got {n_vars}")
        self.n_vars = n_vars
        side = math.ceil(math.sqrt(n_vars))
        self.side = side
        self.n_row_qubits = side
        self.n_col_qubits = side
        self.n_qubits = 2 * side

        masks = np.zeros(n_vars, dtype=np.uint64)
        for i in range(n_vars):
            r, c = divmod(i, side)
            row_bit = np.uint64(1) << np.uint64(r)
            col_bit = np.uint64(1) << np.uint64(side + c)
            masks[i] = row_bit | col_bit
        self.parity_masks = masks

    def decode_batch(self, bitstrings: np.ndarray) -> np.ndarray:
        bs = np.asarray(bitstrings, dtype=np.uint64)
        intersect = bs[:, None] & self.parity_masks[None, :]
        return _popcount_parity(intersect)

    def pauli_correlations(self, bitstrings: np.ndarray, weights: np.ndarray) -> np.ndarray:
        bs = np.asarray(bitstrings, dtype=np.uint64)
        w = np.asarray(weights, dtype=np.float64)
        w = w / w.sum()
        parities = _popcount_parity(bs[:, None] & self.parity_masks[None, :]).astype(np.float64)
        signed = 1.0 - 2.0 * parities
        return (signed * w[:, None]).sum(axis=0)

    @property
    def compression_ratio(self) -> float:
        return self.n_vars / self.n_qubits

    def __repr__(self) -> str:
        return (
            f"PolyEncoding(n_vars={self.n_vars}, side={self.side}, "
            f"n_qubits={self.n_qubits}, compression={self.compression_ratio:.1f}x)"
        )


# ---------------------------------------------------------------------------
# Type alias
# ---------------------------------------------------------------------------

Encoding = DenseEncoding | PolyEncoding
