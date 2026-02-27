"""Standard QUBO problem encodings.

Converts classical optimisation problems to BinaryQubo instances ready
for PCESolver.  All encodings are penalty-method formulations: constraint
violations are penalised quadratically so the QUBO minimum coincides with
the feasible optimum.

Supported problems
------------------
- MaxCut          — maximise edge cuts in a weighted graph
- TSP             — Travelling Salesman Problem (n cities → n² variables)
- Portfolio       — Markowitz mean-variance portfolio selection

References
----------
- Lucas, A. (2014). Ising formulations of many NP problems. Frontiers in
  Physics, 2, 5.  https://doi.org/10.3389/fphy.2014.00005
- Glover, F. et al. (2019). Quantum Bridge Analytics I: a tutorial on
  formulating and using QUBO models. 4OR, 17, 335–371.
"""

from __future__ import annotations

from typing import Sequence

import numpy as np

from ._qubo import BinaryQubo

try:
    import networkx as _nx
    _HAS_NX = True
except ImportError:
    _HAS_NX = False


# ---------------------------------------------------------------------------
# MaxCut
# ---------------------------------------------------------------------------

def qubo_from_maxcut(
    graph,
    *,
    weight: str = "weight",
    default_weight: float = 1.0,
) -> BinaryQubo:
    """Encode a MaxCut problem as a QUBO.

    MaxCut: partition vertices into two sets S, S̄ to maximise
    Σ_{(i,j)∈E} w_{ij} · (x_i ⊕ x_j).

    QUBO form (minimisation)::

        C(x) = -Σ_{(i,j)∈E} w_{ij} · (x_i + x_j - 2·x_i·x_j)

    so the minimum of C equals the negative of the maximum cut weight.

    Args:
        graph:          Weighted graph as one of:
                          - dict {(i, j): weight}  (0-indexed node IDs)
                          - np.ndarray  (symmetric n×n adjacency matrix)
                          - networkx.Graph  (if networkx is installed)
        weight:         NetworkX edge attribute name for weights (default "weight").
        default_weight: Weight used when an edge has no weight attribute.

    Returns:
        BinaryQubo with n = number of nodes.

    Example::

        edges = {(0, 1): 1.0, (1, 2): 1.0, (2, 3): 1.0, (3, 0): 1.0, (0, 2): 0.5}
        qubo = qubo_from_maxcut(edges, n_nodes=4)   # 4-node cycle + diagonal
        result = PCESolver(qubo).solve()
    """
    A = _graph_to_adjacency(graph, weight=weight, default_weight=default_weight)
    n = A.shape[0]

    linear: dict[int, float] = {}
    quadratic: dict[tuple[int, int], float] = {}

    for i in range(n):
        for j in range(i + 1, n):
            w = A[i, j]
            if w == 0.0:
                continue
            # -w·x_i  -w·x_j  +2w·x_i·x_j
            linear[i] = linear.get(i, 0.0) - w
            linear[j] = linear.get(j, 0.0) - w
            quadratic[(i, j)] = quadratic.get((i, j), 0.0) + 2.0 * w

    # Drop zero linear terms.
    linear = {k: v for k, v in linear.items() if v != 0.0}
    return BinaryQubo.from_dict(n, linear=linear, quadratic=quadratic)


# ---------------------------------------------------------------------------
# TSP
# ---------------------------------------------------------------------------

def qubo_from_tsp(
    distances: np.ndarray | Sequence[Sequence[float]],
    *,
    penalty: float | None = None,
) -> BinaryQubo:
    """Encode a Travelling Salesman Problem as a QUBO.

    Uses the standard one-hot time-step encoding (Lucas 2014 §3.1).

    Variables: x_{i,t} = 1 if city i is visited at time step t.
    Variable index: i·n + t  (n = number of cities).

    Total variables: n².

    Constraints (both penalised with coefficient ``penalty``):

    1. Each city visited exactly once:
       Σ_t x_{i,t} = 1  for all i.

    2. Each time step has exactly one city:
       Σ_i x_{i,t} = 1  for all t.

    Objective (route length, coefficient 1.0):
       Σ_{i,j,t} d_{ij} · x_{i,t} · x_{j,(t+1) mod n}

    Args:
        distances: n×n symmetric distance matrix (d[i,i] ignored).
        penalty:   Constraint penalty.  If None, defaults to
                   ``max(distances) * n`` — large enough to dominate
                   any feasible route cost.

    Returns:
        BinaryQubo with n² variables.  Variable (i, t) → index i·n + t.

    Example::

        D = np.array([[0,1,2],[1,0,1],[2,1,0]], dtype=float)
        qubo = qubo_from_tsp(D)
        result = PCESolver(qubo, encoding="dense", shots=2048).solve()
        # Decode: x[i*n+t] → city i at time t
    """
    D = np.asarray(distances, dtype=float)
    if D.ndim != 2 or D.shape[0] != D.shape[1]:
        raise ValueError("distances must be a square 2-D array")
    n = D.shape[0]
    if n < 2:
        raise ValueError("TSP requires at least 2 cities")

    if penalty is None:
        penalty = float(np.max(D)) * n

    n_vars = n * n
    linear: dict[int, float] = {}
    quadratic: dict[tuple[int, int], float] = {}

    def idx(city: int, time: int) -> int:
        return city * n + time

    def add_linear(i: int, v: float) -> None:
        linear[i] = linear.get(i, 0.0) + v

    def add_quad(i: int, j: int, v: float) -> None:
        a, b = (i, j) if i < j else (j, i)
        if a == b:
            add_linear(a, v)
            return
        quadratic[(a, b)] = quadratic.get((a, b), 0.0) + v

    # Constraint 1: Σ_t x_{i,t} = 1  for each city i.
    # Penalty: A · (Σ_t x_{i,t} - 1)²
    # Expansion: A · (-Σ_t x_{i,t} + 2·Σ_{t<t'} x_{i,t}·x_{i,t'} + const)
    for i in range(n):
        for t in range(n):
            add_linear(idx(i, t), -penalty)
        for t in range(n):
            for tp in range(t + 1, n):
                add_quad(idx(i, t), idx(i, tp), 2.0 * penalty)

    # Constraint 2: Σ_i x_{i,t} = 1  for each time step t.
    for t in range(n):
        for i in range(n):
            add_linear(idx(i, t), -penalty)
        for i in range(n):
            for ip in range(i + 1, n):
                add_quad(idx(i, t), idx(ip, t), 2.0 * penalty)

    # Objective: Σ_{i≠j, t} d_{ij} · x_{i,t} · x_{j,(t+1) mod n}
    for t in range(n):
        t_next = (t + 1) % n
        for i in range(n):
            for j in range(n):
                if i == j:
                    continue
                d = D[i, j]
                if d == 0.0:
                    continue
                add_quad(idx(i, t), idx(j, t_next), d)

    linear = {k: v for k, v in linear.items() if v != 0.0}
    return BinaryQubo.from_dict(n_vars, linear=linear, quadratic=quadratic)


def decode_tsp(solution: list[bool], n_cities: int) -> list[int] | None:
    """Decode a TSP QUBO solution back to a city tour.

    Args:
        solution:  Binary assignment from PCESolver (length n_cities²).
        n_cities:  Number of cities.

    Returns:
        List of city indices in visit order, or None if the solution is
        infeasible (constraint violated — more or fewer than one city per
        time step).

    Example::

        result = PCESolver(qubo).solve()
        tour = decode_tsp(result.solution, n_cities=4)
        if tour:
            print("Tour:", " → ".join(str(c) for c in tour))
    """
    n = n_cities
    if len(solution) != n * n:
        raise ValueError(f"Expected {n*n} variables, got {len(solution)}")

    x = np.array(solution, dtype=int).reshape(n, n)   # x[city, time]
    tour: list[int] = []
    for t in range(n):
        col = x[:, t]
        cities_at_t = np.where(col == 1)[0]
        if len(cities_at_t) != 1:
            return None  # infeasible
        tour.append(int(cities_at_t[0]))

    if len(set(tour)) != n:
        return None  # duplicate city — infeasible
    return tour


def tsp_tour_length(tour: list[int], distances: np.ndarray) -> float:
    """Compute total length of a TSP tour (including return to start)."""
    D = np.asarray(distances, dtype=float)
    n = len(tour)
    return float(sum(D[tour[t], tour[(t + 1) % n]] for t in range(n)))


# ---------------------------------------------------------------------------
# Portfolio optimisation
# ---------------------------------------------------------------------------

def qubo_from_portfolio(
    returns: np.ndarray | Sequence[float],
    covariance: np.ndarray | Sequence[Sequence[float]],
    *,
    risk_factor: float = 1.0,
    budget: int | None = None,
    budget_penalty: float | None = None,
) -> BinaryQubo:
    """Encode a Markowitz portfolio optimisation problem as a QUBO.

    Objective (minimise)::

        C(x) = -Σ_i r_i·x_i  +  risk_factor · Σ_{i,j} σ_{ij}·x_i·x_j

    where r_i is the expected return of asset i and σ_{ij} is the
    covariance between assets i and j.

    Optional budget constraint (select exactly ``budget`` assets)::

        budget_penalty · (Σ_i x_i - budget)²

    Args:
        returns:         (n,) expected return for each asset.
        covariance:      (n, n) covariance matrix (symmetric, PSD).
        risk_factor:     Weight of the risk (variance) term vs. return.
                         Higher values prefer lower-risk portfolios.
                         Default 1.0.
        budget:          If given, penalise solutions that select ≠ budget
                         assets.  E.g. budget=5 selects exactly 5 assets.
        budget_penalty:  Penalty coefficient for budget constraint.
                         Defaults to ``max(|r_i|) * n`` when budget is set.

    Returns:
        BinaryQubo with n = number of assets.

    Example::

        import numpy as np
        r = np.array([0.10, 0.12, 0.08, 0.15])
        cov = np.diag([0.02, 0.03, 0.015, 0.04])
        qubo = qubo_from_portfolio(r, cov, risk_factor=2.0, budget=2)
        result = PCESolver(qubo, encoding="dense", shots=1024).solve()
        selected = [i for i, x in enumerate(result.solution) if x]
    """
    r = np.asarray(returns, dtype=float)
    Σ = np.asarray(covariance, dtype=float)
    n = len(r)

    if Σ.shape != (n, n):
        raise ValueError(
            f"covariance must be ({n}, {n}), got {Σ.shape}"
        )

    linear: dict[int, float] = {}
    quadratic: dict[tuple[int, int], float] = {}

    # Return term: -r_i·x_i
    # Risk diagonal: risk_factor · σ_{ii} · x_i  (since x_i² = x_i)
    for i in range(n):
        v = -r[i] + risk_factor * Σ[i, i]
        if v != 0.0:
            linear[i] = v

    # Risk off-diagonal: risk_factor · (σ_{ij} + σ_{ji}) · x_i·x_j  (i < j)
    for i in range(n):
        for j in range(i + 1, n):
            v = risk_factor * (Σ[i, j] + Σ[j, i])
            if v != 0.0:
                quadratic[(i, j)] = quadratic.get((i, j), 0.0) + v

    # Budget constraint: budget_penalty · (Σ_i x_i - budget)²
    if budget is not None:
        if budget_penalty is None:
            budget_penalty = float(np.max(np.abs(r))) * n
        A = budget_penalty
        # Expansion of (Σ x_i - B)²:
        #   = Σ_i x_i² - 2B·Σ_i x_i + 2·Σ_{i<j} x_i·x_j + B²
        #   = Σ_i x_i·(1 - 2B) + 2·Σ_{i<j} x_i·x_j  + const
        for i in range(n):
            linear[i] = linear.get(i, 0.0) + A * (1.0 - 2.0 * budget)
        for i in range(n):
            for j in range(i + 1, n):
                quadratic[(i, j)] = quadratic.get((i, j), 0.0) + 2.0 * A

    linear = {k: v for k, v in linear.items() if v != 0.0}
    return BinaryQubo.from_dict(n, linear=linear, quadratic=quadratic)


# ---------------------------------------------------------------------------
# Shared utility
# ---------------------------------------------------------------------------

def _graph_to_adjacency(
    graph,
    *,
    weight: str = "weight",
    default_weight: float = 1.0,
) -> np.ndarray:
    """Convert any supported graph input to a dense adjacency matrix."""
    if _HAS_NX and isinstance(graph, _nx.Graph):
        return _nx.to_numpy_array(graph, weight=weight, nonedge=0.0)

    if isinstance(graph, dict):
        nodes: set[int] = set()
        for i, j in graph.keys():
            nodes.add(i)
            nodes.add(j)
        n = max(nodes) + 1 if nodes else 0
        A = np.zeros((n, n), dtype=float)
        for (i, j), w in graph.items():
            A[i, j] += w
            A[j, i] += w
        return A

    A = np.asarray(graph, dtype=float)
    if A.ndim != 2 or A.shape[0] != A.shape[1]:
        raise ValueError("graph adjacency must be a square 2-D array")
    return (A + A.T) / 2.0
