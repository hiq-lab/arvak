"""QOBLIB Benchmark Runner — solve standardized instances with arvak.optimize.

Runs Portfolio, LABS, Market Split, and MIS instances from the Quantum
Optimization Benchmark Library (QOBLIB, arXiv:2504.03832) through Arvak's
QAOA, VQE, and PCE solvers. Compares quantum results against brute-force
classical optimal where feasible.

Usage:
    python demos/qoblib_benchmark.py                    # all instances, simulator
    python demos/qoblib_benchmark.py --problem portfolio
    python demos/qoblib_benchmark.py --problem labs --instance 5
    python demos/qoblib_benchmark.py --problem marketsplit
    python demos/qoblib_benchmark.py --problem mis --instance karate
    python demos/qoblib_benchmark.py --backend ibm_torino --shots 4096

Results are saved to demos/data/qoblib_results.json for potential submission
to the QOBLIB benchmark repository at git.zib.de/qopt/qoblib.
"""

from __future__ import annotations

import argparse
import json
import logging
import sys
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path

import numpy as np

# Arvak imports
from arvak.optimize import BinaryQubo, PCESolver, QAOASolver

logging.basicConfig(level=logging.INFO, format="%(message)s")
log = logging.getLogger(__name__)

# --- Data paths ---
# Primary: Garm project QOBLIB data (already downloaded)
_GARM_QOBLIB = Path.home() / "Projects/Garm-Platform/code/services/shadow-etf/data/qoblib"
# Fallback: local copy under demos/data/qoblib/
_LOCAL_QOBLIB = Path(__file__).parent / "data" / "qoblib"


def _data_dir() -> Path:
    """Find the QOBLIB data directory."""
    if _GARM_QOBLIB.exists():
        return _GARM_QOBLIB
    if _LOCAL_QOBLIB.exists():
        return _LOCAL_QOBLIB
    raise FileNotFoundError(
        f"QOBLIB data not found at {_GARM_QOBLIB} or {_LOCAL_QOBLIB}.\n"
        "Clone from: git clone https://git.zib.de/qopt/qoblib-quantum-optimization-benchmarking-library"
    )


# --- Data loaders ---


def load_labs_qubo(n: int) -> tuple[np.ndarray, dict]:
    """Load a LABS QUBO instance for sequence length n.

    Returns (Q matrix as ndarray, metadata dict).
    """
    qs_path = _data_dir() / "labs" / f"labs{n:03d}.qs"
    if not qs_path.exists():
        available = sorted(
            int(p.stem.replace("labs", ""))
            for p in (_data_dir() / "labs").glob("labs*.qs")
        )
        raise FileNotFoundError(f"LABS n={n} not found. Available: {available}")

    q, obj_offset = load_qs_qubo(qs_path)
    metadata = {
        "source": "qoblib",
        "problem": "labs",
        "sequence_length": n,
        "qubo_variables": q.shape[0],
        "objective_offset": obj_offset,
    }
    return q, metadata


def load_qs_qubo(qs_path: Path) -> tuple[np.ndarray, float]:
    """Load any .qs file into a QUBO matrix.

    Returns (Q matrix, objective_offset).
    """
    obj_offset = 0.0
    entries: list[tuple[int, int, float]] = []
    max_idx = 0

    with open(qs_path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            if line.startswith("# ObjectiveOffset"):
                obj_offset = float(line.split()[-1])
                continue
            if line.startswith("#"):
                continue
            parts = line.split()
            if len(parts) == 2 and parts[0].isdigit():
                continue  # "Vars Non-zeros" header
            if len(parts) != 3:
                continue
            try:
                r, c, v = int(parts[0]), int(parts[1]), float(parts[2])
            except ValueError:
                continue
            entries.append((r, c, v))
            max_idx = max(max_idx, r, c)

    dim = max_idx  # 1-indexed
    q = np.zeros((dim, dim), dtype=float)
    for r, c, v in entries:
        q[r - 1, c - 1] = v
        if r != c:
            q[c - 1, r - 1] = v

    return q, obj_offset


def load_marketsplit_qubo(instance_name: str) -> tuple[np.ndarray, dict]:
    """Load a Market Split QUBO instance.

    Instance names like 'ms_03_050_002' (size=3, vars=50, seed=002).
    """
    qs_path = _LOCAL_QOBLIB / "marketsplit" / f"{instance_name}.qs"
    if not qs_path.exists():
        ms_dir = _LOCAL_QOBLIB / "marketsplit"
        available = sorted(p.stem for p in ms_dir.glob("*.qs")) if ms_dir.exists() else []
        raise FileNotFoundError(f"Market Split '{instance_name}' not found. Available: {available[:10]}...")

    q, obj_offset = load_qs_qubo(qs_path)
    metadata = {
        "source": "qoblib",
        "problem": "marketsplit",
        "instance": instance_name,
        "qubo_variables": q.shape[0],
        "objective_offset": obj_offset,
    }
    return q, metadata


def load_mis_qubo(instance_name: str) -> tuple[np.ndarray, dict]:
    """Load a Maximum Independent Set QUBO instance.

    Instance names like 'karate', 'farm', 'chesapeake', 'football'.
    """
    qs_path = _LOCAL_QOBLIB / "mis" / f"{instance_name}.qs"
    if not qs_path.exists():
        mis_dir = _LOCAL_QOBLIB / "mis"
        available = sorted(p.stem for p in mis_dir.glob("*.qs")) if mis_dir.exists() else []
        raise FileNotFoundError(f"MIS '{instance_name}' not found. Available: {available[:10]}...")

    q, obj_offset = load_qs_qubo(qs_path)
    metadata = {
        "source": "qoblib",
        "problem": "mis",
        "instance": instance_name,
        "qubo_variables": q.shape[0],
        "objective_offset": obj_offset,
    }
    return q, metadata


def load_portfolio_qubo(
    num_assets: int = 10,
    num_periods: int = 10,
    risk_aversion: float = 0.001,
) -> tuple[np.ndarray, dict]:
    """Load a QOBLIB portfolio instance and build QUBO matrix.

    Returns (Q matrix as ndarray, metadata dict with asset names).
    """
    instance_name = f"po_a{num_assets:03d}_t{num_periods:02d}_orig"
    instance_dir = _data_dir() / "portfolio" / instance_name
    if not instance_dir.exists():
        available = sorted(
            p.name for p in (_data_dir() / "portfolio").iterdir() if p.is_dir()
        )
        raise FileNotFoundError(
            f"Portfolio instance '{instance_name}' not found. Available: {available}"
        )

    # Load stock prices
    prices: dict[str, list[float]] = {}
    with open(instance_dir / "stock_prices.txt") as f:
        for line in f:
            parts = line.strip().split()
            if len(parts) != 3:
                continue
            _t, asset, price = int(parts[0]), parts[1], float(parts[2])
            if asset not in prices:
                prices[asset] = []
            prices[asset].append(price)

    assets = list(prices.keys())
    n = len(assets)

    # Expected returns
    expected_returns = []
    for asset in assets:
        p = prices[asset]
        if len(p) < 2:
            expected_returns.append(0.0)
            continue
        rets = [(p[t + 1] - p[t]) / p[t] for t in range(len(p) - 1)]
        expected_returns.append(sum(rets) / len(rets))

    # Load covariance matrix (period 0)
    cov: dict[tuple[str, str], float] = {}
    with open(instance_dir / "covariance_matrices.txt") as f:
        for line in f:
            parts = line.strip().split()
            if len(parts) != 4:
                continue
            t, a1, a2, val = int(parts[0]), parts[1], parts[2], float(parts[3])
            if t == 0:
                cov[(a1, a2)] = val

    # Build QUBO: min risk_aversion * x^T Sigma x - mu^T x
    q = np.zeros((n, n), dtype=float)
    for i in range(n):
        for j in range(n):
            sigma_ij = cov.get((assets[i], assets[j]), 0.0)
            q[i, j] = risk_aversion * sigma_ij
        q[i, i] -= expected_returns[i]

    metadata = {
        "source": "qoblib",
        "problem": "portfolio",
        "instance": instance_name,
        "num_assets": n,
        "num_periods": num_periods,
        "assets": assets,
        "risk_aversion": risk_aversion,
        "expected_returns": {a: r for a, r in zip(assets, expected_returns)},
    }
    return q, metadata


# --- Solvers ---


def brute_force_optimal(
    q: np.ndarray,
    target_ones: int | None = None,
) -> tuple[str, float]:
    """Find optimal bitstring by enumeration. Feasible for n <= 20."""
    n = q.shape[0]
    if n > 20:
        raise ValueError(f"Brute-force infeasible for n={n} (limit 20)")

    best_bs = "0" * n
    best_energy = float("inf")

    for x in range(2**n):
        bs = format(x, f"0{n}b")
        if target_ones is not None and bs.count("1") != target_ones:
            continue
        vec = np.array([int(b) for b in bs], dtype=float)
        energy = float(vec @ q @ vec)
        if energy < best_energy:
            best_energy = energy
            best_bs = bs

    return best_bs, best_energy


def evaluate_qubo(q: np.ndarray, bitstring: str) -> float:
    """Evaluate x^T Q x for a given bitstring."""
    vec = np.array([int(b) for b in bitstring], dtype=float)
    return float(vec @ q @ vec)


def _approx_ratio(classical_energy: float | None, quantum_energy: float) -> float:
    """Compute approximation ratio (closer to 1.0 = better).

    For minimization: ratio = classical / quantum.
    If both are zero, ratio = 1.0 (perfect match).
    """
    if classical_energy is None:
        return float("nan")
    if classical_energy == 0 and quantum_energy == 0:
        return 1.0
    if quantum_energy == 0:
        return float("nan")
    return classical_energy / quantum_energy


# --- Result dataclass ---


@dataclass
class BenchmarkResult:
    problem: str
    instance: str
    n_variables: int
    solver: str
    best_bitstring: str
    best_energy: float
    classical_optimal_energy: float
    approximation_ratio: float
    wall_time_seconds: float
    solver_params: dict = field(default_factory=dict)
    metadata: dict = field(default_factory=dict)


# --- Benchmark runner ---


def run_portfolio_benchmark(
    num_assets: int = 10,
    num_periods: int = 10,
    target_positions: int = 5,
    shots: int = 1024,
    seed: int = 42,
) -> list[BenchmarkResult]:
    """Run portfolio benchmark with all solvers."""
    q, meta = load_portfolio_qubo(num_assets, num_periods)
    n = q.shape[0]
    instance = meta["instance"]

    log.info("=" * 60)
    log.info("QOBLIB Portfolio: %s (%d assets, %d periods)", instance, n, num_periods)
    log.info("Assets: %s", ", ".join(meta["assets"]))
    log.info("=" * 60)

    # Classical optimal
    t0 = time.monotonic()
    classical_bs, classical_energy = brute_force_optimal(q, target_ones=target_positions)
    classical_time = time.monotonic() - t0
    classical_assets = [meta["assets"][i] for i, b in enumerate(classical_bs) if b == "1"]
    log.info(
        "Classical optimal: %s (energy=%.8f, assets=%s) [%.2fs]",
        classical_bs, classical_energy, ", ".join(classical_assets), classical_time,
    )

    results: list[BenchmarkResult] = []
    qubo = BinaryQubo.from_matrix(q)

    # --- QAOA ---
    for p in [1, 2, 3]:
        log.info("\nQAOA p=%d, shots=%d...", p, shots)
        t0 = time.monotonic()
        try:
            qaoa = QAOASolver(qubo, p=p, shots=shots, seed=seed, max_iter=300, cvar_top=0.1)
            result = qaoa.solve()
            wall_time = time.monotonic() - t0

            bs = "".join("1" if b else "0" for b in result.solution)
            energy = result.cost
            ratio = _approx_ratio(classical_energy, energy)

            log.info(
                "  QAOA p=%d: %s (energy=%.8f, ratio=%.4f) [%.2fs]",
                p, bs, energy, ratio, wall_time,
            )

            results.append(BenchmarkResult(
                problem="portfolio",
                instance=instance,
                n_variables=n,
                solver=f"qaoa_p{p}",
                best_bitstring=bs,
                best_energy=energy,
                classical_optimal_energy=classical_energy,
                approximation_ratio=ratio,
                wall_time_seconds=wall_time,
                solver_params={"p": p, "shots": shots, "seed": seed, "cvar_top": 0.1},
                metadata={"assets": meta["assets"], "target_positions": target_positions},
            ))
        except Exception as e:
            log.error("  QAOA p=%d failed: %s", p, e)

    # --- PCE ---
    for encoding in ["dense", "poly"]:
        log.info("\nPCE encoding=%s, shots=%d...", encoding, shots)
        t0 = time.monotonic()
        try:
            pce = PCESolver(
                qubo, encoding=encoding, shots=shots, seed=seed,
                max_iter=300, alpha=2.0, cvar_top=0.1,
            )
            result = pce.solve()
            wall_time = time.monotonic() - t0

            bs = "".join("1" if b else "0" for b in result.solution)
            energy = result.cost
            ratio = _approx_ratio(classical_energy, energy)

            log.info(
                "  PCE %s: %s (energy=%.8f, ratio=%.4f, %dq→%dq) [%.2fs]",
                encoding, bs, energy, ratio, n, result.n_qubits, wall_time,
            )

            results.append(BenchmarkResult(
                problem="portfolio",
                instance=instance,
                n_variables=n,
                solver=f"pce_{encoding}",
                best_bitstring=bs,
                best_energy=energy,
                classical_optimal_energy=classical_energy,
                approximation_ratio=ratio,
                wall_time_seconds=wall_time,
                solver_params={
                    "encoding": encoding, "shots": shots, "seed": seed,
                    "n_qubits": result.n_qubits, "compression_ratio": result.compression_ratio,
                },
                metadata={"assets": meta["assets"], "target_positions": target_positions},
            ))
        except Exception as e:
            log.error("  PCE %s failed: %s", encoding, e)

    return results


def run_labs_benchmark(
    n: int = 5,
    shots: int = 2048,
    seed: int = 42,
) -> list[BenchmarkResult]:
    """Run LABS benchmark with all solvers."""
    q, meta = load_labs_qubo(n)
    dim = q.shape[0]
    offset = meta["objective_offset"]

    log.info("=" * 60)
    log.info("QOBLIB LABS: n=%d (QUBO vars=%d, offset=%.1f)", n, dim, offset)
    log.info("=" * 60)

    results: list[BenchmarkResult] = []

    # Classical optimal (only if feasible)
    classical_bs, classical_energy = None, None
    if dim <= 20:
        t0 = time.monotonic()
        classical_bs, classical_energy = brute_force_optimal(q)
        classical_time = time.monotonic() - t0
        log.info(
            "Classical optimal: %s (energy=%.2f, +offset=%.2f) [%.2fs]",
            classical_bs, classical_energy, classical_energy + offset, classical_time,
        )
    else:
        log.info("Brute-force skipped (dim=%d > 20)", dim)

    qubo = BinaryQubo.from_matrix(q)

    # --- QAOA --- (limit depth for large instances)
    max_p = 2 if dim <= 15 else 1
    for p in range(1, max_p + 1):
        log.info("\nQAOA p=%d, shots=%d...", p, shots)
        t0 = time.monotonic()
        try:
            qaoa = QAOASolver(qubo, p=p, shots=shots, seed=seed, max_iter=300, cvar_top=0.1)
            result = qaoa.solve()
            wall_time = time.monotonic() - t0

            bs = "".join("1" if b else "0" for b in result.solution)
            energy = result.cost

            ratio = _approx_ratio(classical_energy, energy)

            log.info(
                "  QAOA p=%d: energy=%.2f (+offset=%.2f, ratio=%.4f) [%.2fs]",
                p, energy, energy + offset, ratio, wall_time,
            )

            results.append(BenchmarkResult(
                problem="labs",
                instance=f"labs{n:03d}",
                n_variables=dim,
                solver=f"qaoa_p{p}",
                best_bitstring=bs,
                best_energy=energy,
                classical_optimal_energy=classical_energy or 0.0,
                approximation_ratio=ratio,
                wall_time_seconds=wall_time,
                solver_params={"p": p, "shots": shots, "seed": seed},
                metadata={"sequence_length": n, "objective_offset": offset},
            ))
        except Exception as e:
            log.error("  QAOA p=%d failed: %s", p, e)

    # --- PCE (only for small instances) ---
    if dim <= 20:
        for encoding in ["dense", "poly"]:
            log.info("\nPCE encoding=%s, shots=%d...", encoding, shots)
            t0 = time.monotonic()
            try:
                pce = PCESolver(
                    qubo, encoding=encoding, shots=shots, seed=seed,
                    max_iter=300, alpha=2.0, cvar_top=0.1,
                )
                result = pce.solve()
                wall_time = time.monotonic() - t0

                bs = "".join("1" if b else "0" for b in result.solution)
                energy = result.cost

                ratio = _approx_ratio(classical_energy, energy)

                log.info(
                    "  PCE %s: energy=%.2f (+offset=%.2f, ratio=%.4f, %dq→%dq) [%.2fs]",
                    encoding, energy, energy + offset, ratio, dim, result.n_qubits, wall_time,
                )

                results.append(BenchmarkResult(
                    problem="labs",
                    instance=f"labs{n:03d}",
                    n_variables=dim,
                    solver=f"pce_{encoding}",
                    best_bitstring=bs,
                    best_energy=energy,
                    classical_optimal_energy=classical_energy or 0.0,
                    approximation_ratio=ratio,
                    wall_time_seconds=wall_time,
                    solver_params={
                        "encoding": encoding, "shots": shots, "seed": seed,
                        "n_qubits": result.n_qubits,
                        "compression_ratio": result.compression_ratio,
                    },
                    metadata={"sequence_length": n, "objective_offset": offset},
                ))
            except Exception as e:
                log.error("  PCE %s failed: %s", encoding, e)

    return results


def _run_generic_qubo_benchmark(
    problem: str,
    instance_name: str,
    q: np.ndarray,
    meta: dict,
    shots: int = 1024,
    seed: int = 42,
    max_qubo_for_brute: int = 20,
) -> list[BenchmarkResult]:
    """Run PCE benchmark on any QUBO problem. QAOA only if n <= 25."""
    n = q.shape[0]

    log.info("=" * 60)
    log.info("QOBLIB %s: %s (%d variables)", problem.upper(), instance_name, n)
    log.info("=" * 60)

    # Classical optimal (only if tiny)
    classical_energy = None
    if n <= max_qubo_for_brute:
        t0 = time.monotonic()
        classical_bs, classical_energy = brute_force_optimal(q)
        classical_time = time.monotonic() - t0
        log.info("Classical optimal: energy=%.4f [%.2fs]", classical_energy, classical_time)
    else:
        log.info("Brute-force skipped (n=%d > %d)", n, max_qubo_for_brute)

    results: list[BenchmarkResult] = []
    qubo = BinaryQubo.from_matrix(q)

    # --- PCE (works for any size via compression) ---
    for encoding in ["dense", "poly"]:
        log.info("\nPCE encoding=%s, shots=%d...", encoding, shots)
        t0 = time.monotonic()
        try:
            pce = PCESolver(
                qubo, encoding=encoding, shots=shots, seed=seed,
                max_iter=300, alpha=2.0, cvar_top=0.1,
            )
            result = pce.solve()
            wall_time = time.monotonic() - t0

            bs = "".join("1" if b else "0" for b in result.solution)
            energy = result.cost
            ratio = _approx_ratio(classical_energy, energy)

            log.info(
                "  PCE %s: energy=%.4f (ratio=%s, %dq→%dq) [%.2fs]",
                encoding, energy,
                f"{ratio:.4f}" if not np.isnan(ratio) else "N/A",
                n, result.n_qubits, wall_time,
            )

            results.append(BenchmarkResult(
                problem=problem,
                instance=instance_name,
                n_variables=n,
                solver=f"pce_{encoding}",
                best_bitstring=bs,
                best_energy=energy,
                classical_optimal_energy=classical_energy or 0.0,
                approximation_ratio=ratio,
                wall_time_seconds=wall_time,
                solver_params={
                    "encoding": encoding, "shots": shots, "seed": seed,
                    "n_qubits": result.n_qubits,
                    "compression_ratio": result.compression_ratio,
                },
                metadata=meta,
            ))
        except Exception as e:
            log.error("  PCE %s failed: %s", encoding, e)

    # --- QAOA (only for small instances) ---
    if n <= 25:
        for p in [1, 2]:
            log.info("\nQAOA p=%d, shots=%d...", p, shots)
            t0 = time.monotonic()
            try:
                qaoa = QAOASolver(qubo, p=p, shots=shots, seed=seed, max_iter=300, cvar_top=0.1)
                result = qaoa.solve()
                wall_time = time.monotonic() - t0

                bs = "".join("1" if b else "0" for b in result.solution)
                energy = result.cost
                ratio = _approx_ratio(classical_energy, energy)

                log.info(
                    "  QAOA p=%d: energy=%.4f (ratio=%s) [%.2fs]",
                    p, energy,
                    f"{ratio:.4f}" if not np.isnan(ratio) else "N/A",
                    wall_time,
                )

                results.append(BenchmarkResult(
                    problem=problem,
                    instance=instance_name,
                    n_variables=n,
                    solver=f"qaoa_p{p}",
                    best_bitstring=bs,
                    best_energy=energy,
                    classical_optimal_energy=classical_energy or 0.0,
                    approximation_ratio=ratio,
                    wall_time_seconds=wall_time,
                    solver_params={"p": p, "shots": shots, "seed": seed},
                    metadata=meta,
                ))
            except Exception as e:
                log.error("  QAOA p=%d failed: %s", p, e)

    return results


def run_marketsplit_benchmark(
    instance_name: str = "ms_03_050_002",
    shots: int = 1024,
    seed: int = 42,
) -> list[BenchmarkResult]:
    """Run Market Split benchmark."""
    q, meta = load_marketsplit_qubo(instance_name)
    return _run_generic_qubo_benchmark("marketsplit", instance_name, q, meta, shots, seed)


def run_mis_benchmark(
    instance_name: str = "farm",
    shots: int = 1024,
    seed: int = 42,
) -> list[BenchmarkResult]:
    """Run Maximum Independent Set benchmark."""
    q, meta = load_mis_qubo(instance_name)
    return _run_generic_qubo_benchmark("mis", instance_name, q, meta, shots, seed)


# --- Available instances ---


def list_instances() -> dict:
    """List all available QOBLIB instances."""
    data = _data_dir()
    result: dict[str, list] = {"portfolio": [], "labs": [], "marketsplit": [], "mis": []}

    portfolio_dir = data / "portfolio"
    if portfolio_dir.exists():
        result["portfolio"] = sorted(p.name for p in portfolio_dir.iterdir() if p.is_dir())

    labs_dir = data / "labs"
    if labs_dir.exists():
        result["labs"] = sorted(
            int(p.stem.replace("labs", "")) for p in labs_dir.glob("labs*.qs")
        )

    ms_dir = _LOCAL_QOBLIB / "marketsplit"
    if ms_dir.exists():
        result["marketsplit"] = sorted(p.stem for p in ms_dir.glob("*.qs"))

    mis_dir = _LOCAL_QOBLIB / "mis"
    if mis_dir.exists():
        result["mis"] = sorted(p.stem for p in mis_dir.glob("*.qs"))

    return result


# --- Main ---


def main():
    parser = argparse.ArgumentParser(description="QOBLIB Benchmark Runner for Arvak")
    parser.add_argument(
        "--problem",
        choices=["portfolio", "labs", "marketsplit", "mis", "all"],
        default="all",
    )
    parser.add_argument("--instance", type=str, help="Instance name (e.g., 10 for portfolio, farm for MIS)")
    parser.add_argument("--shots", type=int, default=1024)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--target-positions", type=int, default=5, help="Portfolio: number of assets to select")
    parser.add_argument("--max-vars", type=int, default=200, help="Skip instances with more variables")
    parser.add_argument("--list", action="store_true", help="List available instances")
    parser.add_argument("--output", type=str, default=None, help="Output JSON path")
    args = parser.parse_args()

    if args.list:
        instances = list_instances()
        print(json.dumps(instances, indent=2))
        return

    all_results: list[BenchmarkResult] = []

    # Portfolio benchmarks
    if args.problem in ("portfolio", "all"):
        instances = list_instances().get("portfolio", [])
        if args.instance:
            instances = [i for i in instances if args.instance in i]

        for inst in instances:
            parts = inst.split("_")
            n_assets = int(parts[1][1:])
            n_periods = int(parts[2][1:])

            if n_assets > 20:
                log.info("Skipping %s (n=%d > 20, brute-force infeasible)", inst, n_assets)
                continue

            try:
                results = run_portfolio_benchmark(
                    num_assets=n_assets,
                    num_periods=n_periods,
                    target_positions=min(args.target_positions, n_assets // 2),
                    shots=args.shots,
                    seed=args.seed,
                )
                all_results.extend(results)
            except Exception as e:
                log.error("Portfolio %s failed: %s", inst, e)

    # LABS benchmarks
    if args.problem in ("labs", "all"):
        instances = list_instances().get("labs", [])
        if args.instance:
            try:
                instances = [int(args.instance)] if int(args.instance) in instances else []
            except ValueError:
                instances = []

        for n in instances:
            try:
                q, meta = load_labs_qubo(n)
                dim = q.shape[0]
                if dim > 25:
                    log.info("Skipping LABS n=%d (QUBO dim=%d, too large for simulator)", n, dim)
                    continue
                results = run_labs_benchmark(n=n, shots=args.shots, seed=args.seed)
                all_results.extend(results)
            except Exception as e:
                log.error("LABS n=%d failed: %s", n, e)

    # Market Split benchmarks
    if args.problem in ("marketsplit", "all"):
        instances = list_instances().get("marketsplit", [])
        if args.instance:
            instances = [i for i in instances if args.instance in i]
        elif args.problem == "all":
            # In "all" mode, only run small Market Split instances
            instances = [i for i in instances if "_050_" in i][:3]

        for inst in instances:
            try:
                q, meta = load_marketsplit_qubo(inst)
                if q.shape[0] > args.max_vars:
                    log.info("Skipping %s (n=%d > %d)", inst, q.shape[0], args.max_vars)
                    continue
                results = run_marketsplit_benchmark(inst, shots=args.shots, seed=args.seed)
                all_results.extend(results)
            except Exception as e:
                log.error("Market Split %s failed: %s", inst, e)

    # MIS benchmarks
    if args.problem in ("mis", "all"):
        instances = list_instances().get("mis", [])
        if args.instance:
            instances = [i for i in instances if args.instance in i]
        elif args.problem == "all":
            # In "all" mode, only run small MIS instances
            instances = [i for i in instances if i in ("farm", "karate", "chesapeake")]

        for inst in instances:
            try:
                q, meta = load_mis_qubo(inst)
                if q.shape[0] > args.max_vars:
                    log.info("Skipping %s (n=%d > %d)", inst, q.shape[0], args.max_vars)
                    continue
                results = run_mis_benchmark(inst, shots=args.shots, seed=args.seed)
                all_results.extend(results)
            except Exception as e:
                log.error("MIS %s failed: %s", inst, e)

    # Summary
    if all_results:
        log.info("\n" + "=" * 60)
        log.info("QOBLIB BENCHMARK SUMMARY")
        log.info("=" * 60)
        log.info("%-20s %-15s %-8s %-12s %-10s %-8s",
                 "Instance", "Solver", "Vars", "Energy", "Ratio", "Time")
        log.info("-" * 73)
        for r in all_results:
            ratio_str = f"{r.approximation_ratio:.4f}" if not np.isnan(r.approximation_ratio) else "N/A"
            log.info("%-20s %-15s %-8d %-12.4f %-10s %-8.2fs",
                     r.instance, r.solver, r.n_variables, r.best_energy,
                     ratio_str, r.wall_time_seconds)

        # Save results
        output_path = args.output or str(Path(__file__).parent / "data" / "qoblib_results.json")
        Path(output_path).parent.mkdir(parents=True, exist_ok=True)
        with open(output_path, "w") as f:
            json.dump(
                {
                    "benchmark": "qoblib",
                    "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                    "platform": "arvak",
                    "solvers": ["qaoa", "pce"],
                    "results": [asdict(r) for r in all_results],
                },
                f, indent=2, default=str,
            )
        log.info("\nResults saved to %s", output_path)
    else:
        log.info("No results to report.")


if __name__ == "__main__":
    main()
