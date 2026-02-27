"""Spectral graph partitioning for large-circuit decomposition.

Splits a weighted graph into n_parts balanced subgraphs using normalised
spectral clustering (Shi & Malik 2000).  Intended use: decompose a large
QAOA or MaxCut problem into smaller sub-circuits that can run in parallel
on separate QPUs or be solved independently and recombined.

The implementation is pure numpy/scipy — no external graph library needed,
though NetworkX graphs are accepted as a convenience input.

Example::

    from arvak.optimize import spectral_partition

    # adjacency as dict of edge weights
    edges = {(0, 1): 1.0, (1, 2): 0.5, (2, 3): 1.0, (3, 0): 0.5}
    parts = spectral_partition(edges, n_nodes=4, n_parts=2)
    # parts[0], parts[1] — lists of node indices

    # Or with a numpy adjacency matrix
    import numpy as np
    A = np.array([[0,1,0,1],[1,0,1,0],[0,1,0,1],[1,0,1,0]], dtype=float)
    parts = spectral_partition(A, n_parts=2)
"""

from __future__ import annotations

from typing import Union

import numpy as np

# NetworkX is optional; used only for the isinstance check.
try:
    import networkx as nx
    _HAS_NX = True
except ImportError:
    _HAS_NX = False

AdjacencyInput = Union[
    np.ndarray,
    dict[tuple[int, int], float],
    "nx.Graph",  # type: ignore[name-defined]
]


def spectral_partition(
    adjacency: AdjacencyInput,
    n_parts: int,
    *,
    n_nodes: int | None = None,
    random_state: int | None = 0,
) -> list[list[int]]:
    """Partition a graph into n_parts balanced subgraphs via spectral clustering.

    Args:
        adjacency:    Graph as one of:
                        - np.ndarray (n×n adjacency matrix, symmetric)
                        - dict {(i, j): weight} (undirected edges)
                        - networkx.Graph (if networkx is installed)
        n_parts:      Number of partitions to produce.
        n_nodes:      Required when adjacency is a dict and nodes are not
                      0-indexed up to max; inferred otherwise.
        random_state: K-means seed for reproducibility.

    Returns:
        List of n_parts lists, each containing node indices.
        All nodes appear in exactly one partition.
        Some partitions may be empty if the graph has fewer connected
        components than n_parts.

    Raises:
        ValueError: If n_parts < 1 or adjacency shape is invalid.
        ImportError: If adjacency is a NetworkX graph but networkx is not installed.
    """
    if n_parts < 1:
        raise ValueError(f"n_parts must be >= 1, got {n_parts}")

    A = _to_adjacency_matrix(adjacency, n_nodes)
    n = A.shape[0]

    if n_parts == 1:
        return [list(range(n))]

    if n_parts >= n:
        return [[i] for i in range(n)] + [[] for _ in range(n_parts - n)]

    L_sym = _normalised_laplacian(A)

    # k smallest eigenvectors (by eigenvalue) of the symmetric Laplacian.
    # np.linalg.eigh returns in ascending order.
    _, U = np.linalg.eigh(L_sym)
    U = U[:, :n_parts]          # (n, n_parts)

    # Row-normalise for k-means stability.
    norms = np.linalg.norm(U, axis=1, keepdims=True)
    U = U / np.where(norms > 1e-12, norms, 1.0)

    labels = _kmeans(U, n_parts, random_state=random_state)

    partitions: list[list[int]] = [[] for _ in range(n_parts)]
    for node, label in enumerate(labels):
        partitions[int(label)].append(node)
    return partitions


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

def _to_adjacency_matrix(
    adjacency: AdjacencyInput, n_nodes: int | None
) -> np.ndarray:
    """Normalise any supported adjacency input to a dense numpy matrix."""
    if _HAS_NX and isinstance(adjacency, nx.Graph):
        A = nx.to_numpy_array(adjacency)
        return A.astype(np.float64)

    if isinstance(adjacency, dict):
        # Infer n from keys if not provided.
        nodes: set[int] = set()
        for i, j in adjacency.keys():
            nodes.add(i)
            nodes.add(j)
        n = (n_nodes if n_nodes is not None else (max(nodes) + 1 if nodes else 0))
        if n == 0:
            raise ValueError("Empty adjacency dict and n_nodes not given")
        A = np.zeros((n, n), dtype=np.float64)
        for (i, j), w in adjacency.items():
            A[i, j] += w
            A[j, i] += w
        return A

    A = np.asarray(adjacency, dtype=np.float64)
    if A.ndim != 2 or A.shape[0] != A.shape[1]:
        raise ValueError("adjacency matrix must be a square 2-D array")
    # Symmetrise in case upper-triangular was provided.
    return (A + A.T) / 2.0


def _normalised_laplacian(A: np.ndarray) -> np.ndarray:
    """Compute the normalised (symmetric) Laplacian L_sym = I - D^{-1/2} A D^{-1/2}."""
    degree = A.sum(axis=1)
    d_inv_sqrt = np.where(degree > 1e-12, 1.0 / np.sqrt(degree), 0.0)
    D_inv_sqrt = np.diag(d_inv_sqrt)
    L_sym = np.eye(A.shape[0]) - D_inv_sqrt @ A @ D_inv_sqrt
    return L_sym


def _kmeans(
    X: np.ndarray,
    k: int,
    *,
    max_iter: int = 300,
    random_state: int | None = 0,
) -> np.ndarray:
    """K-means clustering; uses scikit-learn if available, else pure numpy."""
    try:
        from sklearn.cluster import KMeans  # type: ignore[import]
        km = KMeans(n_clusters=k, n_init=10, max_iter=max_iter, random_state=random_state)
        return km.fit_predict(X)
    except ImportError:
        return _numpy_kmeans(X, k, max_iter=max_iter, random_state=random_state)


def _numpy_kmeans(
    X: np.ndarray,
    k: int,
    *,
    max_iter: int = 300,
    random_state: int | None = 0,
) -> np.ndarray:
    """Pure-numpy k-means fallback (no external deps)."""
    rng = np.random.default_rng(random_state)
    n = X.shape[0]
    # k-means++ initialisation.
    centers = [X[rng.integers(n)]]
    for _ in range(1, k):
        dists = np.array([min(np.linalg.norm(x - c) ** 2 for c in centers) for x in X])
        probs = dists / dists.sum()
        centers.append(X[rng.choice(n, p=probs)])
    centers_arr = np.stack(centers)

    labels = np.zeros(n, dtype=int)
    for _ in range(max_iter):
        dists = np.linalg.norm(X[:, None, :] - centers_arr[None, :, :], axis=2)
        new_labels = np.argmin(dists, axis=1)
        if np.array_equal(new_labels, labels):
            break
        labels = new_labels
        for i in range(k):
            mask = labels == i
            if mask.any():
                centers_arr[i] = X[mask].mean(axis=0)
    return labels
