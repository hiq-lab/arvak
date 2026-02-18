"""Error hierarchy for arvak-bquant."""

from __future__ import annotations


class ArvakError(Exception):
    """Base exception for all Arvak errors."""


class ArvakAPIError(ArvakError):
    """HTTP-level error from the Arvak REST API."""

    def __init__(self, status_code: int, detail: str) -> None:
        self.status_code = status_code
        self.detail = detail
        super().__init__(f"Arvak API error {status_code}: {detail}")


class ArvakConnectionError(ArvakError):
    """Could not reach the Arvak API."""


class ArvakTimeoutError(ArvakError):
    """Timed out waiting for a job to complete."""


class ArvakCompilationError(ArvakError):
    """Circuit compilation failed."""


class ArvakJobError(ArvakError):
    """Job execution failed on the backend."""

    def __init__(self, job_id: str, detail: str) -> None:
        self.job_id = job_id
        self.detail = detail
        super().__init__(f"Job {job_id} failed: {detail}")
