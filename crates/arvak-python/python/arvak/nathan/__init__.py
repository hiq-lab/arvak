"""arvak.nathan — AI-powered research optimizer for quantum computing.

Analyze quantum circuits against 1700+ papers and get optimization
suggestions, hardware fit analysis, and relevant literature.

Example:
    >>> import arvak
    >>> qc = arvak.Circuit("bell", num_qubits=2)
    >>> qc.h(0).cx(0, 1)
    >>> report = arvak.nathan.analyze(qc)
    >>> print(report.summary)

    >>> # Analyze raw QASM3
    >>> report = arvak.nathan.analyze("OPENQASM 3.0; qubit[5] q; ...")

    >>> # Framework circuits (auto-converted)
    >>> from qiskit import QuantumCircuit
    >>> qc = QuantumCircuit(4)
    >>> report = arvak.nathan.analyze(qc)

    >>> # Hardware-specific analysis
    >>> report = arvak.nathan.analyze(qc, backend="iqm_garnet")
"""

from __future__ import annotations

import logging
import os
from pathlib import Path
from typing import TYPE_CHECKING

from .client import NathanClient
from .report import AnalysisReport, ChatResponse, Paper, Suggestion
from .clifford import analyze_clifford_content, generate_clifford_suggestions
from .verify import VerificationResult, VerificationStatus, verify_equivalence
from .bench import BenchReference
from .noise import FidelityEstimate
from .qecc import QecRecommendation
from .session import Session, SessionDiff, SessionEntry

if TYPE_CHECKING:
    pass

logger = logging.getLogger(__name__)

# Module-level client (lazy-initialized)
_client: NathanClient | None = None
_api_key: str | None = None
_api_url: str = "https://arvak.io/api/nathan"

# P2 #8 — module-level knowledge sources
_knowledge_sources: list = []


def configure(
    api_key: str | None = None,
    api_url: str | None = None,
) -> None:
    """Configure Nathan API credentials.

    Args:
        api_key: API key for authenticated access (higher rate limits).
                 Can also be set via ARVAK_NATHAN_API_KEY environment variable.
        api_url: Override the Nathan API URL (default: https://arvak.io/api/nathan).
    """
    global _client, _api_key, _api_url

    if api_key is not None:
        _api_key = api_key
    if api_url is not None:
        _api_url = api_url

    # Reset client so it picks up new config
    _client = None


def _get_client() -> NathanClient:
    """Get or create the Nathan API client."""
    global _client

    if _client is None:
        key = _api_key or os.environ.get("ARVAK_NATHAN_API_KEY", "")
        _client = NathanClient(api_url=_api_url, api_key=key)

    return _client


def add_source(path_or_content: str | Path) -> None:
    """Register a persistent knowledge source for all future analyze() calls.

    Args:
        path_or_content: A file path (string or Path) or raw QASM3/text content.
            Paths are read at call time; raw strings are embedded as-is.

    Example:
        >>> arvak.nathan.add_source("/path/to/reference.qasm")
        >>> arvak.nathan.add_source("OPENQASM 3.0; // custom knowledge")
    """
    _knowledge_sources.append(path_or_content)


def clear_sources() -> None:
    """Remove all registered knowledge sources."""
    _knowledge_sources.clear()


def _load_knowledge_sources(sources: list | None) -> str | None:
    """Load and concatenate knowledge source content.

    Args:
        sources: List of file paths or raw strings.

    Returns:
        Concatenated string separated by ``---``, or None if empty.
    """
    if not sources:
        return None
    parts = []
    for s in sources:
        p = Path(s) if isinstance(s, (str, Path)) else None
        if p is not None and p.exists():
            parts.append(p.read_text(encoding="utf-8"))
        else:
            parts.append(str(s))
    return "\n\n---\n\n".join(parts) if parts else None


def analyze(
    circuit,
    backend: str | None = None,
    language: str | None = None,
    anonymize: bool = True,
    verify: bool = True,
    optimize_clifford: bool = True,
    predict_device: bool = False,
    show_references: bool = True,
    estimate_fidelity: bool = False,
    knowledge_sources: list | None = None,
) -> AnalysisReport:
    """Analyze a quantum circuit and get optimization suggestions.

    Accepts:
    - arvak.Circuit objects
    - Raw QASM3 strings
    - Qiskit QuantumCircuit (auto-converted via arvak.get_integration('qiskit'))
    - PennyLane QNode/tape (auto-converted via arvak.get_integration('pennylane'))
    - Cirq Circuit (auto-converted via arvak.get_integration('cirq'))

    Args:
        circuit: Quantum circuit in any supported format.
        backend: Optional backend ID for hardware-specific analysis
                 (e.g., "iqm_garnet", "ibm_eagle", "aer_simulator").
        language: Override language detection ("qasm3", "qiskit", "pennylane", "cirq").
        anonymize: Anonymize code before sending to API (default: True).
                   Strips comments, normalizes variable names, and removes
                   string literals to protect proprietary information.
        verify: Verify QASM3 rewrites against the original circuit using
                MQT QCEC (default: True).  Requires ``pip install mqt.qcec``.
                If QCEC is not installed, suggestions are returned unverified.
        optimize_clifford: Detect and optimize Clifford subcircuits using
                MQT QMAP's SAT-based synthesis (default: True).  Requires
                ``pip install mqt.qmap``.  Clifford suggestions are provably
                optimal and marked with ``source="qmap_sat"``.
        predict_device: Run ML-based device selection using MQT Predictor
                (default: False).  Requires ``pip install mqt.predictor``.
                Falls back to heuristic ranking if not installed.  Populates
                ``report.recommended_device`` and ``report.device_ranking``.
        show_references: Cross-reference problem_type against MQT Bench and
                populate ``report.reference_circuits`` (default: True).
        estimate_fidelity: Run noise-aware fidelity simulation via DDSIM
                (default: False — expensive).  Populates
                ``report.simulated_fidelity`` and ``report.fidelity_estimate``.
                Falls back to heuristic estimate when mqt.ddsim is not installed.
        knowledge_sources: List of file paths or QASM3/text strings to embed
                as additional context in the analyze request.  Combined with
                any sources registered via ``add_source()``.

    Returns:
        AnalysisReport with summary, suggestions, papers, and circuit stats.
        Suggestions with QASM3 rewrites will have ``verified=True`` if
        proven equivalent, ``verified=False`` if proven non-equivalent,
        or ``verified=None`` if verification was skipped.
        Clifford-optimal suggestions have ``source="qmap_sat"`` and
        ``is_optimal == True``.

    Example:
        >>> report = arvak.nathan.analyze(circuit)
        >>> print(report.problem_type)
        "qaoa"
        >>> print(report.suitability)
        0.72
        >>> for s in report.suggestions:
        ...     print(s.title, s.impact, s.verified, s.source)
    """
    qasm3_code, detected_lang = _to_qasm3(circuit, language)

    # Combine per-call + module-level knowledge sources
    all_sources: list = list(_knowledge_sources)
    if knowledge_sources:
        all_sources.extend(knowledge_sources)
    extra_context = _load_knowledge_sources(all_sources) if all_sources else None

    client = _get_client()
    report = client.analyze(
        code=qasm3_code,
        language=detected_lang,
        backend_id=backend,
        anonymize=anonymize,
        extra_context=extra_context,
    )

    # Store original circuit reference on report (P2 #7)
    report._original_circuit = circuit

    # Verify suggestions with QASM3 rewrites via MQT QCEC
    if verify and report.suggestions:
        from .verify import verify_suggestions

        verify_suggestions(qasm3_code, report.suggestions)

    # Generate Clifford-optimal suggestions via MQT QMAP
    if optimize_clifford:
        from .clifford import generate_clifford_suggestions

        clifford_suggestions = generate_clifford_suggestions(qasm3_code)
        if clifford_suggestions:
            # Prepend Clifford suggestions (higher priority than LLM suggestions)
            report.suggestions = clifford_suggestions + report.suggestions

    # ML-based device selection via MQT Predictor
    if predict_device:
        from arvak.predictor import predict_device as _predict_device

        try:
            prediction = _predict_device(qasm3_code)
            report.recommended_device = prediction.device
            report.device_ranking = prediction.ranking
        except Exception as e:
            logger.warning("Device prediction failed: %s", e)

    # P1 #3 — MQT Bench reference circuits
    if show_references:
        from .bench import find_references

        num_qubits = report.circuit.num_qubits if report.circuit else 0
        report.reference_circuits = find_references(
            report.problem_type, num_qubits
        )

    # P1 #4 — DDSIM noise-aware fidelity
    if estimate_fidelity and backend:
        from .noise import estimate_fidelity as _estimate_fidelity

        try:
            fe = _estimate_fidelity(qasm3_code, backend)
            report.simulated_fidelity = fe.fidelity
            report.fidelity_estimate = fe
        except Exception as e:
            logger.warning("Fidelity estimation failed: %s", e)

    # P1 #5 — QEC suggestions when suitability < 0.4
    if report.suitability < 0.4:
        from .qecc import recommend_qec

        num_logical = report.circuit.num_qubits if report.circuit else report.estimated_qubits
        qec = recommend_qec(
            num_logical_qubits=num_logical,
            estimated_error_rate=report.estimated_error_rate,
            suitability=report.suitability,
        )
        if qec is not None:
            report.qec_recommendation = qec
            # Add a Suggestion entry for QEC
            report.suggestions.append(
                Suggestion(
                    title=f"Apply {qec.code.replace('_', ' ').title()}",
                    description=(
                        f"Circuit error rate is too high for direct execution "
                        f"(suitability {report.suitability:.0%}). "
                        f"Use {qec.code.replace('_', ' ')} at distance {qec.distance} "
                        f"({qec.physical_qubits} physical qubits for {qec.logical_qubits} logical). "
                        f"Threshold: {qec.threshold * 100:.2f}%."
                    ),
                    impact="high",
                    verified=True,  # deterministic, not LLM
                    verification_status="verified",
                    source="qecc",
                )
            )

    return report


def chat(message: str, context: str = "") -> ChatResponse:
    """Ask Nathan a question about quantum computing.

    Args:
        message: Your question or message.
        context: Optional conversation context from previous exchanges.

    Returns:
        ChatResponse with message and relevant paper references.

    Example:
        >>> resp = arvak.nathan.chat("What's the best algorithm for MaxCut?")
        >>> print(resp.message)
    """
    client = _get_client()
    return client.chat(message=message, context=context)


def _to_qasm3(circuit, language: str | None = None) -> tuple[str, str]:
    """Convert any supported circuit format to QASM3 string.

    Returns (qasm3_code, language).
    """
    # Already a string — assume QASM3 or framework code
    if isinstance(circuit, str):
        return circuit, language or "qasm3"

    # Arvak Circuit — use built-in conversion
    try:
        import arvak as _arvak

        if isinstance(circuit, _arvak.Circuit):
            return _arvak.to_qasm(circuit), "qasm3"
    except (ImportError, AttributeError):
        pass

    # Qiskit QuantumCircuit
    _type_name = type(circuit).__module__ + "." + type(circuit).__qualname__
    if "qiskit" in _type_name.lower():
        try:
            import arvak as _arvak

            integration = _arvak.get_integration("qiskit")
            arvak_circuit = integration.to_arvak(circuit)
            return _arvak.to_qasm(arvak_circuit), "qasm3"
        except Exception:
            # Fallback: try qiskit's own QASM3 export
            try:
                from qiskit.qasm3 import dumps
                return dumps(circuit), "qasm3"
            except ImportError:
                pass

    # Cirq Circuit
    if "cirq" in _type_name.lower():
        try:
            import arvak as _arvak

            integration = _arvak.get_integration("cirq")
            arvak_circuit = integration.to_arvak(circuit)
            return _arvak.to_qasm(arvak_circuit), "qasm3"
        except Exception:
            pass

    # PennyLane QNode
    if "pennylane" in _type_name.lower():
        try:
            import arvak as _arvak

            integration = _arvak.get_integration("pennylane")
            arvak_circuit = integration.to_arvak(circuit)
            return _arvak.to_qasm(arvak_circuit), "qasm3"
        except Exception:
            pass

    raise TypeError(
        f"Unsupported circuit type: {type(circuit).__name__}. "
        "Pass an arvak.Circuit, QASM3 string, or a Qiskit/PennyLane/Cirq circuit."
    )


__all__ = [
    "analyze",
    "analyze_clifford_content",
    "chat",
    "configure",
    "generate_clifford_suggestions",
    "verify_equivalence",
    "add_source",
    "clear_sources",
    "AnalysisReport",
    "BenchReference",
    "ChatResponse",
    "FidelityEstimate",
    "Paper",
    "QecRecommendation",
    "Session",
    "SessionDiff",
    "SessionEntry",
    "Suggestion",
    "VerificationResult",
    "VerificationStatus",
]
