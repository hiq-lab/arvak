"""Shared data types for arvak-bquant."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class BackendInfo:
    """Summary of a quantum backend."""

    backend_id: str
    name: str
    is_available: bool
    max_qubits: int
    max_shots: int
    supported_gates: list[str] = field(default_factory=list)


@dataclass
class JobStatus:
    """Current status of a submitted job."""

    job_id: str
    status: str  # queued | running | completed | failed | cancelled
    backend_id: str
    shots: int
    submitted_at: int
    started_at: int | None = None
    completed_at: int | None = None
    error_message: str | None = None

    @property
    def is_terminal(self) -> bool:
        return self.status in ("completed", "failed", "cancelled")


@dataclass
class JobResult:
    """Measurement counts returned by a completed job."""

    job_id: str
    counts: dict[str, int]
    shots: int
    execution_time_ms: int | None = None


@dataclass
class CompileResult:
    """Result of a compile-only request."""

    compiled_qasm3: str
    num_qubits: int
    depth: int
    gate_count: int
