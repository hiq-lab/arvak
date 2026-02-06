"""Tests for the async Arvak gRPC client."""

import pytest
import asyncio
from arvak_grpc import AsyncArvakClient, JobState
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
async def async_client():
    """Create an async client connected to the test server."""
    # Note: Tests assume server is running on localhost:50051
    client = AsyncArvakClient("localhost:50051", timeout=30.0)
    yield client
    await client.close()


@pytest.mark.asyncio
async def test_list_backends(async_client):
    """Test listing available backends async."""
    backends = await async_client.list_backends()

    assert len(backends) > 0, "Should have at least one backend"

    # Check simulator backend exists
    simulator = next((b for b in backends if b.backend_id == "simulator"), None)
    assert simulator is not None, "Simulator backend should be available"
    assert simulator.is_available
    assert simulator.max_qubits > 0


@pytest.mark.asyncio
async def test_get_backend_info(async_client):
    """Test getting backend information async."""
    backend = await async_client.get_backend_info("simulator")

    assert backend.backend_id == "simulator"
    assert backend.is_available
    assert len(backend.supported_gates) > 0


@pytest.mark.asyncio
async def test_submit_and_wait(async_client):
    """Test async job submission and waiting."""
    # Submit job
    job_id = await async_client.submit_qasm(BELL_STATE_QASM, "simulator", shots=1000)
    assert job_id is not None
    assert len(job_id) > 0

    # Wait for completion
    result = await async_client.wait_for_job(job_id, max_wait=30.0)

    assert result.job_id == job_id
    assert result.shots == 1000
    assert len(result.counts) > 0

    # Verify probabilities
    probs = result.probabilities()
    assert abs(sum(probs.values()) - 1.0) < 0.001


@pytest.mark.asyncio
async def test_concurrent_submissions(async_client):
    """Test concurrent job submissions."""
    # Submit 5 jobs concurrently
    tasks = [
        async_client.submit_qasm(BELL_STATE_QASM, "simulator", shots=500)
        for _ in range(5)
    ]

    job_ids = await asyncio.gather(*tasks)

    assert len(job_ids) == 5
    assert all(len(job_id) > 0 for job_id in job_ids)

    # Wait for all jobs concurrently
    wait_tasks = [async_client.wait_for_job(job_id, max_wait=30.0) for job_id in job_ids]
    results = await asyncio.gather(*wait_tasks)

    assert len(results) == 5
    assert all(result.shots == 500 for result in results)


@pytest.mark.asyncio
async def test_context_manager():
    """Test async context manager."""
    async with AsyncArvakClient("localhost:50051") as client:
        backends = await client.list_backends()
        assert len(backends) > 0


@pytest.mark.asyncio
async def test_submit_batch(async_client):
    """Test batch submission async."""
    circuits = [
        (BELL_STATE_QASM, 500),
        (BELL_STATE_QASM, 1000),
        (BELL_STATE_QASM, 1500),
    ]

    job_ids = await async_client.submit_batch(circuits, "simulator", format="qasm3")

    assert len(job_ids) == 3

    # Wait for all
    results = await asyncio.gather(
        *[async_client.wait_for_job(job_id, max_wait=30.0) for job_id in job_ids]
    )

    assert len(results) == 3
    assert results[0].shots == 500
    assert results[1].shots == 1000
    assert results[2].shots == 1500


@pytest.mark.asyncio
async def test_progress_callback(async_client):
    """Test progress callback during wait."""
    job_id = await async_client.submit_qasm(BELL_STATE_QASM, "simulator", shots=100)

    states_seen = []

    def progress(job):
        states_seen.append(job.state)

    result = await async_client.wait_for_job(
        job_id, poll_interval=0.1, max_wait=30.0, progress_callback=progress
    )

    assert result is not None
    assert len(states_seen) > 0
    assert JobState.COMPLETED in states_seen or states_seen[-1] == JobState.COMPLETED


@pytest.mark.asyncio
async def test_get_job_status(async_client):
    """Test getting job status async."""
    job_id = await async_client.submit_qasm(BELL_STATE_QASM, "simulator", shots=500)

    job = await async_client.get_job_status(job_id)

    assert job.job_id == job_id
    assert job.backend_id == "simulator"
    assert job.shots == 500


@pytest.mark.asyncio
async def test_cancel_job(async_client):
    """Test canceling a job async."""
    job_id = await async_client.submit_qasm(BELL_STATE_QASM, "simulator", shots=1000)

    # Try to cancel
    success, message = await async_client.cancel_job(job_id)

    assert isinstance(success, bool)
    assert isinstance(message, str)


@pytest.mark.asyncio
async def test_invalid_circuit(async_client):
    """Test submitting invalid circuit async."""
    with pytest.raises(ArvakInvalidCircuitError):
        await async_client.submit_qasm(INVALID_QASM, "simulator", shots=1000)


@pytest.mark.asyncio
async def test_invalid_backend(async_client):
    """Test submitting to nonexistent backend async."""
    with pytest.raises(ArvakBackendNotFoundError):
        await async_client.submit_qasm(BELL_STATE_QASM, "nonexistent", shots=1000)


@pytest.mark.asyncio
async def test_job_not_found(async_client):
    """Test getting status for nonexistent job async."""
    with pytest.raises(ArvakJobNotFoundError):
        await async_client.get_job_status("nonexistent-job-id")


@pytest.mark.asyncio
async def test_connection_pooling():
    """Test connection pooling with multiple clients."""
    clients = [AsyncArvakClient("localhost:50051", pool_size=3) for _ in range(5)]

    try:
        # Use all clients concurrently
        tasks = [
            client.submit_qasm(BELL_STATE_QASM, "simulator", shots=100)
            for client in clients
        ]

        job_ids = await asyncio.gather(*tasks)
        assert len(job_ids) == 5

    finally:
        # Close all clients
        await asyncio.gather(*[client.close() for client in clients])


@pytest.mark.asyncio
async def test_high_concurrency():
    """Test high concurrency with connection pooling."""
    async with AsyncArvakClient("localhost:50051", pool_size=10) as client:
        # Submit 50 jobs concurrently
        tasks = [
            client.submit_qasm(BELL_STATE_QASM, "simulator", shots=100)
            for _ in range(50)
        ]

        job_ids = await asyncio.gather(*tasks)
        assert len(job_ids) == 50

        # We won't wait for all to complete to keep test fast
        # Just verify they were all accepted
        assert all(len(job_id) > 0 for job_id in job_ids)


if __name__ == "__main__":
    pytest.main([__file__, "-v", "-s"])
