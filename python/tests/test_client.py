"""Tests for the Arvak gRPC Python client."""

import pytest
import grpc
from arvak_grpc import ArvakClient, JobState
from arvak_grpc.exceptions import (
    ArvakJobNotFoundError,
    ArvakBackendNotFoundError,
    ArvakInvalidCircuitError,
)

# Test circuit: Bell state
BELL_STATE_QASM = """
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
"""

INVALID_QASM = "this is not valid qasm"


@pytest.fixture
def client():
    """Create a client connected to the test server."""
    # Note: Tests assume server is running on localhost:50051
    # In a real setup, you would start a test server programmatically
    client = ArvakClient("localhost:50051", timeout=30.0)
    yield client
    client.close()


def test_list_backends(client):
    """Test listing available backends."""
    backends = client.list_backends()

    assert len(backends) > 0, "Should have at least one backend"

    # Check simulator backend exists
    simulator = next((b for b in backends if b.backend_id == "simulator"), None)
    assert simulator is not None, "Simulator backend should be available"
    assert simulator.is_available
    assert simulator.max_qubits > 0
    assert simulator.max_shots > 0
    assert len(simulator.supported_gates) > 0


def test_get_backend_info(client):
    """Test getting backend information."""
    backend = client.get_backend_info("simulator")

    assert backend.backend_id == "simulator"
    assert backend.is_available
    assert backend.max_qubits > 0
    assert backend.max_shots > 0
    assert len(backend.supported_gates) > 0


def test_get_backend_info_not_found(client):
    """Test getting info for nonexistent backend."""
    with pytest.raises(ArvakBackendNotFoundError):
        client.get_backend_info("nonexistent")


def test_submit_and_wait(client):
    """Test submitting a job and waiting for results."""
    # Submit job
    job_id = client.submit_qasm(BELL_STATE_QASM, "simulator", shots=1000)
    assert job_id is not None
    assert len(job_id) > 0

    # Wait for completion
    result = client.wait_for_job(job_id, max_wait=30.0)

    assert result.job_id == job_id
    assert result.shots == 1000
    assert len(result.counts) > 0

    # Bell state should produce 00 and 11
    total = sum(result.counts.values())
    assert total == 1000

    # Check probabilities
    probs = result.probabilities()
    assert len(probs) > 0
    assert all(0.0 <= p <= 1.0 for p in probs.values())
    assert abs(sum(probs.values()) - 1.0) < 0.001


def test_get_job_status(client):
    """Test getting job status."""
    job_id = client.submit_qasm(BELL_STATE_QASM, "simulator", shots=500)

    job = client.get_job_status(job_id)

    assert job.job_id == job_id
    assert job.backend_id == "simulator"
    assert job.shots == 500
    assert job.state in [JobState.QUEUED, JobState.RUNNING, JobState.COMPLETED]


def test_submit_batch(client):
    """Test batch submission."""
    circuits = [
        (BELL_STATE_QASM, 500),
        (BELL_STATE_QASM, 1000),
        (BELL_STATE_QASM, 1500),
    ]

    job_ids = client.submit_batch(circuits, "simulator", format="qasm3")

    assert len(job_ids) == 3
    assert all(len(job_id) > 0 for job_id in job_ids)

    # Wait for all jobs
    for job_id in job_ids:
        result = client.wait_for_job(job_id, max_wait=30.0)
        assert result.shots in [500, 1000, 1500]


def test_submit_invalid_circuit(client):
    """Test submitting an invalid circuit."""
    with pytest.raises(ArvakInvalidCircuitError):
        client.submit_qasm(INVALID_QASM, "simulator", shots=1000)


def test_submit_invalid_backend(client):
    """Test submitting to a nonexistent backend."""
    with pytest.raises(ArvakBackendNotFoundError):
        client.submit_qasm(BELL_STATE_QASM, "nonexistent", shots=1000)


def test_get_job_status_not_found(client):
    """Test getting status for nonexistent job."""
    with pytest.raises(ArvakJobNotFoundError):
        client.get_job_status("nonexistent-job-id")


def test_get_job_result_not_found(client):
    """Test getting result for nonexistent job."""
    with pytest.raises(ArvakJobNotFoundError):
        client.get_job_result("nonexistent-job-id")


def test_most_frequent_result(client):
    """Test getting the most frequent measurement."""
    job_id = client.submit_qasm(BELL_STATE_QASM, "simulator", shots=1000)
    result = client.wait_for_job(job_id, max_wait=30.0)

    most_freq = result.most_frequent()
    assert most_freq is not None

    bitstring, prob = most_freq
    assert bitstring in result.counts
    assert 0.0 <= prob <= 1.0


def test_cancel_job(client):
    """Test canceling a job."""
    job_id = client.submit_qasm(BELL_STATE_QASM, "simulator", shots=1000)

    # Try to cancel (may already be completed)
    success, message = client.cancel_job(job_id)

    # Either successfully canceled or already in terminal state
    assert isinstance(success, bool)
    assert isinstance(message, str)


def test_context_manager(client):
    """Test using client as context manager."""
    with ArvakClient("localhost:50051") as client:
        backends = client.list_backends()
        assert len(backends) > 0


def test_job_terminal_states(client):
    """Test checking terminal states."""
    job_id = client.submit_qasm(BELL_STATE_QASM, "simulator", shots=100)
    result = client.wait_for_job(job_id, max_wait=30.0)

    job = client.get_job_status(job_id)
    assert job.is_terminal
    assert job.is_success
    assert not job.is_pending


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
