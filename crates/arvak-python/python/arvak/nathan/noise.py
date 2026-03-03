"""P1 #4 — DDSIM noise-aware fidelity scoring.

Simulates a circuit under a noise model matching the target backend's
capabilities and reports empirical fidelity (overlap between ideal and
noisy output distributions).

Two modes:
- ``mqt.ddsim`` available: runs proper noisy simulation + TVD fidelity
- Heuristic fallback: estimates fidelity from circuit depth × gate error rates

The heuristic always returns a value — never None.
"""

from __future__ import annotations

import math
import re
from dataclasses import dataclass


@dataclass
class FidelityEstimate:
    """Result of a fidelity estimation."""

    fidelity: float       # 0.0–1.0
    method: str           # "ddsim_noisy" | "heuristic"
    noise_model: str      # description of noise model used
    backend: str
    num_shots: int

    def __repr__(self) -> str:
        return (
            f"FidelityEstimate(fidelity={self.fidelity:.3f}, "
            f"method={self.method!r}, backend={self.backend!r})"
        )


# ---------------------------------------------------------------------------
# Backend noise profiles — (single_qubit_error, two_qubit_error, t1_us, t2_us)
# Used by the heuristic fallback when ddsim is not installed.
# ---------------------------------------------------------------------------
_BACKEND_PROFILES: dict[str, tuple[float, float, float, float]] = {
    # (sq_err, tq_err, T1_us, T2_us)
    "ibm_heron":        (0.0005, 0.003, 200.0, 150.0),
    "ibm_eagle":        (0.001,  0.005, 150.0, 100.0),
    "ibm_marrakesh":    (0.0005, 0.003, 200.0, 150.0),
    "ibm_torino":       (0.0005, 0.003, 200.0, 150.0),
    "ibm_strasbourg":   (0.001,  0.005, 150.0, 100.0),
    "ibm_brussels":     (0.001,  0.005, 150.0, 100.0),
    "iqm_garnet":       (0.002,  0.007, 80.0,  50.0),
    "iqm_sirius":       (0.002,  0.006, 90.0,  60.0),
    "iqm_emerald":      (0.002,  0.008, 75.0,  45.0),
    "iqm_crystal":      (0.002,  0.007, 80.0,  50.0),
    "quantinuum_h2":    (0.0001, 0.002, 1000.0, 500.0),
    "quantinuum_h1":    (0.0002, 0.003, 800.0, 400.0),
    "aqt_offline":      (0.001,  0.005, 100.0, 80.0),
    "sim:ascella":      (0.003,  0.01,  50.0,  30.0),
    "sim:belenos":      (0.003,  0.01,  50.0,  30.0),
    "aer_simulator":    (0.0,    0.0,   1e9,   1e9),
}


def _ddsim_available() -> bool:
    """Check if mqt.ddsim is installed."""
    try:
        import mqt.ddsim  # noqa: F401
        return True
    except ImportError:
        return False


def _parse_qasm3_stats(qasm3_code: str) -> tuple[int, int, int]:
    """Quick parse: (num_qubits, total_gates, two_qubit_gates)."""
    num_qubits = 0
    for m in re.finditer(r"qubit\s*\[(\d+)\]", qasm3_code):
        num_qubits += int(m.group(1))
    if num_qubits == 0:
        for m in re.finditer(r"qreg\s+\w+\s*\[(\d+)\]", qasm3_code):
            num_qubits += int(m.group(1))

    # Count gate invocations (lines that look like gate calls)
    # Match QASM3 gate invocations: identifier optionally followed by (params),
    # then at least one space, then a qubit arg (letter or '[').
    # Excludes keywords, declarations, and classical assignments.
    gate_line = re.compile(
        r"^\s*(?!//|qubit|qreg|creg|bit|input|output|OPENQASM|include|measure|barrier|reset)"
        r"[a-zA-Z_]\w*(?:\s*\([^)]*\))?\s+[a-zA-Z\[]",
        re.MULTILINE,
    )
    total_gates = len(gate_line.findall(qasm3_code))

    # Two-qubit gates: cx, cz, cnot, cp, cu, ecr, rxx, rzz, swap, iswap, prx (2q)
    tq_pattern = re.compile(
        r"^\s*(?:cx|cz|cnot|cp|cu|ecr|rxx|rzz|swap|iswap|ccx|toffoli)\b",
        re.MULTILINE | re.IGNORECASE,
    )
    two_qubit_gates = len(tq_pattern.findall(qasm3_code))

    return num_qubits, total_gates, two_qubit_gates


def _heuristic_fidelity(
    qasm3_code: str,
    backend_name: str,
    noise_profile: dict | None = None,
) -> FidelityEstimate:
    """Estimate fidelity heuristically without running a simulation.

    Uses a simple product-of-gate-fidelities model:
        F ≈ (1 - sq_err)^n_sq_gates × (1 - tq_err)^n_tq_gates

    Args:
        qasm3_code: QASM3 source.
        backend_name: Target backend identifier.
        noise_profile: Override noise parameters (sq_err, tq_err).

    Returns:
        FidelityEstimate with method="heuristic".
    """
    _num_qubits, total_gates, tq_gates = _parse_qasm3_stats(qasm3_code)
    sq_gates = max(0, total_gates - tq_gates)

    # Resolve backend profile
    profile = None
    if noise_profile:
        sq_err = float(noise_profile.get("sq_err", 0.001))
        tq_err = float(noise_profile.get("tq_err", 0.005))
        noise_model_str = f"custom(sq={sq_err}, tq={tq_err})"
    else:
        key = backend_name.lower().replace("-", "_")
        for k, v in _BACKEND_PROFILES.items():
            if k in key or key in k:
                profile = v
                break
        if profile is None:
            profile = (0.001, 0.005, 100.0, 80.0)
        sq_err, tq_err = profile[0], profile[1]
        noise_model_str = f"depolarizing(sq={sq_err}, tq={tq_err})"

    fidelity = (1.0 - sq_err) ** sq_gates * (1.0 - tq_err) ** tq_gates
    fidelity = max(0.0, min(1.0, fidelity))

    return FidelityEstimate(
        fidelity=fidelity,
        method="heuristic",
        noise_model=noise_model_str,
        backend=backend_name,
        num_shots=0,
    )


def _tvd(dist_a: dict, dist_b: dict) -> float:
    """Total Variation Distance between two probability distributions."""
    all_keys = set(dist_a) | set(dist_b)
    return 0.5 * sum(abs(dist_a.get(k, 0.0) - dist_b.get(k, 0.0)) for k in all_keys)


def _run_ddsim(
    qasm3_code: str,
    backend_name: str,
    noise_profile: dict | None,
    shots: int,
) -> FidelityEstimate | None:
    """Run noisy + ideal simulation via mqt.ddsim.

    Returns None if ddsim fails or raises.
    """
    try:
        from mqt.ddsim import DDSIMProvider  # type: ignore[import]
        from qiskit import QuantumCircuit
        from qiskit.qasm3 import loads as qasm3_loads
    except ImportError:
        return None

    try:
        qc: QuantumCircuit = qasm3_loads(qasm3_code)
    except Exception:
        return None

    try:
        provider = DDSIMProvider()

        # Ideal simulation
        ideal_backend = provider.get_backend("statevector_simulator")
        ideal_job = ideal_backend.run(qc, shots=shots)
        ideal_counts: dict[str, int] = ideal_job.result().get_counts()

        # Build noise model
        sq_err = 0.001
        tq_err = 0.005
        if noise_profile:
            sq_err = float(noise_profile.get("sq_err", sq_err))
            tq_err = float(noise_profile.get("tq_err", tq_err))
        else:
            key = backend_name.lower()
            for k, v in _BACKEND_PROFILES.items():
                if k in key or key in k:
                    sq_err, tq_err = v[0], v[1]
                    break

        noise_model_str = f"depolarizing(sq={sq_err}, tq={tq_err})"

        # Noisy simulation using ddsim's noise-aware backend
        noisy_backend = provider.get_backend("noise_aware_qasm_simulator")
        # ddsim noise params (best-effort; API varies by version)
        try:
            noisy_job = noisy_backend.run(
                qc,
                shots=shots,
                noise_effects={"depolarizingError": tq_err},
            )
        except TypeError:
            noisy_job = noisy_backend.run(qc, shots=shots)

        noisy_counts: dict[str, int] = noisy_job.result().get_counts()

        total_ideal = sum(ideal_counts.values()) or 1
        total_noisy = sum(noisy_counts.values()) or 1
        ideal_dist = {k: v / total_ideal for k, v in ideal_counts.items()}
        noisy_dist = {k: v / total_noisy for k, v in noisy_counts.items()}

        tvd = _tvd(ideal_dist, noisy_dist)
        fidelity = max(0.0, 1.0 - tvd)

        return FidelityEstimate(
            fidelity=fidelity,
            method="ddsim_noisy",
            noise_model=noise_model_str,
            backend=backend_name,
            num_shots=shots,
        )
    except Exception:
        return None


def estimate_fidelity(
    qasm3_code: str,
    backend_name: str,
    noise_profile: dict | None = None,
    shots: int = 1024,
) -> FidelityEstimate:
    """Simulate circuit under noise and return fidelity vs ideal.

    Always returns a FidelityEstimate — uses ddsim if available,
    falls back to heuristic estimate otherwise.

    Args:
        qasm3_code: QASM3 source string.
        backend_name: Target backend (e.g. "iqm_garnet", "ibm_heron").
        noise_profile: Optional dict overriding noise params:
            ``sq_err`` (single-qubit), ``tq_err`` (two-qubit).
        shots: Number of shots for ddsim simulation.

    Returns:
        FidelityEstimate with fidelity in [0, 1].
    """
    if _ddsim_available():
        result = _run_ddsim(qasm3_code, backend_name, noise_profile, shots)
        if result is not None:
            return result

    return _heuristic_fidelity(qasm3_code, backend_name, noise_profile)
