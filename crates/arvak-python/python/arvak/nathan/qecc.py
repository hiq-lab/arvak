"""P1 #5 — QECC error correction suggestions.

When a circuit's suitability score is below 0.4, it is too error-prone
for direct hardware execution. This module recommends an appropriate
Quantum Error Correction (QEC) code with estimated overhead.

Two modes:
- ``mqt.qecc`` available: delegates to it for precise overhead calculations
- Static table fallback: uses well-known code parameters (always available)
"""

from __future__ import annotations

import math
import re
from dataclasses import dataclass


@dataclass
class QecRecommendation:
    """A Quantum Error Correction code recommendation."""

    code: str                  # "surface_code", "color_code", "repetition_code"
    distance: int              # code distance (min weight of detectable error)
    physical_qubits: int       # estimated total physical qubit count
    logical_qubits: int        # number of logical qubits protected
    threshold: float           # code error rate threshold
    description: str
    mqt_qecc_available: bool   # whether mqt.qecc was used

    def __repr__(self) -> str:
        return (
            f"QecRecommendation(code={self.code!r}, distance={self.distance}, "
            f"physical_qubits={self.physical_qubits}, logical_qubits={self.logical_qubits})"
        )


# ---------------------------------------------------------------------------
# Static code table: (threshold, phys_per_logical_at_d1, description)
# Physical qubits at distance d = phys_base × d²  (surface/color)
#                              = d  (repetition, only protects bit-flip)
# ---------------------------------------------------------------------------
_CODE_TABLE: dict[str, dict] = {
    "surface_code": {
        "threshold": 0.01,      # ~1% physical error threshold
        "phys_formula": "2*d*d",  # 2d² per logical qubit (rotated surface code)
        "description": (
            "Rotated surface code — best balance of threshold (~1%) and overhead. "
            "Standard choice for near-term fault-tolerant quantum computing."
        ),
    },
    "color_code": {
        "threshold": 0.0082,    # ~0.82% threshold (slightly lower than surface)
        "phys_formula": "3*d*d//2",  # ~3/2 d² per logical qubit
        "description": (
            "Color code — transversal Clifford gates, lower qubit overhead than "
            "surface code. Threshold ~0.82%."
        ),
    },
    "repetition_code": {
        "threshold": 0.5,       # very high threshold but only protects one error type
        "phys_formula": "d",    # d physical per 1 logical (bit-flip only)
        "description": (
            "Repetition code — very high threshold but only corrects bit-flip "
            "(or phase-flip) errors. Best for simple near-term demonstrations."
        ),
    },
}


def _qecc_available() -> bool:
    """Check if mqt.qecc is installed."""
    try:
        import mqt.qecc  # noqa: F401
        return True
    except ImportError:
        return False


def _parse_error_rate(error_rate_str: str) -> float:
    """Parse an error rate string to a float.

    Handles:
    - "~2%" → 0.02
    - "0.01" → 0.01
    - "high" → 0.05
    - "medium" → 0.01
    - "low" → 0.001
    - "<1%" → 0.01
    - "" → 0.01 (default)
    """
    if not error_rate_str:
        return 0.01

    s = error_rate_str.strip().lower()

    # Named levels
    if s in ("high", "very high", "very_high"):
        return 0.05
    if s in ("medium", "moderate"):
        return 0.01
    if s in ("low", "very low", "very_low"):
        return 0.001

    # Percentage: "~2%", "<1%", "2.5%"
    pct = re.search(r"(\d+\.?\d*)\s*%", s)
    if pct:
        return float(pct.group(1)) / 100.0

    # Plain float: "0.02", ".005"
    flt = re.search(r"(\d*\.?\d+)", s)
    if flt:
        val = float(flt.group(1))
        # If it looks like a percentage (> 0.1 without % sign), treat as percent
        if val > 0.1:
            return val / 100.0
        return val

    return 0.01  # safe default


def _compute_distance(
    physical_error_rate: float,
    threshold: float,
    target_logical_error: float = 1e-6,
) -> int:
    """Estimate required code distance d.

    Using the approximate formula:
        p_L ≈ (p / p_th)^((d+1)/2)

    Solving for d:
        d = 2 * ceil(log(p_L) / log(p / p_th)) - 1
    """
    if physical_error_rate <= 0.0:
        return 3

    ratio = physical_error_rate / threshold
    if ratio >= 1.0:
        # Physical error rate above threshold — QEC won't help, return large d
        return 51

    if ratio <= 0.0:
        return 3

    # d ≈ 2 * ceil(log(target) / log(ratio)) - 1
    log_ratio = math.log(ratio)
    if log_ratio >= 0.0:
        return 3

    d_float = 2.0 * math.log(target_logical_error) / log_ratio - 1.0
    d = max(3, math.ceil(d_float))

    # Ensure d is odd (required for surface/color codes)
    if d % 2 == 0:
        d += 1

    return d


def _phys_qubits(code: str, d: int, n_logical: int) -> int:
    """Estimate physical qubit count for code at distance d."""
    formula = _CODE_TABLE[code]["phys_formula"]
    per_logical = eval(formula, {"d": d})  # safe: formula is hardcoded above
    return int(per_logical) * n_logical


def _select_code(physical_error_rate: float, suitability: float) -> str:
    """Select best QEC code based on physical error rate and suitability.

    Rules:
    - If error rate is extremely high (> 10%) → repetition_code (demonstration only)
    - If error rate is above surface code threshold (~1%) → surface_code (best effort)
    - If error rate is below color code threshold → color_code preferred
    - Default → surface_code
    """
    if physical_error_rate > 0.10:
        return "repetition_code"
    if physical_error_rate < _CODE_TABLE["color_code"]["threshold"]:
        return "color_code"
    return "surface_code"


def recommend_qec(
    num_logical_qubits: int,
    estimated_error_rate: str,
    suitability: float,
) -> QecRecommendation | None:
    """Return a QEC recommendation if the suitability is too low.

    Args:
        num_logical_qubits: Number of qubits in the logical circuit.
        estimated_error_rate: Error rate string from AnalysisReport
            (e.g. "~2%", "high", "0.01").
        suitability: Quantum suitability score (0.0 – 1.0).

    Returns:
        QecRecommendation if suitability < 0.4, else None.
    """
    if suitability >= 0.4:
        return None

    physical_error = _parse_error_rate(estimated_error_rate)
    code = _select_code(physical_error, suitability)
    code_info = _CODE_TABLE[code]
    threshold = code_info["threshold"]

    # If mqt.qecc available, try to use it for precise calculation
    mqt_available = _qecc_available()
    if mqt_available:
        try:
            d, phys = _mqt_qecc_estimate(code, physical_error, num_logical_qubits)
        except Exception:
            d = _compute_distance(physical_error, threshold)
            phys = _phys_qubits(code, d, num_logical_qubits)
    else:
        d = _compute_distance(physical_error, threshold)
        phys = _phys_qubits(code, d, num_logical_qubits)

    return QecRecommendation(
        code=code,
        distance=d,
        physical_qubits=phys,
        logical_qubits=num_logical_qubits,
        threshold=threshold,
        description=code_info["description"],
        mqt_qecc_available=mqt_available,
    )


def _mqt_qecc_estimate(
    code: str,
    physical_error: float,
    n_logical: int,
) -> tuple[int, int]:
    """Use mqt.qecc for more precise distance and qubit estimation.

    Returns (distance, physical_qubits).
    """
    from mqt.qecc import Code  # type: ignore[import]

    code_map = {
        "surface_code": "surface",
        "color_code": "color",
        "repetition_code": "repetition",
    }
    qecc_name = code_map.get(code, "surface")

    # Try to instantiate and query — API varies by mqt.qecc version
    try:
        c = Code(qecc_name, 3)  # start at d=3
        # Binary search for sufficient distance
        d = 3
        for candidate_d in range(3, 51, 2):
            c2 = Code(qecc_name, candidate_d)
            # Estimate logical error rate (formula: (p/p_th)^((d+1)/2))
            threshold = _CODE_TABLE[code]["threshold"]
            ratio = physical_error / threshold
            if ratio < 1.0:
                p_l = ratio ** ((candidate_d + 1) / 2)
                if p_l < 1e-6:
                    d = candidate_d
                    break
            else:
                d = candidate_d
                break
        phys = _phys_qubits(code, d, n_logical)
        return d, phys
    except Exception:
        threshold = _CODE_TABLE[code]["threshold"]
        d = _compute_distance(physical_error, threshold)
        phys = _phys_qubits(code, d, n_logical)
        return d, phys
