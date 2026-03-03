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
from typing import TYPE_CHECKING

from .client import NathanClient
from .report import AnalysisReport, ChatResponse, Paper, Suggestion
from .clifford import analyze_clifford_content, generate_clifford_suggestions
from .verify import VerificationResult, VerificationStatus, verify_equivalence

if TYPE_CHECKING:
    pass

logger = logging.getLogger(__name__)

# Module-level client (lazy-initialized)
_client: NathanClient | None = None
_api_key: str | None = None
_api_url: str = "https://arvak.io/api/nathan"


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


def analyze(
    circuit,
    backend: str | None = None,
    language: str | None = None,
    anonymize: bool = True,
    verify: bool = True,
    optimize_clifford: bool = True,
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
    client = _get_client()
    report = client.analyze(
        code=qasm3_code,
        language=detected_lang,
        backend_id=backend,
        anonymize=anonymize,
    )

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
    "AnalysisReport",
    "ChatResponse",
    "Paper",
    "Suggestion",
    "VerificationResult",
    "VerificationStatus",
]
