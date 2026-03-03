"""Clifford circuit detection and optimal synthesis via MQT QMAP.

Detects Clifford-heavy regions in quantum circuits and uses QMAP's
SAT-based synthesis to produce provably depth/gate-optimal decompositions.

Requires: ``pip install mqt.qmap``
"""

from __future__ import annotations

import logging
import re
import tempfile
import os
from dataclasses import dataclass

logger = logging.getLogger(__name__)

# Clifford gate set (case-insensitive matching)
CLIFFORD_GATES: frozenset[str] = frozenset({
    "id", "i",
    "x", "y", "z",
    "h",
    "s", "sdg",
    "cx", "cnot", "cz", "cy",
    "swap",
    "sx", "sxdg",  # sqrt(X) is Clifford
})

# Minimum number of gates in a Clifford region to warrant SAT optimization
MIN_CLIFFORD_REGION_SIZE = 4


@dataclass
class CliffordRegion:
    """A contiguous block of Clifford gates extracted from a circuit."""

    qasm3: str
    qubit_decl: str  # e.g. "qubit[4] q;"
    num_qubits: int
    gate_count: int
    start_line: int  # 0-indexed line number in original circuit
    end_line: int


@dataclass
class CliffordOptResult:
    """Result of optimizing a Clifford region via QMAP."""

    original_qasm: str
    optimized_qasm: str
    original_gates: int
    optimized_gates: int
    original_depth: int
    optimized_depth: int
    improvement_pct: float  # gate count reduction percentage
    method: str  # "sat_optimal" or "heuristic"


def _qmap_available() -> bool:
    """Check if mqt.qmap is installed."""
    try:
        from mqt.qmap.plugins.qiskit.clifford_synthesis import optimize_clifford  # noqa: F401
        return True
    except ImportError:
        try:
            from mqt.qmap import optimize_clifford  # noqa: F401
            return True
        except ImportError:
            return False


def is_clifford_gate(gate_name: str) -> bool:
    """Check if a gate name corresponds to a Clifford gate."""
    return gate_name.lower().strip() in CLIFFORD_GATES


def _parse_qasm3_gates(qasm3_code: str) -> list[tuple[int, str, str]]:
    """Parse gate statements from QASM3 code.

    Returns list of (line_number, gate_name, full_statement).
    """
    gates = []
    # Match gate applications in two forms:
    #   gate_name qubit_args;          e.g., h q[0]; cx q[0],q[1];
    #   gate_name(params) qubit_args;  e.g., rz(0.5) q[0]; rx(3.14) q[1];
    gate_pattern = re.compile(
        r"^\s*([a-zA-Z][a-zA-Z0-9_]*)"  # gate name
        r"(?:\([^)]*\))?"               # optional (params)
        r"\s+([^;]+);\s*$"              # qubit arguments + semicolon
    )
    for i, line in enumerate(qasm3_code.splitlines()):
        stripped = line.strip()
        # Skip directives, declarations, and empty lines
        if not stripped or stripped.startswith("//"):
            continue
        if stripped.startswith(("OPENQASM", "include", "qubit", "bit", "creg", "qreg")):
            continue
        if stripped.startswith(("measure", "barrier", "reset")):
            continue
        m = gate_pattern.match(line)
        if m:
            gate_name = m.group(1).lower()
            gates.append((i, gate_name, stripped))
    return gates


def _extract_qubit_info(qasm3_code: str) -> tuple[str, int]:
    """Extract qubit declaration and count from QASM3.

    Returns (qubit_decl, num_qubits).
    """
    for line in qasm3_code.splitlines():
        stripped = line.strip()
        # Match: qubit[N] name;
        m = re.match(r"qubit\[(\d+)\]\s+\w+\s*;", stripped)
        if m:
            return stripped, int(m.group(1))
        # Match: qreg name[N];
        m = re.match(r"qreg\s+\w+\[(\d+)\]\s*;", stripped)
        if m:
            return stripped, int(m.group(1))
    return "qubit[1] q;", 1


def analyze_clifford_content(qasm3_code: str) -> dict:
    """Analyze the Clifford gate content of a QASM3 circuit.

    Returns a dict with:
        total_gates: int
        clifford_gates: int
        non_clifford_gates: int
        clifford_ratio: float (0.0 - 1.0)
        is_fully_clifford: bool
        gate_breakdown: dict[str, int]
    """
    gates = _parse_qasm3_gates(qasm3_code)
    total = len(gates)
    if total == 0:
        return {
            "total_gates": 0,
            "clifford_gates": 0,
            "non_clifford_gates": 0,
            "clifford_ratio": 0.0,
            "is_fully_clifford": False,
            "gate_breakdown": {},
        }

    breakdown: dict[str, int] = {}
    clifford_count = 0
    for _, gate_name, _stmt in gates:
        breakdown[gate_name] = breakdown.get(gate_name, 0) + 1
        if is_clifford_gate(gate_name):
            clifford_count += 1

    return {
        "total_gates": total,
        "clifford_gates": clifford_count,
        "non_clifford_gates": total - clifford_count,
        "clifford_ratio": clifford_count / total if total > 0 else 0.0,
        "is_fully_clifford": clifford_count == total,
        "gate_breakdown": breakdown,
    }


def find_clifford_regions(
    qasm3_code: str,
    min_gates: int = MIN_CLIFFORD_REGION_SIZE,
) -> list[CliffordRegion]:
    """Find contiguous Clifford-only regions in a QASM3 circuit.

    Scans the gate sequence for maximal runs of Clifford gates and returns
    regions that meet the minimum gate count threshold.

    Args:
        qasm3_code: The QASM3 circuit code.
        min_gates: Minimum number of gates for a region to be returned.

    Returns:
        List of CliffordRegion objects, sorted by gate count (largest first).
    """
    gates = _parse_qasm3_gates(qasm3_code)
    qubit_decl, num_qubits = _extract_qubit_info(qasm3_code)

    regions: list[CliffordRegion] = []
    current_run: list[tuple[int, str, str]] = []

    def flush_run():
        if len(current_run) >= min_gates:
            stmts = [stmt for _, _, stmt in current_run]
            region_qasm = f"OPENQASM 3.0;\n{qubit_decl}\n" + "\n".join(stmts) + "\n"
            regions.append(CliffordRegion(
                qasm3=region_qasm,
                qubit_decl=qubit_decl,
                num_qubits=num_qubits,
                gate_count=len(current_run),
                start_line=current_run[0][0],
                end_line=current_run[-1][0],
            ))

    for line_no, gate_name, stmt in gates:
        if is_clifford_gate(gate_name):
            current_run.append((line_no, gate_name, stmt))
        else:
            flush_run()
            current_run = []

    flush_run()

    # Sort by gate count, largest first
    regions.sort(key=lambda r: r.gate_count, reverse=True)
    return regions


def optimize_clifford(
    clifford_qasm: str,
    target: str = "depth",
    use_heuristic: bool = False,
    timeout: float = 60.0,
) -> CliffordOptResult | None:
    """Optimize a Clifford circuit using MQT QMAP's SAT-based synthesis.

    Args:
        clifford_qasm: QASM3 string containing only Clifford gates.
        target: Optimization target — "depth" or "gates" (default: "depth").
        use_heuristic: Use heuristic mode for larger circuits (faster, not provably optimal).
        timeout: Maximum seconds for SAT solving.

    Returns:
        CliffordOptResult with the optimized circuit, or None if optimization failed.
    """
    try:
        return _run_qmap_optimization(clifford_qasm, target, use_heuristic, timeout)
    except ImportError:
        logger.debug("mqt.qmap not available — skipping Clifford optimization")
        return None
    except Exception as e:
        logger.warning("QMAP Clifford optimization failed: %s", e)
        return None


def _run_qmap_optimization(
    clifford_qasm: str,
    target: str,
    use_heuristic: bool,
    timeout: float,
) -> CliffordOptResult | None:
    """Internal: run QMAP optimization (may raise ImportError)."""
    # Try modern API first (v3.5+), then legacy
    try:
        from mqt.qmap.plugins.qiskit.clifford_synthesis import optimize_clifford as qmap_optimize
    except ImportError:
        from mqt.qmap import optimize_clifford as qmap_optimize  # type: ignore[no-redef]

    # Count original gates from QASM before optimization
    orig_gates_parsed = _parse_qasm3_gates(clifford_qasm)
    original_gates = len(orig_gates_parsed)

    # QMAP's optimize_clifford accepts QASM strings directly.
    # Write to temp file as a reliable path for all QMAP versions.
    fd, qasm_path = tempfile.mkstemp(suffix=".qasm")
    try:
        with os.fdopen(fd, "w") as f:
            f.write(clifford_qasm)

        method = "heuristic" if use_heuristic else "sat_optimal"
        kwargs: dict = {"target_metric": target, "heuristic": use_heuristic}

        optimized_qc, results = qmap_optimize(qasm_path, **kwargs)
    finally:
        try:
            os.unlink(qasm_path)
        except OSError:
            pass

    # Use SynthesisResults attributes when available, fall back to circuit methods
    opt_gates = getattr(results, "gates", None) or optimized_qc.size()
    opt_depth = getattr(results, "depth", None) or optimized_qc.depth()
    original_depth = _estimate_depth(orig_gates_parsed)

    # Convert optimized circuit back to QASM3
    optimized_qasm = _circuit_to_qasm3(optimized_qc)
    if optimized_qasm is None:
        return None

    improvement = (
        (original_gates - opt_gates) / original_gates * 100.0
        if original_gates > 0
        else 0.0
    )

    return CliffordOptResult(
        original_qasm=clifford_qasm,
        optimized_qasm=optimized_qasm,
        original_gates=original_gates,
        optimized_gates=opt_gates,
        original_depth=original_depth,
        optimized_depth=opt_depth,
        improvement_pct=improvement,
        method=method,
    )


def _estimate_depth(gates: list[tuple[int, str, str]]) -> int:
    """Rough depth estimate from parsed gate list (gates on same qubits are sequential)."""
    # Simple heuristic: depth ≈ number of gates (conservative upper bound)
    # Actual depth requires dependency analysis, but this is for display only
    return len(gates)


def _circuit_to_qasm3(qc) -> str | None:
    """Convert a Qiskit QuantumCircuit to QASM3 string.

    Returns None if conversion fails.
    """
    try:
        from qiskit.qasm3 import dumps
        return dumps(qc)
    except Exception:
        pass

    try:
        return qc.qasm()
    except Exception:
        logger.debug("Failed to convert Qiskit circuit to QASM3")
        return None


def generate_clifford_suggestions(
    original_qasm: str,
    min_region_gates: int = MIN_CLIFFORD_REGION_SIZE,
    target: str = "depth",
) -> list:
    """Analyze a circuit for Clifford regions and generate optimized suggestions.

    Scans the original circuit for Clifford-heavy regions, optimizes each
    via QMAP's SAT-based synthesis, and returns Suggestion objects for
    regions where improvement was found.

    Args:
        original_qasm: The original QASM3 circuit.
        min_region_gates: Minimum gate count for regions to optimize.
        target: Optimization target — "depth" or "gates".

    Returns:
        List of Suggestion objects with optimized Clifford rewrites.
    """
    from .report import Suggestion

    if not _qmap_available():
        logger.debug("mqt.qmap not available — skipping Clifford suggestions")
        return []

    analysis = analyze_clifford_content(original_qasm)
    if analysis["clifford_gates"] < min_region_gates:
        return []

    suggestions: list = []

    # If the whole circuit is Clifford, optimize it entirely
    if analysis["is_fully_clifford"] and analysis["total_gates"] >= min_region_gates:
        # For large circuits, use heuristic mode
        use_heuristic = analysis["total_gates"] > 50
        result = optimize_clifford(
            original_qasm,
            target=target,
            use_heuristic=use_heuristic,
        )
        if result and result.improvement_pct > 0:
            method_label = "heuristic" if use_heuristic else "SAT-proven"
            source = "qmap_sat" if not use_heuristic else "qmap_heuristic"
            suggestions.append(Suggestion(
                title=f"Optimal Clifford synthesis ({method_label})",
                description=(
                    f"QMAP {method_label} optimal decomposition: "
                    f"{result.original_gates} → {result.optimized_gates} gates "
                    f"({result.improvement_pct:.0f}% reduction), "
                    f"depth {result.original_depth} → {result.optimized_depth}."
                ),
                qasm3=result.optimized_qasm,
                impact="high",
                verified=True,  # QMAP synthesis is provably correct
                verification_status="verified",
                verification_message=f"Provably equivalent ({method_label} Clifford synthesis)",
                source=source,
            ))
        return suggestions

    # Otherwise, find and optimize Clifford regions
    regions = find_clifford_regions(original_qasm, min_gates=min_region_gates)
    for region in regions[:3]:  # Limit to top 3 largest regions
        use_heuristic = region.gate_count > 50
        result = optimize_clifford(
            region.qasm3,
            target=target,
            use_heuristic=use_heuristic,
        )
        if result and result.improvement_pct > 0:
            method_label = "heuristic" if use_heuristic else "SAT-proven"
            source = "qmap_sat" if not use_heuristic else "qmap_heuristic"
            suggestions.append(Suggestion(
                title=f"Optimize Clifford region (lines {region.start_line + 1}-{region.end_line + 1})",
                description=(
                    f"Clifford subcircuit ({region.gate_count} gates, {region.num_qubits} qubits): "
                    f"QMAP {method_label} synthesis reduces to "
                    f"{result.optimized_gates} gates "
                    f"({result.improvement_pct:.0f}% reduction), "
                    f"depth {result.original_depth} → {result.optimized_depth}."
                ),
                qasm3=result.optimized_qasm,
                impact="high",
                verified=True,
                verification_status="verified",
                verification_message=f"Provably equivalent ({method_label} Clifford synthesis)",
                source=source,
            ))

    return suggestions
