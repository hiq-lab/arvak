"""arvak.predictor — ML-based quantum device selection.

Extracts circuit features and predicts the optimal quantum device using
MQT Predictor's supervised-ML models. Provides the Arvak-side foundation
for Garm's intelligent routing (Garm Roadmap P0 #1).

Feature extraction works without MQT Predictor installed. Device prediction
requires ``pip install mqt.predictor``.

Example:
    >>> import arvak
    >>> qc = arvak.Circuit("bell", num_qubits=2)
    >>> qc.h(0).cx(0, 1)
    >>> features = arvak.predictor.extract_features(qc)
    >>> print(features)
    CircuitFeatures(qubits=2, depth=2, gates=2, ...)

    >>> # With MQT Predictor installed:
    >>> prediction = arvak.predictor.predict_device(qc)
    >>> print(prediction.device, prediction.figure_of_merit)
"""

from __future__ import annotations

from .features import CircuitFeatures, extract_features
from .device import DevicePrediction, predict_device, rank_devices, _predictor_available

__all__ = [
    "CircuitFeatures",
    "DevicePrediction",
    "extract_features",
    "predict_device",
    "rank_devices",
]
