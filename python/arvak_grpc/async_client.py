"""Async Arvak gRPC client implementation with connection pooling."""

import asyncio
import json
from datetime import datetime
from typing import List, Optional, Callable, Any

import grpc.aio

from . import arvak_pb2, arvak_pb2_grpc
from .exceptions import (
    ArvakBackendNotFoundError,
    ArvakError,
    ArvakInvalidCircuitError,
    ArvakJobNotCompletedError,
    ArvakJobNotFoundError,
)
from .types import BackendInfo, Job, JobResult, JobState


class ConnectionPool:
    """Connection pool for gRPC channels.

    Manages a pool of reusable gRPC channels to improve performance
    by avoiding the overhead of creating new connections.
    """

    def __init__(self, address: str, max_size: int = 10):
        """Initialize connection pool.

        Args:
            address: The gRPC server address
            max_size: Maximum number of channels in the pool
        """
        self.address = address
        self.max_size = max_size
        self._pool: List[grpc.aio.Channel] = []
        self._lock = asyncio.Lock()
        self._closed = False

    async def get_channel(self) -> grpc.aio.Channel:
        """Get a channel from the pool or create a new one."""
        async with self._lock:
            if self._closed:
                raise RuntimeError("Connection pool is closed")

            # Try to get an existing channel
            if self._pool:
                return self._pool.pop()

            # Create new channel if pool not full
            channel = grpc.aio.insecure_channel(self.address)
            return channel

    async def return_channel(self, channel: grpc.aio.Channel):
        """Return a channel to the pool."""
        async with self._lock:
            if self._closed:
                await channel.close()
                return

            if len(self._pool) < self.max_size:
                self._pool.append(channel)
            else:
                await channel.close()

    async def close(self):
        """Close all channels in the pool."""
        async with self._lock:
            self._closed = True
            for channel in self._pool:
                await channel.close()
            self._pool.clear()


class AsyncArvakClient:
    """Async client for the Arvak gRPC service.

    This client provides async/await methods for submitting quantum circuits,
    checking job status, and retrieving results. It uses connection pooling
    for improved performance.

    Args:
        address: The gRPC server address (default: "localhost:50051")
        timeout: Default timeout for RPC calls in seconds (default: 30.0)
        pool_size: Maximum number of connections in the pool (default: 10)

    Example:
        >>> async with AsyncArvakClient("localhost:50051") as client:
        ...     job_id = await client.submit_qasm(qasm, "simulator", shots=1000)
        ...     result = await client.wait_for_job(job_id)
        ...     print(result.counts)
    """

    def __init__(
        self,
        address: str = "localhost:50051",
        timeout: float = 30.0,
        pool_size: int = 10,
    ):
        """Initialize the async Arvak client."""
        self.address = address
        self.timeout = timeout
        self._pool = ConnectionPool(address, pool_size)
        self._channel: Optional[grpc.aio.Channel] = None
        self._stub: Optional[arvak_pb2_grpc.ArvakServiceStub] = None

    async def __aenter__(self):
        """Async context manager entry."""
        await self._ensure_connected()
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """Async context manager exit."""
        await self.close()

    async def _ensure_connected(self):
        """Ensure we have an active connection."""
        if self._channel is None:
            self._channel = await self._pool.get_channel()
            self._stub = arvak_pb2_grpc.ArvakServiceStub(self._channel)

    async def close(self):
        """Close the client and return channel to pool."""
        if self._channel is not None:
            await self._pool.return_channel(self._channel)
            self._channel = None
            self._stub = None

    async def submit_qasm(
        self, qasm_code: str, backend_id: str, shots: int = 1024
    ) -> str:
        """Submit an OpenQASM 3 circuit for execution.

        Args:
            qasm_code: OpenQASM 3 source code
            backend_id: ID of the backend to execute on
            shots: Number of shots to execute (default: 1024)

        Returns:
            Job ID string

        Raises:
            ArvakInvalidCircuitError: If the circuit is invalid
            ArvakBackendNotFoundError: If the backend does not exist
            ArvakError: For other errors
        """
        await self._ensure_connected()

        try:
            request = arvak_pb2.SubmitJobRequest(
                circuit=arvak_pb2.CircuitPayload(qasm3=qasm_code),
                backend_id=backend_id,
                shots=shots,
            )
            response = await self._stub.SubmitJob(request, timeout=self.timeout)
            return response.job_id
        except grpc.RpcError as e:
            self._handle_grpc_error(e)

    async def submit_circuit_json(
        self, circuit_json: str, backend_id: str, shots: int = 1024
    ) -> str:
        """Submit an Arvak IR JSON circuit for execution.

        Args:
            circuit_json: Arvak IR JSON representation
            backend_id: ID of the backend to execute on
            shots: Number of shots to execute (default: 1024)

        Returns:
            Job ID string

        Raises:
            ArvakInvalidCircuitError: If the circuit is invalid
            ArvakBackendNotFoundError: If the backend does not exist
            ArvakError: For other errors
        """
        await self._ensure_connected()

        try:
            request = arvak_pb2.SubmitJobRequest(
                circuit=arvak_pb2.CircuitPayload(arvak_ir_json=circuit_json),
                backend_id=backend_id,
                shots=shots,
            )
            response = await self._stub.SubmitJob(request, timeout=self.timeout)
            return response.job_id
        except grpc.RpcError as e:
            self._handle_grpc_error(e)

    async def submit_batch(
        self,
        circuits: List[tuple[str, int]],
        backend_id: str,
        format: str = "qasm3",
    ) -> List[str]:
        """Submit multiple circuits as a batch.

        Args:
            circuits: List of (circuit_code, shots) tuples
            backend_id: ID of the backend to execute on
            format: Circuit format ("qasm3" or "json")

        Returns:
            List of job ID strings

        Raises:
            ArvakInvalidCircuitError: If any circuit is invalid
            ArvakBackendNotFoundError: If the backend does not exist
            ArvakError: For other errors
        """
        await self._ensure_connected()

        try:
            batch_jobs = []
            for circuit_code, shots in circuits:
                if format == "qasm3":
                    payload = arvak_pb2.CircuitPayload(qasm3=circuit_code)
                elif format == "json":
                    payload = arvak_pb2.CircuitPayload(arvak_ir_json=circuit_code)
                else:
                    raise ValueError(f"Invalid format: {format}")

                batch_jobs.append(
                    arvak_pb2.BatchJobRequest(circuit=payload, shots=shots)
                )

            request = arvak_pb2.SubmitBatchRequest(
                backend_id=backend_id, jobs=batch_jobs
            )
            response = await self._stub.SubmitBatch(request, timeout=self.timeout)
            return list(response.job_ids)
        except grpc.RpcError as e:
            self._handle_grpc_error(e)

    async def get_job_status(self, job_id: str) -> Job:
        """Get the status of a job.

        Args:
            job_id: Job ID

        Returns:
            Job object with status and metadata

        Raises:
            ArvakJobNotFoundError: If the job does not exist
            ArvakError: For other errors
        """
        await self._ensure_connected()

        try:
            request = arvak_pb2.GetJobStatusRequest(job_id=job_id)
            response = await self._stub.GetJobStatus(request, timeout=self.timeout)
            return self._proto_to_job(response.job)
        except grpc.RpcError as e:
            self._handle_grpc_error(e)

    async def get_job_result(self, job_id: str) -> JobResult:
        """Get the result of a completed job.

        Args:
            job_id: Job ID

        Returns:
            JobResult object with measurement counts

        Raises:
            ArvakJobNotFoundError: If the job does not exist
            ArvakJobNotCompletedError: If the job is not completed
            ArvakError: For other errors
        """
        await self._ensure_connected()

        try:
            request = arvak_pb2.GetJobResultRequest(job_id=job_id)
            response = await self._stub.GetJobResult(request, timeout=self.timeout)
            return self._proto_to_result(response.result)
        except grpc.RpcError as e:
            self._handle_grpc_error(e)

    async def cancel_job(self, job_id: str) -> tuple[bool, str]:
        """Cancel a running or queued job.

        Args:
            job_id: Job ID

        Returns:
            Tuple of (success, message)

        Raises:
            ArvakJobNotFoundError: If the job does not exist
            ArvakError: For other errors
        """
        await self._ensure_connected()

        try:
            request = arvak_pb2.CancelJobRequest(job_id=job_id)
            response = await self._stub.CancelJob(request, timeout=self.timeout)
            return (response.success, response.message)
        except grpc.RpcError as e:
            self._handle_grpc_error(e)

    async def wait_for_job(
        self,
        job_id: str,
        poll_interval: float = 1.0,
        max_wait: Optional[float] = None,
        progress_callback: Optional[Callable[[Job], None]] = None,
    ) -> JobResult:
        """Wait for a job to complete and return its result.

        This method polls the job status until it reaches a terminal state,
        then retrieves and returns the result.

        Args:
            job_id: Job ID
            poll_interval: Time to wait between polls in seconds (default: 1.0)
            max_wait: Maximum time to wait in seconds, None for no limit (default: None)
            progress_callback: Optional callback called with Job on each poll

        Returns:
            JobResult object

        Raises:
            TimeoutError: If max_wait is exceeded
            ArvakJobNotFoundError: If the job does not exist
            ArvakError: If the job fails
        """
        start_time = asyncio.get_event_loop().time()

        while True:
            job = await self.get_job_status(job_id)

            if progress_callback:
                progress_callback(job)

            if job.state == JobState.COMPLETED:
                return await self.get_job_result(job_id)
            elif job.state == JobState.FAILED:
                raise ArvakError(f"Job failed: {job.error_message}")
            elif job.state == JobState.CANCELED:
                raise ArvakError("Job was canceled")

            if max_wait is not None:
                elapsed = asyncio.get_event_loop().time() - start_time
                if elapsed >= max_wait:
                    raise TimeoutError(
                        f"Job did not complete within {max_wait} seconds"
                    )

            await asyncio.sleep(poll_interval)

    async def list_backends(self) -> List[BackendInfo]:
        """List all available backends.

        Returns:
            List of BackendInfo objects

        Raises:
            ArvakError: For errors
        """
        await self._ensure_connected()

        try:
            request = arvak_pb2.ListBackendsRequest()
            response = await self._stub.ListBackends(request, timeout=self.timeout)
            return [self._proto_to_backend_info(b) for b in response.backends]
        except grpc.RpcError as e:
            self._handle_grpc_error(e)

    async def get_backend_info(self, backend_id: str) -> BackendInfo:
        """Get detailed information about a specific backend.

        Args:
            backend_id: Backend ID

        Returns:
            BackendInfo object

        Raises:
            ArvakBackendNotFoundError: If the backend does not exist
            ArvakError: For other errors
        """
        await self._ensure_connected()

        try:
            request = arvak_pb2.GetBackendInfoRequest(backend_id=backend_id)
            response = await self._stub.GetBackendInfo(request, timeout=self.timeout)
            return self._proto_to_backend_info(response.backend)
        except grpc.RpcError as e:
            self._handle_grpc_error(e)

    def _proto_to_job(self, proto_job) -> Job:
        """Convert protobuf Job to Job dataclass."""
        submitted_at = datetime.fromtimestamp(proto_job.submitted_at)
        started_at = (
            datetime.fromtimestamp(proto_job.started_at)
            if proto_job.started_at > 0
            else None
        )
        completed_at = (
            datetime.fromtimestamp(proto_job.completed_at)
            if proto_job.completed_at > 0
            else None
        )

        return Job(
            job_id=proto_job.job_id,
            state=JobState(proto_job.state),
            submitted_at=submitted_at,
            backend_id=proto_job.backend_id,
            shots=proto_job.shots,
            started_at=started_at,
            completed_at=completed_at,
            error_message=proto_job.error_message if proto_job.error_message else None,
        )

    def _proto_to_result(self, proto_result) -> JobResult:
        """Convert protobuf JobResult to JobResult dataclass."""
        metadata = None
        if proto_result.metadata_json and proto_result.metadata_json != "{}":
            try:
                metadata = json.loads(proto_result.metadata_json)
            except json.JSONDecodeError:
                pass

        return JobResult(
            job_id=proto_result.job_id,
            counts=dict(proto_result.counts),
            shots=proto_result.shots,
            execution_time_ms=(
                proto_result.execution_time_ms
                if proto_result.execution_time_ms > 0
                else None
            ),
            metadata=metadata,
        )

    def _proto_to_backend_info(self, proto_backend) -> BackendInfo:
        """Convert protobuf BackendInfo to BackendInfo dataclass."""
        topology = None
        if proto_backend.topology_json and proto_backend.topology_json != "{}":
            try:
                topology = json.loads(proto_backend.topology_json)
            except json.JSONDecodeError:
                pass

        return BackendInfo(
            backend_id=proto_backend.backend_id,
            name=proto_backend.name,
            is_available=proto_backend.is_available,
            max_qubits=proto_backend.max_qubits,
            max_shots=proto_backend.max_shots,
            description=proto_backend.description,
            supported_gates=list(proto_backend.supported_gates),
            topology=topology,
        )

    def _handle_grpc_error(self, error: grpc.RpcError):
        """Convert gRPC errors to Arvak exceptions."""
        code = error.code()
        details = error.details()

        if code == grpc.StatusCode.NOT_FOUND:
            if "job" in details.lower():
                raise ArvakJobNotFoundError(details)
            elif "backend" in details.lower():
                raise ArvakBackendNotFoundError(details)
            else:
                raise ArvakError(details)
        elif code == grpc.StatusCode.INVALID_ARGUMENT:
            raise ArvakInvalidCircuitError(details)
        elif code == grpc.StatusCode.FAILED_PRECONDITION:
            raise ArvakJobNotCompletedError(details)
        else:
            raise ArvakError(f"{code.name}: {details}")
