"""Tests for streaming gRPC methods.

NOTE: These tests require regenerated protobuf code with streaming types.
To regenerate the Python protobuf code from proto/arvak.proto:
    cd crates/arvak-grpc
    python -m grpc_tools.protoc -I proto --python_out=../../python/arvak_grpc --grpc_python_out=../../python/arvak_grpc proto/arvak.proto
"""

import pytest
from unittest.mock import Mock, AsyncMock, MagicMock
from datetime import datetime

from arvak_grpc import AsyncArvakClient, JobState
from arvak_grpc import arvak_pb2

# Enable asyncio mode for pytest
pytestmark = pytest.mark.asyncio


@pytest.fixture
def mock_channel():
    """Mock gRPC channel."""
    channel = AsyncMock()
    return channel


@pytest.fixture
def mock_stub():
    """Mock gRPC stub."""
    stub = Mock()
    return stub


@pytest.fixture
async def client(mock_channel, mock_stub, monkeypatch):
    """Create async client with mocked channel."""
    client = AsyncArvakClient("localhost:50051")

    # Mock the channel and stub
    async def mock_get_channel():
        return mock_channel

    monkeypatch.setattr(client._pool, "get_channel", mock_get_channel)
    client._channel = mock_channel
    client._stub = mock_stub

    yield client
    await client.close()


async def test_watch_job_stream(client, mock_stub):
    """Test WatchJob streaming RPC."""
    # Mock streaming response
    updates = [
        arvak_pb2.JobStatusUpdate(
            job_id="test-job-1",
            state=arvak_pb2.JOB_STATE_QUEUED,
            timestamp=1234567890,
            error_message="",
        ),
        arvak_pb2.JobStatusUpdate(
            job_id="test-job-1",
            state=arvak_pb2.JOB_STATE_RUNNING,
            timestamp=1234567891,
            error_message="",
        ),
        arvak_pb2.JobStatusUpdate(
            job_id="test-job-1",
            state=arvak_pb2.JOB_STATE_COMPLETED,
            timestamp=1234567892,
            error_message="",
        ),
    ]

    # Create async generator for mock response
    async def mock_watch_job(*args, **kwargs):
        for update in updates:
            yield update

    mock_stub.WatchJob = mock_watch_job

    # Consume the stream
    received_states = []
    async for state, timestamp, error_msg in client.watch_job("test-job-1"):
        received_states.append(state)
        assert isinstance(timestamp, datetime)
        assert error_msg is None

    # Verify all states received
    assert len(received_states) == 3
    assert received_states[0] == JobState.QUEUED
    assert received_states[1] == JobState.RUNNING
    assert received_states[2] == JobState.COMPLETED


async def test_watch_job_with_error(client, mock_stub):
    """Test WatchJob streaming with job failure."""
    updates = [
        arvak_pb2.JobStatusUpdate(
            job_id="test-job-2",
            state=arvak_pb2.JOB_STATE_RUNNING,
            timestamp=1234567890,
            error_message="",
        ),
        arvak_pb2.JobStatusUpdate(
            job_id="test-job-2",
            state=arvak_pb2.JOB_STATE_FAILED,
            timestamp=1234567891,
            error_message="Backend error: timeout",
        ),
    ]

    async def mock_watch_job(*args, **kwargs):
        for update in updates:
            yield update

    mock_stub.WatchJob = mock_watch_job

    # Consume the stream
    received_updates = []
    async for state, timestamp, error_msg in client.watch_job("test-job-2"):
        received_updates.append((state, error_msg))

    assert len(received_updates) == 2
    assert received_updates[0] == (JobState.RUNNING, None)
    assert received_updates[1] == (JobState.FAILED, "Backend error: timeout")


async def test_stream_results(client, mock_stub):
    """Test StreamResults server streaming RPC."""
    # Mock chunked results
    chunks = [
        arvak_pb2.ResultChunk(
            job_id="test-job-3",
            counts={"00": 500, "11": 500},
            is_final=False,
            chunk_index=0,
            total_chunks=3,
        ),
        arvak_pb2.ResultChunk(
            job_id="test-job-3",
            counts={"01": 300, "10": 200},
            is_final=False,
            chunk_index=1,
            total_chunks=3,
        ),
        arvak_pb2.ResultChunk(
            job_id="test-job-3",
            counts={"00": 100},
            is_final=True,
            chunk_index=2,
            total_chunks=3,
        ),
    ]

    async def mock_stream_results(*args, **kwargs):
        for chunk in chunks:
            yield chunk

    mock_stub.StreamResults = mock_stream_results

    # Consume the stream
    all_counts = {}
    chunk_count = 0
    async for counts, is_final, idx, total in client.stream_results(
        "test-job-3", chunk_size=1000
    ):
        all_counts.update(counts)
        chunk_count += 1
        assert idx == chunk_count - 1
        assert total == 3

    # Verify all chunks received
    assert chunk_count == 3
    assert all_counts == {"00": 600, "11": 500, "01": 300, "10": 200}


async def test_submit_batch_stream(client, mock_stub):
    """Test SubmitBatchStream bidirectional streaming RPC."""
    # Mock streaming responses
    results = [
        arvak_pb2.BatchJobResult(
            job_id="job-1",
            client_request_id="req-0",
            submitted="Job submitted successfully",
        ),
        arvak_pb2.BatchJobResult(
            job_id="job-2",
            client_request_id="req-1",
            submitted="Job submitted successfully",
        ),
        arvak_pb2.BatchJobResult(
            job_id="job-1",
            client_request_id="req-0",
            completed=arvak_pb2.JobResult(
                job_id="job-1",
                counts={"00": 50, "11": 50},
                shots=100,
                execution_time_ms=150,
                metadata_json="{}",
            ),
        ),
        arvak_pb2.BatchJobResult(
            job_id="job-2",
            client_request_id="req-1",
            error="Circuit parsing failed",
        ),
    ]

    async def mock_submit_batch_stream(request_gen, *args, **kwargs):
        # Consume request generator
        async for req in request_gen:
            assert hasattr(req, "circuit")
            assert hasattr(req, "backend_id")
            assert hasattr(req, "shots")

        # Yield results
        for result in results:
            yield result

    mock_stub.SubmitBatchStream = mock_submit_batch_stream

    # Create circuit generator
    async def circuit_gen():
        circuits = [
            ("qasm1", "simulator", 100, "qasm3", "req-0"),
            ("qasm2", "simulator", 100, "qasm3", "req-1"),
        ]
        for circuit, backend, shots, fmt, req_id in circuits:
            yield (circuit, backend, shots, fmt, req_id)

    # Consume the stream
    submissions = []
    completions = []
    errors = []

    async for job_id, req_id, rtype, rdata in client.submit_batch_stream(circuit_gen()):
        if rtype == "submitted":
            submissions.append((job_id, req_id))
        elif rtype == "completed":
            completions.append((job_id, req_id, rdata))
        elif rtype == "error":
            errors.append((job_id, req_id, rdata))

    # Verify results
    assert len(submissions) == 2
    assert len(completions) == 1
    assert len(errors) == 1

    assert submissions[0] == ("job-1", "req-0")
    assert submissions[1] == ("job-2", "req-1")

    assert completions[0][0] == "job-1"
    assert completions[0][1] == "req-0"
    assert completions[0][2].counts == {"00": 50, "11": 50}

    assert errors[0] == ("job-2", "req-1", "Circuit parsing failed")


async def test_stream_empty_results(client, mock_stub):
    """Test StreamResults with empty results."""
    chunks = [
        arvak_pb2.ResultChunk(
            job_id="test-job-4",
            counts={},
            is_final=True,
            chunk_index=0,
            total_chunks=1,
        ),
    ]

    async def mock_stream_results(*args, **kwargs):
        for chunk in chunks:
            yield chunk

    mock_stub.StreamResults = mock_stream_results

    # Consume the stream
    all_counts = {}
    async for counts, is_final, idx, total in client.stream_results("test-job-4"):
        all_counts.update(counts)
        if is_final:
            break

    assert all_counts == {}


async def test_watch_job_immediate_completion(client, mock_stub):
    """Test WatchJob when job is already completed."""
    updates = [
        arvak_pb2.JobStatusUpdate(
            job_id="test-job-5",
            state=arvak_pb2.JOB_STATE_COMPLETED,
            timestamp=1234567890,
            error_message="",
        ),
    ]

    async def mock_watch_job(*args, **kwargs):
        for update in updates:
            yield update

    mock_stub.WatchJob = mock_watch_job

    # Consume the stream
    states = []
    async for state, _, _ in client.watch_job("test-job-5"):
        states.append(state)

    assert len(states) == 1
    assert states[0] == JobState.COMPLETED
