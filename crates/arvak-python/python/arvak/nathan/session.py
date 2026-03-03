"""P2 #6 — Nathan Session class for multi-turn optimization workflows.

A Session tracks circuit versions through iterative optimization steps,
maintaining a history of (circuit, report, applied_suggestion) tuples.

Example:
    >>> session = arvak.nathan.Session(circuit, backend="iqm_garnet")
    >>> report = session.analyze()
    >>> session.apply(0)  # apply first suggestion
    >>> diff = session.compare()
    >>> print(diff.gate_reduction_pct)
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .report import AnalysisReport, Suggestion


@dataclass
class SessionEntry:
    """One entry in the session history."""

    circuit: object                          # arvak.Circuit or QASM3 string
    report: AnalysisReport
    applied_suggestion: Suggestion | None    # None = initial analysis


@dataclass
class SessionDiff:
    """Difference between original and current circuit state."""

    original_gates: int
    current_gates: int
    gate_reduction: int
    gate_reduction_pct: float
    original_depth: int
    current_depth: int
    depth_reduction: int
    depth_reduction_pct: float
    suggestions_applied: int
    verified_rewrites: int

    def __repr__(self) -> str:
        return (
            f"SessionDiff(gate_reduction={self.gate_reduction_pct:+.1%}, "
            f"depth_reduction={self.depth_reduction_pct:+.1%}, "
            f"applied={self.suggestions_applied})"
        )


class Session:
    """Track a circuit through an iterative Nathan optimization workflow.

    Args:
        circuit: Initial quantum circuit (arvak.Circuit, QASM3 string,
                 Qiskit QuantumCircuit, etc.)
        backend: Optional backend ID for hardware-specific analysis.
        **analyze_kwargs: Additional keyword arguments forwarded to
            ``arvak.nathan.analyze()`` on every call.

    Example:
        >>> session = Session(circuit, backend="iqm_garnet", verify=True)
        >>> report = session.analyze()
        >>> session.apply(0).apply(1)
        >>> diff = session.compare()
    """

    def __init__(self, circuit, backend: str | None = None, **analyze_kwargs):
        self._original = circuit
        self._current = circuit
        self._backend = backend
        self._analyze_kwargs = analyze_kwargs
        self._history: list[SessionEntry] = []
        self._last_report: AnalysisReport | None = None

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def analyze(self) -> AnalysisReport:
        """Analyze the current circuit and record in history.

        Returns:
            AnalysisReport for the current circuit.
        """
        from . import analyze as _analyze

        report = _analyze(self._current, backend=self._backend, **self._analyze_kwargs)
        self._last_report = report
        self._history.append(
            SessionEntry(
                circuit=self._current,
                report=report,
                applied_suggestion=None,
            )
        )
        return report

    def apply(self, suggestion_index: int) -> "Session":
        """Apply suggestion at *suggestion_index* from the last report.

        Replaces the current circuit with the suggestion's rewrite,
        then re-analyzes so the next report reflects the new state.

        Args:
            suggestion_index: Index into ``report.suggestions``.

        Returns:
            self — allows chaining: ``session.apply(0).apply(1)``

        Raises:
            RuntimeError: If no analysis has been run yet.
            IndexError: If suggestion_index is out of range.
            ValueError: If the suggestion has no QASM3 rewrite or
                        was proven non-equivalent.
        """
        if self._last_report is None:
            raise RuntimeError(
                "No analysis has been run yet. Call session.analyze() first."
            )

        suggestions = self._last_report.suggestions
        if suggestion_index < 0 or suggestion_index >= len(suggestions):
            raise IndexError(
                f"suggestion_index {suggestion_index} out of range "
                f"(0 – {len(suggestions) - 1})"
            )

        suggestion = suggestions[suggestion_index]

        if not suggestion.qasm3:
            raise ValueError(
                f"Suggestion {suggestion_index} ({suggestion.title!r}) "
                "has no QASM3 rewrite and cannot be applied."
            )

        if suggestion.verified is False:
            raise ValueError(
                f"Suggestion {suggestion_index} ({suggestion.title!r}) "
                "was proven non-equivalent and cannot be applied."
            )

        new_circuit = suggestion.circuit
        if new_circuit is None:
            # Fallback: use QASM3 string directly
            new_circuit = suggestion.qasm3

        prev_report = self._last_report

        # Update state
        self._current = new_circuit

        # Re-analyze with the new circuit
        from . import analyze as _analyze

        new_report = _analyze(
            self._current, backend=self._backend, **self._analyze_kwargs
        )
        self._last_report = new_report

        # Record history entry with the applied suggestion
        self._history.append(
            SessionEntry(
                circuit=new_circuit,
                report=new_report,
                applied_suggestion=suggestion,
            )
        )

        return self

    def compare(self) -> SessionDiff:
        """Diff the original circuit stats against the current circuit stats.

        Uses CircuitStats from the first and last analysis reports.
        If no analysis has been run, all values are zero.

        Returns:
            SessionDiff with gate/depth reduction metrics.
        """
        orig_gates = 0
        orig_depth = 0
        curr_gates = 0
        curr_depth = 0

        if self._history:
            first = self._history[0].report
            last = self._history[-1].report

            if first.circuit:
                orig_gates = first.circuit.total_gates
                orig_depth = first.circuit.depth
            if last.circuit:
                curr_gates = last.circuit.total_gates
                curr_depth = last.circuit.depth

        gate_reduction = orig_gates - curr_gates
        gate_pct = gate_reduction / orig_gates if orig_gates else 0.0
        depth_reduction = orig_depth - curr_depth
        depth_pct = depth_reduction / orig_depth if orig_depth else 0.0

        suggestions_applied = sum(
            1 for e in self._history if e.applied_suggestion is not None
        )
        verified_rewrites = sum(
            1
            for e in self._history
            if e.applied_suggestion is not None
            and getattr(e.applied_suggestion, "verified", None) is True
        )

        return SessionDiff(
            original_gates=orig_gates,
            current_gates=curr_gates,
            gate_reduction=gate_reduction,
            gate_reduction_pct=gate_pct,
            original_depth=orig_depth,
            current_depth=curr_depth,
            depth_reduction=depth_reduction,
            depth_reduction_pct=depth_pct,
            suggestions_applied=suggestions_applied,
            verified_rewrites=verified_rewrites,
        )

    def reset(self) -> "Session":
        """Reset to the original circuit and clear history.

        Returns:
            self
        """
        self._current = self._original
        self._history = []
        self._last_report = None
        return self

    # ------------------------------------------------------------------
    # Properties
    # ------------------------------------------------------------------

    @property
    def history(self) -> list[SessionEntry]:
        """Full list of (circuit, report, applied_suggestion) entries."""
        return list(self._history)

    @property
    def current(self):
        """The current circuit (possibly rewritten)."""
        return self._current

    @property
    def report(self) -> AnalysisReport | None:
        """The most recent analysis report, or None if not yet analyzed."""
        return self._last_report

    def __repr__(self) -> str:
        steps = len(self._history)
        applied = sum(1 for e in self._history if e.applied_suggestion is not None)
        return (
            f"Session(steps={steps}, applied={applied}, "
            f"backend={self._backend!r})"
        )
