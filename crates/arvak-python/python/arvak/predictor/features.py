"""Circuit feature extraction for ML-based device selection.

Extracts the seven features used by MQT Predictor's supervised-ML model:
  1. num_qubits — qubit count
  2. depth — circuit depth (longest path through the DAG)
  3. program_communication — qubit interaction density (0.0–1.0)
  4. critical_depth — fraction of multi-qubit gates on longest path
  5. entanglement_ratio — fraction of gates that are multi-qubit
  6. parallelism — gate parallelism score (0.0–1.0)
  7. gate_counts — breakdown by gate name

Works from QASM3 strings, arvak.Circuit, or Qiskit QuantumCircuit.
No external dependencies required.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field


@dataclass
class CircuitFeatures:
    """Extracted circuit features for ML-based device selection.

    These features match MQT Predictor's expected input format for
    supervised-ML device selection and RL-based compilation.
    """

    num_qubits: int = 0
    depth: int = 0
    num_gates: int = 0
    num_single_qubit_gates: int = 0
    num_multi_qubit_gates: int = 0
    program_communication: float = 0.0
    critical_depth: float = 0.0
    entanglement_ratio: float = 0.0
    parallelism: float = 0.0
    gate_counts: dict[str, int] = field(default_factory=dict)
    qubit_interaction_graph: list[tuple[int, int]] = field(default_factory=list)

    def to_dict(self) -> dict:
        """Convert to a flat dict suitable for ML model input."""
        return {
            "num_qubits": self.num_qubits,
            "depth": self.depth,
            "num_gates": self.num_gates,
            "num_single_qubit_gates": self.num_single_qubit_gates,
            "num_multi_qubit_gates": self.num_multi_qubit_gates,
            "program_communication": self.program_communication,
            "critical_depth": self.critical_depth,
            "entanglement_ratio": self.entanglement_ratio,
            "parallelism": self.parallelism,
        }

    def to_predictor_features(self) -> list[float]:
        """Convert to the 7-element feature vector used by MQT Predictor.

        Returns [num_qubits, depth, program_communication, critical_depth,
                 entanglement_ratio, parallelism, loschmidt_echo_placeholder].
        """
        return [
            float(self.num_qubits),
            float(self.depth),
            self.program_communication,
            self.critical_depth,
            self.entanglement_ratio,
            self.parallelism,
            0.0,  # placeholder for additional features
        ]

    def __repr__(self) -> str:
        return (
            f"CircuitFeatures(qubits={self.num_qubits}, depth={self.depth}, "
            f"gates={self.num_gates}, "
            f"multi_qubit={self.num_multi_qubit_gates}, "
            f"entanglement_ratio={self.entanglement_ratio:.2f}, "
            f"program_communication={self.program_communication:.2f})"
        )


def extract_features(circuit, language: str | None = None) -> CircuitFeatures:
    """Extract circuit features for ML-based device selection.

    Accepts:
    - arvak.Circuit objects
    - Raw QASM3 strings
    - Qiskit QuantumCircuit (auto-converted)

    Args:
        circuit: Quantum circuit in any supported format.
        language: Override language detection.

    Returns:
        CircuitFeatures with all extracted metrics.
    """
    qasm3_code = _to_qasm3_string(circuit, language)
    return _extract_from_qasm3(qasm3_code)


def _to_qasm3_string(circuit, language: str | None = None) -> str:
    """Convert any supported circuit format to QASM3 string."""
    if isinstance(circuit, str):
        return circuit

    try:
        import arvak as _arvak
        if isinstance(circuit, _arvak.Circuit):
            return _arvak.to_qasm(circuit)
    except (ImportError, AttributeError):
        pass

    # Qiskit
    _type_name = type(circuit).__module__ + "." + type(circuit).__qualname__
    if "qiskit" in _type_name.lower():
        try:
            from qiskit.qasm3 import dumps
            return dumps(circuit)
        except Exception:
            pass

    raise TypeError(
        f"Unsupported circuit type: {type(circuit).__name__}. "
        "Pass an arvak.Circuit, QASM3 string, or Qiskit QuantumCircuit."
    )


# ---------------------------------------------------------------------------
# QASM3 parsing and feature computation
# ---------------------------------------------------------------------------

# Regex for gate applications (with optional parameters)
_GATE_RE = re.compile(
    r"^\s*([a-zA-Z][a-zA-Z0-9_]*)"  # gate name
    r"(?:\([^)]*\))?"               # optional params
    r"\s+([^;]+);\s*$"              # qubit args + semicolon
)

# Regex for qubit references like q[0], q[1], r[3]
_QUBIT_REF_RE = re.compile(r"([a-zA-Z_]\w*)\[(\d+)\]")

# Lines to skip during parsing
_SKIP_PREFIXES = ("OPENQASM", "include", "qubit", "bit", "creg", "qreg",
                  "measure", "barrier", "reset", "//")


def _extract_from_qasm3(qasm3_code: str) -> CircuitFeatures:
    """Extract all features from a QASM3 string."""
    num_qubits = _parse_num_qubits(qasm3_code)
    gates = _parse_gates(qasm3_code)

    if not gates:
        return CircuitFeatures(num_qubits=num_qubits)

    # Classify gates
    gate_counts: dict[str, int] = {}
    single_qubit = 0
    multi_qubit = 0
    multi_qubit_gates: list[tuple[str, list[int]]] = []
    all_gate_ops: list[tuple[str, list[int]]] = []

    for gate_name, qubit_indices in gates:
        gate_counts[gate_name] = gate_counts.get(gate_name, 0) + 1
        all_gate_ops.append((gate_name, qubit_indices))
        if len(qubit_indices) >= 2:
            multi_qubit += 1
            multi_qubit_gates.append((gate_name, qubit_indices))
        else:
            single_qubit += 1

    num_gates = len(gates)

    # Compute depth via per-qubit layer scheduling
    depth = _compute_depth(all_gate_ops, num_qubits)

    # Entanglement ratio: fraction of multi-qubit gates
    entanglement_ratio = multi_qubit / num_gates if num_gates > 0 else 0.0

    # Program communication: qubit interaction density
    interaction_graph, program_communication = _compute_program_communication(
        multi_qubit_gates, num_qubits
    )

    # Critical depth: fraction of multi-qubit gates on the critical (longest) path
    critical_depth = _compute_critical_depth(all_gate_ops, num_qubits)

    # Parallelism
    parallelism = _compute_parallelism(all_gate_ops, num_qubits, depth)

    return CircuitFeatures(
        num_qubits=num_qubits,
        depth=depth,
        num_gates=num_gates,
        num_single_qubit_gates=single_qubit,
        num_multi_qubit_gates=multi_qubit,
        program_communication=program_communication,
        critical_depth=critical_depth,
        entanglement_ratio=entanglement_ratio,
        parallelism=parallelism,
        gate_counts=gate_counts,
        qubit_interaction_graph=interaction_graph,
    )


def _parse_num_qubits(qasm3_code: str) -> int:
    """Extract qubit count from QASM3 code."""
    total = 0
    for line in qasm3_code.splitlines():
        stripped = line.strip()
        m = re.match(r"qubit\[(\d+)\]\s+\w+\s*;", stripped)
        if m:
            total += int(m.group(1))
            continue
        m = re.match(r"qreg\s+\w+\[(\d+)\]\s*;", stripped)
        if m:
            total += int(m.group(1))
    return max(total, 1)


def _parse_gates(qasm3_code: str) -> list[tuple[str, list[int]]]:
    """Parse gates from QASM3, returning (gate_name, qubit_indices) pairs."""
    # Build register→offset map for multi-register circuits
    reg_offsets: dict[str, int] = {}
    offset = 0
    for line in qasm3_code.splitlines():
        stripped = line.strip()
        m = re.match(r"qubit\[(\d+)\]\s+(\w+)\s*;", stripped)
        if m:
            reg_offsets[m.group(2)] = offset
            offset += int(m.group(1))
            continue
        m = re.match(r"qreg\s+(\w+)\[(\d+)\]\s*;", stripped)
        if m:
            reg_offsets[m.group(1)] = offset
            offset += int(m.group(2))

    # Default register name for single-register circuits
    if not reg_offsets:
        reg_offsets["q"] = 0

    gates: list[tuple[str, list[int]]] = []
    for line in qasm3_code.splitlines():
        stripped = line.strip()
        if not stripped or any(stripped.startswith(p) for p in _SKIP_PREFIXES):
            continue
        m = _GATE_RE.match(line)
        if not m:
            continue
        gate_name = m.group(1).lower()
        args_str = m.group(2)

        # Extract qubit indices from arguments
        qubit_indices = []
        for reg_name, idx_str in _QUBIT_REF_RE.findall(args_str):
            base = reg_offsets.get(reg_name, 0)
            qubit_indices.append(base + int(idx_str))

        if qubit_indices:
            gates.append((gate_name, qubit_indices))

    return gates


def _compute_depth(
    gates: list[tuple[str, list[int]]],
    num_qubits: int,
) -> int:
    """Compute circuit depth via ASAP (as-soon-as-possible) scheduling.

    Each qubit tracks its current layer. A gate occupies
    max(layer of its qubits) + 1. Depth = max layer across all qubits.
    """
    qubit_layer = [0] * num_qubits
    for _, qubits in gates:
        valid_qubits = [q for q in qubits if q < num_qubits]
        if not valid_qubits:
            continue
        gate_layer = max(qubit_layer[q] for q in valid_qubits) + 1
        for q in valid_qubits:
            qubit_layer[q] = gate_layer
    return max(qubit_layer) if qubit_layer else 0


def _compute_program_communication(
    multi_qubit_gates: list[tuple[str, list[int]]],
    num_qubits: int,
) -> tuple[list[tuple[int, int]], float]:
    """Compute program communication: fraction of qubit pairs that interact.

    Returns (interaction_edges, communication_score).
    communication_score = |interacting_pairs| / |all_possible_pairs|
    A value of 1.0 means every qubit interacts with every other qubit.
    """
    interacting_pairs: set[tuple[int, int]] = set()
    for _, qubits in multi_qubit_gates:
        for i in range(len(qubits)):
            for j in range(i + 1, len(qubits)):
                a, b = min(qubits[i], qubits[j]), max(qubits[i], qubits[j])
                if a < num_qubits and b < num_qubits:
                    interacting_pairs.add((a, b))

    max_pairs = num_qubits * (num_qubits - 1) // 2
    communication = len(interacting_pairs) / max_pairs if max_pairs > 0 else 0.0

    return sorted(interacting_pairs), min(communication, 1.0)


def _compute_critical_depth(
    gates: list[tuple[str, list[int]]],
    num_qubits: int,
) -> float:
    """Compute critical depth: fraction of multi-qubit gates on the longest path.

    Uses ASAP scheduling to find the critical path, then counts what fraction
    of the gates on that path are multi-qubit.
    """
    if not gates:
        return 0.0

    # Track per-qubit layers and whether each gate is multi-qubit
    qubit_layer = [0] * num_qubits
    # For each layer on each qubit, track if it was placed by a multi-qubit gate
    layer_multi: dict[tuple[int, int], bool] = {}  # (qubit, layer) -> is_multi

    for _, qubits in gates:
        valid_qubits = [q for q in qubits if q < num_qubits]
        if not valid_qubits:
            continue
        is_multi = len(valid_qubits) >= 2
        gate_layer = max(qubit_layer[q] for q in valid_qubits) + 1
        for q in valid_qubits:
            qubit_layer[q] = gate_layer
            layer_multi[(q, gate_layer)] = is_multi

    depth = max(qubit_layer) if qubit_layer else 0
    if depth == 0:
        return 0.0

    # Find the critical qubit (the one that determines depth)
    critical_qubit = qubit_layer.index(depth)

    # Walk the critical path and count multi-qubit gates
    multi_on_path = 0
    total_on_path = 0
    for layer in range(1, depth + 1):
        if (critical_qubit, layer) in layer_multi:
            total_on_path += 1
            if layer_multi[(critical_qubit, layer)]:
                multi_on_path += 1

    return multi_on_path / total_on_path if total_on_path > 0 else 0.0


def _compute_parallelism(
    gates: list[tuple[str, list[int]]],
    num_qubits: int,
    depth: int,
) -> float:
    """Compute parallelism: ratio of minimum possible depth to actual depth.

    A fully serial circuit has parallelism ≈ 0. A perfectly parallel circuit
    (all gates in one layer) has parallelism ≈ 1.

    parallelism = 1 - (depth / num_gates)  clamped to [0, 1]
    """
    num_gates = len(gates)
    if num_gates <= 1 or depth <= 0:
        return 0.0

    # Parallelism = 1 - (depth / num_gates)
    # When depth == num_gates (fully serial), parallelism = 0
    # When depth == 1 (fully parallel), parallelism = 1 - 1/N ≈ 1
    p = 1.0 - (depth / num_gates)
    return max(0.0, min(1.0, p))
