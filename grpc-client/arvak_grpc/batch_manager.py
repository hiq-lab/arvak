"""Enhanced batch job management with concurrent execution and progress tracking."""

import time
from concurrent.futures import ThreadPoolExecutor, as_completed as cf_as_completed
from dataclasses import dataclass
from typing import List, Optional, Callable, Dict, Any
from enum import Enum

from .job_future import JobFuture
from .types import JobResult


class BatchStatus(Enum):
    """Status of a batch operation."""
    PENDING = "pending"
    RUNNING = "running"
    COMPLETED = "completed"
    FAILED = "failed"
    PARTIAL = "partial"


@dataclass
class BatchProgress:
    """Progress information for a batch operation.

    Attributes:
        total: Total number of jobs
        completed: Number of completed jobs
        failed: Number of failed jobs
        running: Number of running jobs
        pending: Number of pending jobs
        elapsed_time: Elapsed time in seconds
    """
    total: int
    completed: int = 0
    failed: int = 0
    running: int = 0
    pending: int = 0
    elapsed_time: float = 0.0

    @property
    def percent_complete(self) -> float:
        """Get completion percentage."""
        if self.total == 0:
            return 100.0
        return (self.completed / self.total) * 100.0

    @property
    def is_complete(self) -> bool:
        """Check if batch is complete."""
        return (self.completed + self.failed) >= self.total

    @property
    def success_rate(self) -> float:
        """Get success rate."""
        finished = self.completed + self.failed
        if finished == 0:
            return 0.0
        return (self.completed / finished) * 100.0


@dataclass
class BatchResult:
    """Result of a batch operation.

    Attributes:
        results: List of successful results
        failures: List of (job_id, exception) tuples
        progress: Final progress information
        status: Overall batch status
        total_time: Total execution time in seconds
    """
    results: List[JobResult]
    failures: List[tuple[str, Exception]]
    progress: BatchProgress
    status: BatchStatus
    total_time: float

    @property
    def success_count(self) -> int:
        """Number of successful jobs."""
        return len(self.results)

    @property
    def failure_count(self) -> int:
        """Number of failed jobs."""
        return len(self.failures)

    @property
    def total_count(self) -> int:
        """Total number of jobs."""
        return self.success_count + self.failure_count


class BatchJobManager:
    """Manager for executing multiple jobs concurrently with progress tracking.

    Provides concurrent execution, progress callbacks, and partial failure handling
    for batch quantum circuit submissions.

    Example:
        >>> manager = BatchJobManager(client, max_workers=10)
        >>> futures = manager.submit_many(circuits, "simulator")
        >>> result = manager.wait_all(futures, progress_callback=print_progress)
    """

    def __init__(self, client, max_workers: int = 10):
        """Initialize batch job manager.

        Args:
            client: ArvakClient instance
            max_workers: Maximum number of concurrent workers (default: 10)
        """
        self.client = client
        self.max_workers = max_workers
        self._executor = ThreadPoolExecutor(max_workers=max_workers)

    def submit_many(
        self,
        circuits: List[tuple[str, int]],
        backend_id: str,
        format: str = "qasm3",
        poll_interval: float = 1.0,
    ) -> List[JobFuture]:
        """Submit multiple circuits concurrently.

        Args:
            circuits: List of (circuit_code, shots) tuples
            backend_id: Backend to execute on
            format: Circuit format ("qasm3" or "json")
            poll_interval: Polling interval for futures

        Returns:
            List of JobFuture objects
        """
        futures = []

        def submit_one(circuit_code, shots):
            if format == "qasm3":
                return self.client.submit_qasm_future(
                    circuit_code, backend_id, shots, poll_interval
                )
            else:
                job_id = self.client.submit_circuit_json(circuit_code, backend_id, shots)
                return JobFuture(self.client, job_id, poll_interval)

        # Submit all jobs concurrently
        submit_futures = [
            self._executor.submit(submit_one, code, shots)
            for code, shots in circuits
        ]

        # Collect JobFutures
        for future in cf_as_completed(submit_futures):
            try:
                job_future = future.result()
                futures.append(job_future)
            except Exception:
                pass  # Skip failed submissions

        return futures

    def wait_all(
        self,
        futures: List[JobFuture],
        timeout: Optional[float] = None,
        progress_callback: Optional[Callable[[BatchProgress], None]] = None,
        fail_fast: bool = False,
    ) -> BatchResult:
        """Wait for all jobs to complete with progress tracking.

        Args:
            futures: List of JobFuture objects
            timeout: Maximum time to wait (None for no limit)
            progress_callback: Callback for progress updates
            fail_fast: If True, stop on first failure

        Returns:
            BatchResult with results and failures
        """
        start_time = time.time()
        total = len(futures)

        progress = BatchProgress(
            total=total,
            pending=total,
        )

        results = []
        failures = []

        if progress_callback:
            progress_callback(progress)

        # Wait for completion
        pending = set(futures)

        while pending:
            # Check for completed futures
            newly_done = {f for f in pending if f.done()}

            for future in newly_done:
                pending.remove(future)
                progress.pending = len(pending)
                progress.elapsed_time = time.time() - start_time

                try:
                    result = future.result(timeout=0)
                    results.append(result)
                    progress.completed += 1
                except Exception as e:
                    failures.append((future.job_id, e))
                    progress.failed += 1

                    if fail_fast:
                        # Cancel remaining jobs
                        for p in pending:
                            p.cancel()
                        break

                if progress_callback:
                    progress_callback(progress)

            # Check timeout
            if timeout is not None:
                elapsed = time.time() - start_time
                if elapsed >= timeout:
                    # Cancel remaining
                    for p in pending:
                        p.cancel()
                    break

            # Update running count
            progress.running = sum(1 for f in pending if f.running())

            if pending:
                time.sleep(0.1)

        # Determine final status
        if progress.failed == 0:
            status = BatchStatus.COMPLETED
        elif progress.completed == 0:
            status = BatchStatus.FAILED
        else:
            status = BatchStatus.PARTIAL

        total_time = time.time() - start_time

        return BatchResult(
            results=results,
            failures=failures,
            progress=progress,
            status=status,
            total_time=total_time,
        )

    def as_completed(
        self,
        futures: List[JobFuture],
        timeout: Optional[float] = None,
        progress_callback: Optional[Callable[[int, int], None]] = None,
    ):
        """Yield futures as they complete with progress tracking.

        Args:
            futures: List of JobFuture objects
            timeout: Maximum time to wait
            progress_callback: Callback with (completed, total) counts

        Yields:
            Completed JobFuture objects
        """
        total = len(futures)
        completed = 0
        start_time = time.time()
        pending = set(futures)

        while pending:
            # Find completed
            newly_done = [f for f in pending if f.done()]

            for future in newly_done:
                pending.remove(future)
                completed += 1

                if progress_callback:
                    progress_callback(completed, total)

                yield future

            if not pending:
                break

            # Check timeout
            if timeout is not None:
                elapsed = time.time() - start_time
                if elapsed >= timeout:
                    raise TimeoutError(f"Not all jobs completed within {timeout}s")

            time.sleep(0.1)

    def execute_batch(
        self,
        circuits: List[tuple[str, int]],
        backend_id: str,
        format: str = "qasm3",
        timeout: Optional[float] = None,
        progress_callback: Optional[Callable[[BatchProgress], None]] = None,
        fail_fast: bool = False,
    ) -> BatchResult:
        """Submit and wait for batch execution (convenience method).

        Args:
            circuits: List of (circuit_code, shots) tuples
            backend_id: Backend to execute on
            format: Circuit format
            timeout: Maximum time to wait
            progress_callback: Progress callback
            fail_fast: Stop on first failure

        Returns:
            BatchResult
        """
        futures = self.submit_many(circuits, backend_id, format)
        return self.wait_all(futures, timeout, progress_callback, fail_fast)

    def map(
        self,
        func: Callable[[JobResult], Any],
        futures: List[JobFuture],
        timeout: Optional[float] = None,
    ) -> List[Any]:
        """Apply a function to each result as it completes.

        Args:
            func: Function to apply to each result
            futures: List of JobFuture objects
            timeout: Maximum time to wait

        Returns:
            List of function results
        """
        results = []

        for future in self.as_completed(futures, timeout):
            try:
                result = future.result()
                mapped = func(result)
                results.append(mapped)
            except Exception:
                pass  # Skip failures

        return results

    def close(self):
        """Shutdown the executor."""
        self._executor.shutdown(wait=True)

    def __enter__(self):
        """Context manager entry."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit."""
        self.close()


def print_progress_bar(progress: BatchProgress, width: int = 50):
    """Print a progress bar for batch operations.

    Args:
        progress: BatchProgress object
        width: Width of progress bar in characters
    """
    filled = int(width * progress.percent_complete / 100)
    bar = "█" * filled + "░" * (width - filled)

    status_parts = []
    if progress.running > 0:
        status_parts.append(f"{progress.running} running")
    if progress.pending > 0:
        status_parts.append(f"{progress.pending} pending")
    if progress.failed > 0:
        status_parts.append(f"{progress.failed} failed")

    status = ", ".join(status_parts) if status_parts else "all completed"

    print(
        f"\r[{bar}] {progress.percent_complete:.1f}% "
        f"({progress.completed}/{progress.total}) | {status}",
        end="",
        flush=True,
    )

    if progress.is_complete:
        print()  # New line when complete
