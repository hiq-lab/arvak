"""Type definitions for the Arvak gRPC client."""

from dataclasses import dataclass
from datetime import datetime
from enum import IntEnum
from typing import Dict, Optional


class JobState(IntEnum):
    """Job execution state."""
    UNSPECIFIED = 0
    QUEUED = 1
    RUNNING = 2
    COMPLETED = 3
    FAILED = 4
    CANCELED = 5


@dataclass
class Job:
    """Job metadata and status."""
    job_id: str
    state: JobState
    submitted_at: datetime
    backend_id: str
    shots: int
    started_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None
    error_message: Optional[str] = None

    @property
    def is_terminal(self) -> bool:
        """Check if the job is in a terminal state."""
        return self.state in (JobState.COMPLETED, JobState.FAILED, JobState.CANCELED)

    @property
    def is_pending(self) -> bool:
        """Check if the job is still pending."""
        return self.state in (JobState.QUEUED, JobState.RUNNING)

    @property
    def is_success(self) -> bool:
        """Check if the job completed successfully."""
        return self.state == JobState.COMPLETED


@dataclass
class JobResult:
    """Result of circuit execution."""
    job_id: str
    counts: Dict[str, int]
    shots: int
    execution_time_ms: Optional[int] = None
    metadata: Optional[Dict] = None

    def probabilities(self) -> Dict[str, float]:
        """Get probabilities for each bitstring."""
        total = sum(self.counts.values())
        if total == 0:
            return {}
        return {k: v / total for k, v in self.counts.items()}

    def most_frequent(self) -> Optional[tuple[str, float]]:
        """Get the most frequent measurement result."""
        if not self.counts:
            return None
        total = sum(self.counts.values())
        if total == 0:
            return None
        most = max(self.counts.items(), key=lambda x: x[1])
        return (most[0], most[1] / total)


@dataclass
class BackendInfo:
    """Backend capabilities and information."""
    backend_id: str
    name: str
    is_available: bool
    max_qubits: int
    max_shots: int
    description: str
    supported_gates: list[str]
    topology: Optional[Dict] = None
