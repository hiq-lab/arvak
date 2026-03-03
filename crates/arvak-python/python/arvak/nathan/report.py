"""Data classes for Nathan analysis reports."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .bench import BenchReference
    from .noise import FidelityEstimate
    from .qecc import QecRecommendation


@dataclass
class Paper:
    """A reference to a relevant research paper."""

    title: str
    arxiv_url: str
    problem_type: str = ""
    algorithm: str = ""
    relevance: str = ""

    def __repr__(self) -> str:
        return f"Paper({self.title!r}, {self.arxiv_url!r})"


@dataclass
class Suggestion:
    """A concrete optimization suggestion with optional QASM3 rewrite.

    When verification is available (mqt.qcec installed), suggestions with
    QASM3 rewrites are checked for semantic equivalence with the original
    circuit.  The ``verified`` field indicates the result.
    """

    title: str
    description: str
    qasm3: str = ""
    impact: str = ""  # "high", "medium", "low"
    verified: bool | None = None  # None = not checked, True/False = QCEC result
    verification_status: str = "not_checked"  # "verified", "not_equivalent", "timeout", "error", "not_checked"
    verification_message: str = ""
    source: str = "nathan_llm"  # "nathan_llm", "qmap_sat", "qmap_heuristic"

    @property
    def circuit(self):
        """Convert the QASM3 suggestion to an arvak.Circuit (if available).

        Only returns a circuit if the suggestion has been verified as
        equivalent (or verification was not performed).  Returns None
        for suggestions proven non-equivalent.
        """
        if not self.qasm3:
            return None
        if self.verified is False:
            return None
        try:
            import arvak
            return arvak.from_qasm(self.qasm3)
        except Exception:
            return None

    @property
    def is_optimal(self) -> bool:
        """Whether this suggestion is provably optimal (SAT-based synthesis)."""
        return self.source == "qmap_sat"

    def __repr__(self) -> str:
        verified_str = ""
        if self.verified is True:
            verified_str = ", verified=True"
        elif self.verified is False:
            verified_str = ", verified=False"
        source_str = ""
        if self.source != "nathan_llm":
            source_str = f", source={self.source!r}"
        return f"Suggestion({self.title!r}, impact={self.impact!r}{verified_str}{source_str})"


@dataclass
class CircuitStats:
    """Parsed circuit structure statistics."""

    num_qubits: int = 0
    total_gates: int = 0
    gate_breakdown: str = ""
    depth: int = 0
    detected_pattern: str = "unknown"
    language: str = "qasm3"

    def __repr__(self) -> str:
        return (
            f"CircuitStats(qubits={self.num_qubits}, gates={self.total_gates}, "
            f"depth={self.depth}, pattern={self.detected_pattern!r})"
        )


@dataclass
class AnalysisReport:
    """Complete analysis report from Nathan.

    Attributes:
        summary: Human-readable analysis summary (markdown).
        problem_type: Detected problem type (e.g., "qaoa", "vqe", "grover").
        suitability: Quantum suitability score (0.0 - 1.0).
        recommended_algorithm: Best algorithm for this problem.
        estimated_qubits: Estimated qubit requirement.
        circuit: Parsed circuit statistics.
        papers: Relevant research papers with arXiv links.
        suggestions: Optimization suggestions with QASM3 rewrites.
        hardware_fit: Hardware compatibility assessment.
        estimated_error_rate: Estimated circuit error rate.
        recommended_shots: Recommended number of measurement shots.
    """

    summary: str = ""
    problem_type: str = "unknown"
    suitability: float = 0.0
    recommended_algorithm: str = ""
    estimated_qubits: int = 0
    circuit: CircuitStats | None = None
    papers: list[Paper] = field(default_factory=list)
    suggestions: list[Suggestion] = field(default_factory=list)
    hardware_fit: str = ""
    estimated_error_rate: str = ""
    recommended_shots: int = 1024
    recommended_device: str = ""
    device_ranking: list = field(default_factory=list)  # list[DeviceScore]
    # P1 #3 — MQT Bench references
    reference_circuits: list = field(default_factory=list)  # list[BenchReference]
    # P1 #4 — DDSIM fidelity
    simulated_fidelity: float | None = None
    fidelity_estimate: object | None = None  # FidelityEstimate | None
    # P1 #5 — QEC recommendation
    qec_recommendation: object | None = None  # QecRecommendation | None
    # P2 #7 — original circuit reference (set by analyze())
    _original_circuit: object = field(default=None, repr=False)

    def apply(self, index: int):
        """Apply suggestion[index] rewrite and return a new arvak.Circuit.

        Args:
            index: Index into ``suggestions`` list.

        Returns:
            A new ``arvak.Circuit`` built from the suggestion's QASM3 rewrite.

        Raises:
            IndexError: If index is out of range.
            ValueError: If the suggestion has no qasm3 or is not verified.
        """
        if index < 0 or index >= len(self.suggestions):
            raise IndexError(
                f"Index {index} out of range (0 – {len(self.suggestions) - 1})"
            )
        suggestion = self.suggestions[index]
        if not suggestion.qasm3:
            raise ValueError(
                f"Suggestion {index} ({suggestion.title!r}) has no QASM3 rewrite "
                "and cannot be applied."
            )
        if suggestion.verified is False:
            raise ValueError(
                f"Suggestion {index} ({suggestion.title!r}) was proven non-equivalent "
                f"(verified={suggestion.verified}) and cannot be applied."
            )
        circuit = suggestion.circuit
        if circuit is None:
            raise ValueError(
                f"Suggestion {index} ({suggestion.title!r}) could not be converted "
                "to an arvak.Circuit (check that arvak is installed)."
            )
        return circuit

    @property
    def original_circuit(self):
        """The original circuit passed to analyze()."""
        return self._original_circuit

    def _repr_html_(self) -> str:
        """Rich HTML rendering for Jupyter notebooks."""
        from .display import report_to_html
        return report_to_html(self)

    def __repr__(self) -> str:
        return (
            f"AnalysisReport(problem_type={self.problem_type!r}, "
            f"suitability={self.suitability:.1%}, "
            f"papers={len(self.papers)}, suggestions={len(self.suggestions)})"
        )


@dataclass
class ChatResponse:
    """Response from Nathan chat."""

    message: str = ""
    papers: list[Paper] = field(default_factory=list)

    def _repr_html_(self) -> str:
        """Rich HTML rendering for Jupyter notebooks."""
        from .display import chat_to_html
        return chat_to_html(self)

    def __repr__(self) -> str:
        return f"ChatResponse(papers={len(self.papers)})"
