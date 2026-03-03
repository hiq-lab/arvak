"""Device prediction and ranking via MQT Predictor.

Wraps MQT Predictor's ML-based device selection for use with Arvak circuits.
Falls back to heuristic ranking when MQT Predictor is not installed.

Requires: ``pip install mqt.predictor``
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field

from .features import CircuitFeatures, extract_features

logger = logging.getLogger(__name__)


@dataclass
class DevicePrediction:
    """Result of ML-based device prediction."""

    device: str
    figure_of_merit: str = "expected_fidelity"
    confidence: float = 0.0
    features: CircuitFeatures | None = None
    method: str = "heuristic"  # "mqt_predictor" or "heuristic"
    ranking: list[DeviceScore] = field(default_factory=list)

    def __repr__(self) -> str:
        return (
            f"DevicePrediction(device={self.device!r}, "
            f"fom={self.figure_of_merit!r}, "
            f"method={self.method!r})"
        )


@dataclass
class DeviceScore:
    """Score for a single device in the ranking."""

    device: str
    score: float
    reason: str = ""

    def __repr__(self) -> str:
        return f"DeviceScore({self.device!r}, score={self.score:.3f})"


# Known quantum devices and their characteristics for heuristic ranking
KNOWN_DEVICES: dict[str, dict] = {
    "ibm_torino": {
        "num_qubits": 133,
        "topology": "heavy_hex",
        "native_gates": {"cx", "id", "rz", "sx", "x"},
        "is_simulator": False,
    },
    "ibm_brisbane": {
        "num_qubits": 127,
        "topology": "heavy_hex",
        "native_gates": {"cx", "id", "rz", "sx", "x"},
        "is_simulator": False,
    },
    "iqm_garnet": {
        "num_qubits": 20,
        "topology": "square_lattice",
        "native_gates": {"prx", "cz"},
        "is_simulator": False,
    },
    "quantinuum_h1": {
        "num_qubits": 20,
        "topology": "full",
        "native_gates": {"rz", "u1q", "zz"},
        "is_simulator": False,
    },
    "ionq_aria": {
        "num_qubits": 25,
        "topology": "full",
        "native_gates": {"gpi", "gpi2", "ms"},
        "is_simulator": False,
    },
    "rigetti_ankaa": {
        "num_qubits": 84,
        "topology": "octagonal",
        "native_gates": {"rx", "rz", "cz"},
        "is_simulator": False,
    },
}


def _predictor_available() -> bool:
    """Check if mqt.predictor is installed."""
    try:
        from mqt.predictor import qcompile  # noqa: F401
        return True
    except ImportError:
        return False


def predict_device(
    circuit,
    figure_of_merit: str = "expected_fidelity",
    devices: list[str] | None = None,
) -> DevicePrediction:
    """Predict the best quantum device for a circuit.

    Uses MQT Predictor's supervised-ML model when available, otherwise
    falls back to heuristic ranking based on circuit features.

    Args:
        circuit: Quantum circuit (QASM3 string, arvak.Circuit, or Qiskit).
        figure_of_merit: Optimization target — "expected_fidelity" (default),
                         "critical_depth", or "gate_count".
        devices: Optional list of device names to consider. If None, uses
                 all known devices.

    Returns:
        DevicePrediction with recommended device and ranking.
    """
    features = extract_features(circuit)

    if _predictor_available():
        try:
            return _predict_with_mqt(circuit, features, figure_of_merit)
        except Exception as e:
            logger.warning("MQT Predictor failed, falling back to heuristic: %s", e)

    return _predict_heuristic(features, figure_of_merit, devices)


def rank_devices(
    circuit,
    figure_of_merit: str = "expected_fidelity",
    devices: list[str] | None = None,
) -> list[DeviceScore]:
    """Rank all candidate devices for a circuit.

    Args:
        circuit: Quantum circuit in any supported format.
        figure_of_merit: Optimization target.
        devices: Optional list of device names. If None, uses all known.

    Returns:
        Sorted list of DeviceScore (best first).
    """
    prediction = predict_device(circuit, figure_of_merit, devices)
    return prediction.ranking


def _predict_with_mqt(
    circuit,
    features: CircuitFeatures,
    figure_of_merit: str,
) -> DevicePrediction:
    """Use MQT Predictor for device selection."""
    from mqt.predictor import qcompile

    # MQT Predictor accepts QASM strings or Qiskit circuits
    qasm_input = circuit if isinstance(circuit, str) else None
    if qasm_input is None:
        try:
            import arvak as _arvak
            if isinstance(circuit, _arvak.Circuit):
                qasm_input = _arvak.to_qasm(circuit)
        except (ImportError, AttributeError):
            pass

    if qasm_input is None:
        # Pass through as-is (might be a Qiskit circuit)
        compiled_qc, compilation_info, device = qcompile(
            qc=circuit,
            figure_of_merit=figure_of_merit,
        )
    else:
        compiled_qc, compilation_info, device = qcompile(
            qc=qasm_input,
            figure_of_merit=figure_of_merit,
        )

    device_name = str(device) if device else "unknown"

    return DevicePrediction(
        device=device_name,
        figure_of_merit=figure_of_merit,
        confidence=1.0,
        features=features,
        method="mqt_predictor",
        ranking=[DeviceScore(device=device_name, score=1.0, reason="MQT Predictor ML selection")],
    )


def _predict_heuristic(
    features: CircuitFeatures,
    figure_of_merit: str,
    devices: list[str] | None = None,
) -> DevicePrediction:
    """Heuristic device selection based on circuit features.

    Scores devices by:
    1. Qubit capacity (circuit must fit)
    2. Topology match (high communication → full connectivity preferred)
    3. Gate set affinity
    """
    candidates = devices or list(KNOWN_DEVICES.keys())
    scores: list[DeviceScore] = []

    for device_name in candidates:
        device_info = KNOWN_DEVICES.get(device_name)
        if not device_info:
            continue

        score, reason = _score_device(features, device_info, figure_of_merit)
        scores.append(DeviceScore(device=device_name, score=score, reason=reason))

    # Sort by score descending
    scores.sort(key=lambda s: s.score, reverse=True)

    best = scores[0] if scores else DeviceScore(device="unknown", score=0.0, reason="no devices available")

    return DevicePrediction(
        device=best.device,
        figure_of_merit=figure_of_merit,
        confidence=best.score,
        features=features,
        method="heuristic",
        ranking=scores,
    )


def _score_device(
    features: CircuitFeatures,
    device_info: dict,
    figure_of_merit: str,
) -> tuple[float, str]:
    """Score a device for the given circuit features.

    Returns (score, reason) where score is 0.0-1.0.
    """
    score = 0.0
    reasons: list[str] = []

    device_qubits = device_info.get("num_qubits", 0)

    # 1. Qubit capacity — circuit must fit
    if features.num_qubits > device_qubits:
        return 0.0, f"circuit needs {features.num_qubits} qubits, device has {device_qubits}"

    # Qubit utilization: prefer devices where circuit uses 10-80% of capacity
    utilization = features.num_qubits / device_qubits if device_qubits > 0 else 0
    if 0.1 <= utilization <= 0.8:
        score += 0.2
        reasons.append(f"good qubit utilization ({utilization:.0%})")
    elif utilization > 0.8:
        score += 0.1
        reasons.append(f"tight qubit fit ({utilization:.0%})")
    else:
        score += 0.05
        reasons.append(f"device oversized ({utilization:.0%} utilization)")

    # 2. Topology match
    topology = device_info.get("topology", "unknown")
    if topology == "full":
        # Full connectivity — always good, especially for high communication circuits
        score += 0.3
        reasons.append("full connectivity")
    elif features.program_communication > 0.5:
        # High communication needs good connectivity
        if topology in ("heavy_hex", "square_lattice"):
            score += 0.15
            reasons.append(f"partial connectivity ({topology})")
        else:
            score += 0.1
    else:
        # Low communication — any topology works
        score += 0.2
        reasons.append("low connectivity requirement")

    # 3. Simulator vs QPU preference
    is_simulator = device_info.get("is_simulator", False)
    if not is_simulator:
        score += 0.2
        reasons.append("real QPU")
    else:
        score += 0.1
        reasons.append("simulator")

    # 4. Figure of merit adjustments
    if figure_of_merit == "expected_fidelity":
        # Prefer ion trap (full connectivity, lower error) for fidelity
        if topology == "full":
            score += 0.15
            reasons.append("full connectivity favors fidelity")
        # Penalize very deep circuits on noisy devices
        if features.depth > 100 and not is_simulator:
            score -= 0.1
            reasons.append("deep circuit on noisy hardware")
    elif figure_of_merit == "critical_depth":
        # Prefer devices with more qubits for parallelism
        if device_qubits > features.num_qubits * 2:
            score += 0.1
            reasons.append("extra qubits for routing")

    # Normalize to [0, 1]
    score = max(0.0, min(1.0, score))

    return score, "; ".join(reasons)
