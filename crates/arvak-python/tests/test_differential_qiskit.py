"""Differential testing: arvak.compile vs. qiskit.transpile semantics.

The Rust-side statevector checks (pipeline_invariants, proptest_pipeline)
compare Arvak against Arvak's own simulator — a gate-convention error
shared by compiler and simulator is invisible to them. This suite breaks
that circularity by using Qiskit as an independent reference:

  reference   = simulate(input circuit)                     [BasicSimulator]
  arvak path  = simulate(qiskit.loads(to_qasm(arvak.compile(input))))
  qiskit path = simulate(qiskit.transpile(input))           [harness sanity]

Inputs are seeded `qiskit.circuit.random.random_circuit` instances
(normalized to an import-safe but diverse gate set), swept over coupling
shapes, target bases, and optimization levels. Distributions are compared
by total variation distance against the two-sample statistical null.

Case count: ARVAK_DIFF_CASES (default 10 for PR CI; nightly runs 100).
"""

from __future__ import annotations

import math
import os
import random

import pytest

qiskit = pytest.importorskip("qiskit")

import arvak  # noqa: E402
import qiskit.qasm3  # noqa: E402
from qiskit.circuit.random import random_circuit  # noqa: E402
from qiskit.providers.basic_provider import BasicSimulator  # noqa: E402
from qiskit.transpiler import CouplingMap as QkCouplingMap  # noqa: E402

CASES = int(os.environ.get("ARVAK_DIFF_CASES", "10"))
SHOTS = 1024
SEED0 = 20260709

# Diverse but guaranteed-importable normalization target for the random
# circuits (arvak's qiskit integration understands all of these).
NORMALIZE_BASIS = [
    "x", "y", "z", "h", "s", "sdg", "t", "tdg", "sx",
    "rx", "ry", "rz", "p", "u",
    "cx", "cy", "cz", "ch", "cp", "crx", "cry", "crz",
    "swap", "iswap", "rxx", "ryy", "rzz",
    "ccx", "cswap", "id",
]

BASES = ["ibm", "iqm", "heron"]

_SIM = BasicSimulator()


def _distribution(qc) -> dict[str, float]:
    """Shot-based classical-register distribution, register-layout
    normalized (multi-register keys are space-separated in qiskit)."""
    runnable = qiskit.transpile(qc, basis_gates=["u", "cx"])
    counts = _SIM.run(runnable, shots=SHOTS, seed_simulator=7).result().get_counts()
    return {k.replace(" ", ""): v / SHOTS for k, v in counts.items()}


def _tvd(p: dict[str, float], q: dict[str, float]) -> float:
    keys = set(p) | set(q)
    return 0.5 * sum(abs(p.get(k, 0.0) - q.get(k, 0.0)) for k in keys)


def _threshold(support: int) -> float:
    """Two-sample TVD null: E[TVD] <= sqrt(K/(pi*N)); 1.8x with a 0.10
    floor separates sampling noise from real bugs (which land at 0.5+)."""
    return max(0.10, 1.8 * math.sqrt(support / (math.pi * SHOTS)))


def _couplings(n: int, rng: random.Random) -> list[tuple[int, int]]:
    shape = rng.choice(["linear", "star", "full"])
    if shape == "linear":
        return [(i, i + 1) for i in range(n - 1)]
    if shape == "star":
        return [(0, i) for i in range(1, n)]
    return [(i, j) for i in range(n) for j in range(i + 1, n)]


@pytest.mark.parametrize("case", range(CASES))
def test_arvak_matches_qiskit_reference(case: int) -> None:
    rng = random.Random(SEED0 + case)
    n = rng.randint(2, 5)
    depth = rng.randint(3, 12)
    level = rng.randint(0, 3)
    basis_name = rng.choice(BASES)
    edges = _couplings(n, rng)

    # Seeded random circuit, normalized to an import-safe gate set.
    rc = random_circuit(n, depth, max_operands=3, measure=True, seed=SEED0 + case)
    norm = qiskit.transpile(
        rc, basis_gates=NORMALIZE_BASIS, optimization_level=0, seed_transpiler=1
    )
    label = f"case{case}: n={n} depth={depth} {basis_name} o{level} edges={edges}"

    reference = _distribution(norm)

    # --- Arvak path ---
    arvak_qc = arvak.get_integration("qiskit").to_arvak(norm)
    compiled = arvak.compile(
        arvak_qc,
        coupling_map=arvak.CouplingMap.from_edge_list(n, edges),
        basis_gates=getattr(arvak.BasisGates, basis_name)(),
        optimization_level=level,
    )
    arvak_out = qiskit.qasm3.loads(arvak.to_qasm(compiled))
    arvak_dist = _distribution(arvak_out)

    tvd = _tvd(reference, arvak_dist)
    bound = _threshold(len(set(reference) | set(arvak_dist)))
    assert tvd <= bound, (
        f"{label}: arvak output diverges from reference "
        f"(tvd={tvd:.3f} > {bound:.3f})"
    )

    # --- Qiskit cross-path (harness sanity: same constraints, independent
    # compiler; a failure here means the test setup is wrong, not arvak) ---
    qk_out = qiskit.transpile(
        norm,
        coupling_map=QkCouplingMap(edges + [(b, a) for a, b in edges]),
        basis_gates=["rz", "sx", "x", "cx"],
        optimization_level=min(level, 3),
        seed_transpiler=2,
    )
    qk_dist = _distribution(qk_out)
    qk_tvd = _tvd(reference, qk_dist)
    qk_bound = _threshold(len(set(reference) | set(qk_dist)))
    assert qk_tvd <= qk_bound, (
        f"{label}: HARNESS problem — qiskit.transpile itself diverges "
        f"(tvd={qk_tvd:.3f} > {qk_bound:.3f})"
    )
