"""Arvak gRPC client implementation."""

import json
import time
from datetime import datetime
from typing import List, Optional

import grpc

from . import arvak_pb2, arvak_pb2_grpc
from .exceptions import (
    ArvakBackendNotFoundError,
    ArvakError,
    ArvakInvalidCircuitError,
    ArvakJobNotCompletedError,
    ArvakJobNotFoundError,
)
from .types import BackendInfo, Job, JobResult, JobState


class ArvakClient:
    """Client for the Arvak gRPC service.

    This client provides methods for submitting quantum circuits for execution,
    checking job status, retrieving results, and managing backends.

    Args:
        address: The gRPC server address (default: "localhost:50051")
        timeout: Default timeout for RPC calls in seconds (default: 30.0)

    Example:
        >>> client = ArvakClient("localhost:50051")
        >>> qasm = '''
        ... OPENQASM 3.0;
        ... qubit[2] q;
        ... h q[0];
        ... cx q[0], q[1];
        ... '''
        >>> job_id = client.submit_qasm(qasm, "simulator", shots=1000)
        >>> result = client.wait_for_job(job_id)
        >>> print(result.counts)
    """

    def __init__(self, address: str = "localhost:50051", timeout: float = 30.0):
        """Initialize the Arvak client."""
        self.address = address
        self.timeout = timeout
        self.channel = grpc.insecure_channel(address)
        self.stub = arvak_pb2_grpc.ArvakServiceStub(self.channel)

    def close(self):
        """Close the gRPC channel."""
        self.channel.close()

    def __enter__(self):
        """Context manager entry."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit."""
        self.close()

    def submit_qasm(
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
        try:
            request = arvak_pb2.SubmitJobRequest(
                circuit=arvak_pb2.CircuitPayload(qasm3=qasm_code),
                backend_id=backend_id,
                shots=shots,
            )
            response = self.stub.SubmitJob(request, timeout=self.timeout)
            return response.job_id
        except grpc.RpcError as e:
            self._handle_grpc_error(e)

    def submit_circuit_json(
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
        try:
            request = arvak_pb2.SubmitJobRequest(
                circuit=arvak_pb2.CircuitPayload(arvak_ir_json=circuit_json),
                backend_id=backend_id,
                shots=shots,
            )
            response = self.stub.SubmitJob(request, timeout=self.timeout)
            return response.job_id
        except grpc.RpcError as e:
            self._handle_grpc_error(e)

    def submit_batch(
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
        try:
            batch_jobs = []
            for circuit_code, shots in circuits:
                if format == "qasm3":
                    payload = arvak_pb2.CircuitPayload(qasm3=circuit_code)
                elif format == "json":
                    payload = arvak_pb2.CircuitPayload(arvak_ir_json=circuit_code)
                else:
                    raise ValueError(f"Invalid format: {format}")

                batch_jobs.append(arvak_pb2.BatchJobRequest(circuit=payload, shots=shots))

            request = arvak_pb2.SubmitBatchRequest(backend_id=backend_id, jobs=batch_jobs)
            response = self.stub.SubmitBatch(request, timeout=self.timeout)
            return list(response.job_ids)
        except grpc.RpcError as e:
            self._handle_grpc_error(e)

    def get_job_status(self, job_id: str) -> Job:
        """Get the status of a job.

        Args:
            job_id: Job ID

        Returns:
            Job object with status and metadata

        Raises:
            ArvakJobNotFoundError: If the job does not exist
            ArvakError: For other errors
        """
        try:
            request = arvak_pb2.GetJobStatusRequest(job_id=job_id)
            response = self.stub.GetJobStatus(request, timeout=self.timeout)
            return self._proto_to_job(response.job)
        except grpc.RpcError as e:
            self._handle_grpc_error(e)

    def get_job_result(self, job_id: str) -> JobResult:
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
        try:
            request = arvak_pb2.GetJobResultRequest(job_id=job_id)
            response = self.stub.GetJobResult(request, timeout=self.timeout)
            return self._proto_to_result(response.result)
        except grpc.RpcError as e:
            self._handle_grpc_error(e)

    def cancel_job(self, job_id: str) -> tuple[bool, str]:
        """Cancel a running or queued job.

        Args:
            job_id: Job ID

        Returns:
            Tuple of (success, message)

        Raises:
            ArvakJobNotFoundError: If the job does not exist
            ArvakError: For other errors
        """
        try:
            request = arvak_pb2.CancelJobRequest(job_id=job_id)
            response = self.stub.CancelJob(request, timeout=self.timeout)
            return (response.success, response.message)
        except grpc.RpcError as e:
            self._handle_grpc_error(e)

    def wait_for_job(
        self,
        job_id: str,
        poll_interval: float = 1.0,
        max_wait: Optional[float] = None,
    ) -> JobResult:
        """Wait for a job to complete and return its result.

        This method polls the job status until it reaches a terminal state,
        then retrieves and returns the result.

        Args:
            job_id: Job ID
            poll_interval: Time to wait between polls in seconds (default: 1.0)
            max_wait: Maximum time to wait in seconds, None for no limit (default: None)

        Returns:
            JobResult object

        Raises:
            TimeoutError: If max_wait is exceeded
            ArvakJobNotFoundError: If the job does not exist
            ArvakError: If the job fails
        """
        start_time = time.time()

        while True:
            job = self.get_job_status(job_id)

            if job.state == JobState.COMPLETED:
                return self.get_job_result(job_id)
            elif job.state == JobState.FAILED:
                raise ArvakError(f"Job failed: {job.error_message}")
            elif job.state == JobState.CANCELED:
                raise ArvakError("Job was canceled")

            if max_wait is not None and (time.time() - start_time) >= max_wait:
                raise TimeoutError(f"Job did not complete within {max_wait} seconds")

            time.sleep(poll_interval)

    def list_backends(self) -> List[BackendInfo]:
        """List all available backends.

        Returns:
            List of BackendInfo objects

        Raises:
            ArvakError: For errors
        """
        try:
            request = arvak_pb2.ListBackendsRequest()
            response = self.stub.ListBackends(request, timeout=self.timeout)
            return [self._proto_to_backend_info(b) for b in response.backends]
        except grpc.RpcError as e:
            self._handle_grpc_error(e)

    def get_backend_info(self, backend_id: str) -> BackendInfo:
        """Get detailed information about a specific backend.

        Args:
            backend_id: Backend ID

        Returns:
            BackendInfo object

        Raises:
            ArvakBackendNotFoundError: If the backend does not exist
            ArvakError: For other errors
        """
        try:
            request = arvak_pb2.GetBackendInfoRequest(backend_id=backend_id)
            response = self.stub.GetBackendInfo(request, timeout=self.timeout)
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
            execution_time_ms=proto_result.execution_time_ms if proto_result.execution_time_ms > 0 else None,
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
