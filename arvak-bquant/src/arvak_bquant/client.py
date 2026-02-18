"""REST client for the Arvak quantum compilation and execution API."""

from __future__ import annotations

import logging
import time

import httpx

from .exceptions import (
    ArvakAPIError,
    ArvakConnectionError,
    ArvakJobError,
    ArvakTimeoutError,
)
from .types import BackendInfo, CompileResult, JobResult, JobStatus

logger = logging.getLogger(__name__)

_DEFAULT_BASE_URL = "https://api.arvak.io"
_DEFAULT_TIMEOUT = 30.0


class ArvakClient:
    """Synchronous HTTP client for the Arvak REST gateway.

    Parameters
    ----------
    api_key : str
        Bearer token for authentication.
    base_url : str
        Base URL of the REST gateway (default: https://api.arvak.io).
    timeout : float
        HTTP request timeout in seconds (default: 30).
    """

    def __init__(
        self,
        api_key: str,
        base_url: str = _DEFAULT_BASE_URL,
        timeout: float = _DEFAULT_TIMEOUT,
    ) -> None:
        self._base_url = base_url.rstrip("/")
        self._api_key = api_key
        self._client = httpx.Client(
            timeout=timeout,
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {api_key}",
            },
        )

    # ── Low-level helpers ──────────────────────────────────────────────

    def _request(self, method: str, path: str, **kwargs) -> dict:
        url = f"{self._base_url}{path}"
        try:
            resp = self._client.request(method, url, **kwargs)
        except httpx.ConnectError as exc:
            raise ArvakConnectionError(
                f"Could not connect to {self._base_url}"
            ) from exc

        if resp.status_code >= 400:
            detail = resp.text
            try:
                detail = resp.json().get("error", detail)
            except Exception:
                pass
            raise ArvakAPIError(resp.status_code, detail)

        if resp.status_code == 204 or not resp.content:
            return {}
        return resp.json()

    # ── Public API ─────────────────────────────────────────────────────

    def health(self) -> dict:
        """Check gateway health."""
        return self._request("GET", "/v1/health")

    def list_backends(self) -> list[BackendInfo]:
        """List available quantum backends."""
        data = self._request("GET", "/v1/backends")
        return [
            BackendInfo(
                backend_id=b["backend_id"],
                name=b["name"],
                is_available=b["is_available"],
                max_qubits=b["max_qubits"],
                max_shots=b["max_shots"],
                supported_gates=b.get("supported_gates", []),
            )
            for b in data.get("backends", [])
        ]

    def get_backend(self, backend_id: str) -> BackendInfo:
        """Get details for a specific backend."""
        b = self._request("GET", f"/v1/backends/{backend_id}")
        return BackendInfo(
            backend_id=b["backend_id"],
            name=b["name"],
            is_available=b["is_available"],
            max_qubits=b["max_qubits"],
            max_shots=b["max_shots"],
            supported_gates=b.get("supported_gates", []),
        )

    def compile(
        self,
        qasm3: str,
        backend_id: str = "simulator",
        optimization_level: int = 1,
    ) -> CompileResult:
        """Compile a circuit without executing it."""
        data = self._request(
            "POST",
            "/v1/compile",
            json={
                "qasm3": qasm3,
                "backend_id": backend_id,
                "optimization_level": optimization_level,
            },
        )
        stats = data.get("stats", {})
        return CompileResult(
            compiled_qasm3=data["compiled_qasm3"],
            num_qubits=stats.get("num_qubits", 0),
            depth=stats.get("depth", 0),
            gate_count=stats.get("gate_count", 0),
        )

    def submit(
        self,
        qasm3: str,
        backend_id: str = "simulator",
        shots: int = 1024,
        optimization_level: int = 1,
    ) -> str:
        """Submit a job and return the job ID."""
        data = self._request(
            "POST",
            "/v1/jobs",
            json={
                "qasm3": qasm3,
                "backend_id": backend_id,
                "shots": shots,
                "optimization_level": optimization_level,
            },
        )
        return data["job_id"]

    def status(self, job_id: str) -> JobStatus:
        """Get current job status."""
        data = self._request("GET", f"/v1/jobs/{job_id}")
        return JobStatus(
            job_id=data["job_id"],
            status=data["status"],
            backend_id=data["backend_id"],
            shots=data["shots"],
            submitted_at=data["submitted_at"],
            started_at=data.get("started_at"),
            completed_at=data.get("completed_at"),
            error_message=data.get("error_message"),
        )

    def result(self, job_id: str) -> JobResult:
        """Get job result (must be completed)."""
        data = self._request("GET", f"/v1/jobs/{job_id}/result")
        return JobResult(
            job_id=data["job_id"],
            counts=data["counts"],
            shots=data["shots"],
            execution_time_ms=data.get("execution_time_ms"),
        )

    def cancel(self, job_id: str) -> bool:
        """Cancel a running job. Returns True if cancellation succeeded."""
        data = self._request("DELETE", f"/v1/jobs/{job_id}")
        return data.get("success", False)

    def wait(
        self,
        job_id: str,
        poll_interval: float = 1.0,
        timeout: float = 300.0,
    ) -> JobResult:
        """Poll until job completes or times out."""
        deadline = time.monotonic() + timeout
        while True:
            st = self.status(job_id)
            if st.status == "completed":
                return self.result(job_id)
            if st.status == "failed":
                raise ArvakJobError(job_id, st.error_message or "unknown")
            if st.status == "cancelled":
                raise ArvakJobError(job_id, "Job was cancelled")
            if time.monotonic() > deadline:
                raise ArvakTimeoutError(
                    f"Job {job_id} did not complete within {timeout}s"
                )
            time.sleep(poll_interval)

    def run(
        self,
        qasm3: str,
        backend_id: str = "simulator",
        shots: int = 1024,
        optimization_level: int = 1,
        poll_interval: float = 1.0,
        timeout: float = 300.0,
    ) -> JobResult:
        """Submit a job and wait for the result (convenience method)."""
        job_id = self.submit(
            qasm3,
            backend_id=backend_id,
            shots=shots,
            optimization_level=optimization_level,
        )
        return self.wait(job_id, poll_interval=poll_interval, timeout=timeout)
