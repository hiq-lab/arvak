"""Data classes for Nathan analysis reports."""

from __future__ import annotations

from dataclasses import dataclass, field


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
    """A concrete optimization suggestion with optional QASM3 rewrite."""

    title: str
    description: str
    qasm3: str = ""
    impact: str = ""  # "high", "medium", "low"

    @property
    def circuit(self):
        """Convert the QASM3 suggestion to an arvak.Circuit (if available)."""
        if not self.qasm3:
            return None
        try:
            import arvak
            return arvak.from_qasm(self.qasm3)
        except Exception:
            return None

    def __repr__(self) -> str:
        return f"Suggestion({self.title!r}, impact={self.impact!r})"


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
