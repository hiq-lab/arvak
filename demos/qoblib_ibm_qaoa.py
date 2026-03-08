"""QOBLIB MIS on IBM Quantum — 123-qubit QAOA on real hardware.

Runs QAOA p=1 on the es60fst01 MIS instance (123 variables, 159 edges)
from the QOBLIB benchmark library on an IBM Heron processor, using Arvak's
built-in QAOASolver + HalBackend pipeline.

Usage:
    export IBM_API_KEY=...
    export IBM_SERVICE_CRN=...
    python demos/qoblib_ibm_qaoa.py
    python demos/qoblib_ibm_qaoa.py --backend ibm_torino --shots 4096
"""

from __future__ import annotations

import json
import time
from pathlib import Path

import numpy as np

from arvak.optimize import BinaryQubo, QAOASolver
from arvak.optimize._backend import HalBackend
from arvak.optimize._qaoa import _eval_qubo


def _load_qs_qubo(qs_path: Path):
    """Load a .qs QUBO file into a numpy matrix."""
    entries = []
    max_idx = 0
    with open(qs_path) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split()
            if len(parts) != 3:
                continue
            try:
                r, c, v = int(parts[0]), int(parts[1]), float(parts[2])
            except ValueError:
                continue
            entries.append((r, c, v))
            max_idx = max(max_idx, r, c)
    dim = max_idx
    q = np.zeros((dim, dim), dtype=float)
    for r, c, v in entries:
        q[r - 1, c - 1] = v
        if r != c:
            q[c - 1, r - 1] = v
    return q


def main():
    import argparse
    import os

    parser = argparse.ArgumentParser(description="QOBLIB 123-qubit QAOA on IBM")
    parser.add_argument("--backend", default="ibm_torino")
    parser.add_argument("--shots", type=int, default=2048)
    parser.add_argument("--max-iter", type=int, default=80)
    args = parser.parse_args()

    # --- Load instance ---
    qs_path = Path(__file__).parent / "data" / "qoblib" / "mis" / "es60fst01.qs"
    q = _load_qs_qubo(qs_path)
    qubo = BinaryQubo.from_matrix(q)
    n = qubo.n
    print(f"=== QOBLIB MIS es60fst01 — {n}-qubit QAOA p=1 on {args.backend} ===")
    print(f"Variables: {n}, Edges: {len(qubo.quadratic)}")

    # --- Connect to IBM via Arvak HalBackend ---
    print(f"\nConnecting to {args.backend}...")
    backend = HalBackend.ibm(args.backend)
    print(f"  Backend: {backend}")

    # --- Run QAOA ---
    print(f"\nRunning QAOASolver(p=1, shots={args.shots}, max_iter={args.max_iter})...")
    print("  This submits ~80+ circuits to IBM. Estimated time: 30-60 min.\n")

    t0 = time.monotonic()
    solver = QAOASolver(
        qubo,
        p=1,
        shots=args.shots,
        backend=backend,
        max_iter=args.max_iter,
        seed=42,
        cvar_top=0.1,
    )
    result = solver.solve()
    total_time = time.monotonic() - t0

    # --- Results ---
    bs = "".join("1" if b else "0" for b in result.solution)
    ones = bs.count("1")
    print(f"\n{'='*60}")
    print(f"RESULTS — es60fst01 MIS, {n} qubits, QAOA p=1")
    print(f"{'='*60}")
    print(f"Backend: {args.backend}")
    print(f"Best energy: {result.cost:.1f}")
    print(f"Independent set size: {ones}")
    print(f"Gamma: {result.gamma}")
    print(f"Beta: {result.beta}")
    print(f"COBYLA iterations: {result.n_iters}")
    print(f"Converged: {result.converged}")
    print(f"Total wall time: {total_time:.1f}s")

    print(f"\nTop solutions:")
    for i, (sol, cost) in enumerate(result.top_solutions[:5]):
        sol_bs = "".join("1" if b else "0" for b in sol)
        print(f"  #{i+1}: energy={cost:.1f}, |1|={sol_bs.count('1')}")

    # --- Save ---
    output = {
        "instance": "es60fst01",
        "problem": "mis",
        "n_variables": n,
        "n_edges": len(qubo.quadratic),
        "backend": args.backend,
        "qaoa_p": 1,
        "shots": args.shots,
        "best_energy": result.cost,
        "best_bitstring": bs,
        "independent_set_size": ones,
        "gamma": result.gamma,
        "beta": result.beta,
        "n_iters": result.n_iters,
        "converged": result.converged,
        "wall_time_seconds": total_time,
        "top_10": [
            {"energy": cost, "ones": "".join("1" if b else "0" for b in sol).count("1")}
            for sol, cost in result.top_solutions[:10]
        ],
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }
    out_path = Path(__file__).parent / "data" / "qoblib_ibm_qaoa_results.json"
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w") as f:
        json.dump(output, f, indent=2, default=str)
    print(f"\nResults saved to {out_path}")


if __name__ == "__main__":
    main()
