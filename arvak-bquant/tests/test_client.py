"""Tests for ArvakClient with mocked HTTP responses."""

import pytest
import httpx

from arvak_bquant.client import ArvakClient
from arvak_bquant.exceptions import (
    ArvakAPIError,
    ArvakConnectionError,
    ArvakJobError,
    ArvakTimeoutError,
)


@pytest.fixture
def client(httpx_mock):
    """Create a client pointing at a fake URL."""
    return ArvakClient(api_key="test-key", base_url="https://fake.arvak.io")


class TestHealth:
    def test_health(self, client, httpx_mock):
        httpx_mock.add_response(
            url="https://fake.arvak.io/v1/health",
            json={"status": "healthy", "version": "1.8.0"},
        )
        resp = client.health()
        assert resp["status"] == "healthy"


class TestListBackends:
    def test_list_backends(self, client, httpx_mock):
        httpx_mock.add_response(
            url="https://fake.arvak.io/v1/backends",
            json={
                "backends": [
                    {
                        "backend_id": "simulator",
                        "name": "Simulator",
                        "is_available": True,
                        "max_qubits": 32,
                        "max_shots": 100000,
                        "supported_gates": ["h", "cx", "rz"],
                    }
                ]
            },
        )
        backends = client.list_backends()
        assert len(backends) == 1
        assert backends[0].backend_id == "simulator"
        assert backends[0].is_available


class TestCompile:
    def test_compile(self, client, httpx_mock):
        httpx_mock.add_response(
            url="https://fake.arvak.io/v1/compile",
            json={
                "compiled_qasm3": 'OPENQASM 3.0;\nqubit[2] q;\nh q[0];\ncx q[0], q[1];\n',
                "stats": {"num_qubits": 2, "depth": 2, "gate_count": 2},
            },
        )
        result = client.compile("OPENQASM 3.0;\nqubit[2] q;\nh q[0];\n")
        assert result.num_qubits == 2
        assert result.gate_count == 2
        assert "OPENQASM" in result.compiled_qasm3


class TestSubmitAndResult:
    def test_submit(self, client, httpx_mock):
        httpx_mock.add_response(
            url="https://fake.arvak.io/v1/jobs",
            json={"job_id": "job-123"},
            status_code=201,
        )
        job_id = client.submit("OPENQASM 3.0;\nqubit[2] q;\n", shots=4096)
        assert job_id == "job-123"

    def test_status(self, client, httpx_mock):
        httpx_mock.add_response(
            url="https://fake.arvak.io/v1/jobs/job-123",
            json={
                "job_id": "job-123",
                "status": "running",
                "backend_id": "simulator",
                "shots": 4096,
                "submitted_at": 1700000000,
            },
        )
        st = client.status("job-123")
        assert st.status == "running"
        assert not st.is_terminal

    def test_result(self, client, httpx_mock):
        httpx_mock.add_response(
            url="https://fake.arvak.io/v1/jobs/job-123/result",
            json={
                "job_id": "job-123",
                "counts": {"00": 500, "11": 524},
                "shots": 1024,
                "execution_time_ms": 42,
            },
        )
        result = client.result("job-123")
        assert result.shots == 1024
        assert result.counts["11"] == 524

    def test_cancel(self, client, httpx_mock):
        httpx_mock.add_response(
            url="https://fake.arvak.io/v1/jobs/job-123",
            json={"success": True, "message": "cancelled"},
        )
        assert client.cancel("job-123") is True


class TestWait:
    def test_wait_completed(self, client, httpx_mock):
        httpx_mock.add_response(
            url="https://fake.arvak.io/v1/jobs/job-1",
            json={
                "job_id": "job-1",
                "status": "completed",
                "backend_id": "sim",
                "shots": 100,
                "submitted_at": 0,
            },
        )
        httpx_mock.add_response(
            url="https://fake.arvak.io/v1/jobs/job-1/result",
            json={
                "job_id": "job-1",
                "counts": {"0": 100},
                "shots": 100,
            },
        )
        result = client.wait("job-1", poll_interval=0.01)
        assert result.shots == 100

    def test_wait_failed(self, client, httpx_mock):
        httpx_mock.add_response(
            url="https://fake.arvak.io/v1/jobs/job-1",
            json={
                "job_id": "job-1",
                "status": "failed",
                "backend_id": "sim",
                "shots": 100,
                "submitted_at": 0,
                "error_message": "backend crashed",
            },
        )
        with pytest.raises(ArvakJobError, match="backend crashed"):
            client.wait("job-1", poll_interval=0.01)


class TestErrors:
    def test_api_error(self, client, httpx_mock):
        httpx_mock.add_response(
            url="https://fake.arvak.io/v1/health",
            json={"error": "unauthorized"},
            status_code=401,
        )
        with pytest.raises(ArvakAPIError) as exc_info:
            client.health()
        assert exc_info.value.status_code == 401

    def test_auth_header_sent(self, client, httpx_mock):
        httpx_mock.add_response(
            url="https://fake.arvak.io/v1/health",
            json={"status": "ok"},
        )
        client.health()
        request = httpx_mock.get_request()
        assert request.headers["authorization"] == "Bearer test-key"
