"""JobFuture implementation for non-blocking job result retrieval."""

import threading
import time
from concurrent.futures import Future as ConcurrentFuture
from typing import Optional, Callable, Any, List

from .types import Job, JobResult, JobState
from .exceptions import ArvakError


class JobFuture:
    """A Future-like object for Arvak job results.

    Provides a promise-like interface for retrieving job results with
    support for callbacks, timeouts, and integration with concurrent.futures.

    Example:
        >>> future = client.submit_qasm_future(qasm, "simulator", shots=1000)
        >>> future.add_done_callback(lambda f: print(f"Done: {f.result()}"))
        >>> result = future.result(timeout=30)  # Blocks until complete
    """

    def __init__(self, client, job_id: str, poll_interval: float = 1.0):
        """Initialize JobFuture.

        Args:
            client: ArvakClient instance
            job_id: The job ID
            poll_interval: Polling interval in seconds (default: 1.0)
        """
        self._client = client
        self._job_id = job_id
        self._poll_interval = poll_interval
        self._result: Optional[JobResult] = None
        self._exception: Optional[Exception] = None
        self._done = False
        self._cancelled = False
        self._callbacks: List[Callable[[JobFuture], Any]] = []
        self._lock = threading.Lock()
        self._condition = threading.Condition(self._lock)

        # Start background thread to poll for completion
        self._thread = threading.Thread(target=self._poll_loop, daemon=True)
        self._thread.start()

    @property
    def job_id(self) -> str:
        """Get the job ID."""
        return self._job_id

    def done(self) -> bool:
        """Return True if the job has completed (success, failure, or cancelled)."""
        with self._lock:
            return self._done

    def cancelled(self) -> bool:
        """Return True if the job was cancelled."""
        with self._lock:
            return self._cancelled

    def running(self) -> bool:
        """Return True if the job is currently running."""
        if self.done():
            return False

        try:
            job = self._client.get_job_status(self._job_id)
            return job.state in [JobState.RUNNING, JobState.QUEUED]
        except Exception:
            return False

    def cancel(self) -> bool:
        """Attempt to cancel the job.

        Returns:
            True if the job was successfully cancelled, False otherwise
        """
        with self._lock:
            if self._done:
                return False

            try:
                success, _ = self._client.cancel_job(self._job_id)
                if success:
                    self._cancelled = True
                    self._done = True
                    self._condition.notify_all()
                    self._run_callbacks()
                return success
            except Exception:
                return False

    def result(self, timeout: Optional[float] = None) -> JobResult:
        """Get the job result, blocking until available.

        Args:
            timeout: Maximum time to wait in seconds (None for no limit)

        Returns:
            JobResult object

        Raises:
            TimeoutError: If timeout is exceeded
            ArvakError: If the job failed
            CancelledError: If the job was cancelled
        """
        with self._condition:
            if not self._done:
                if not self._condition.wait(timeout=timeout):
                    raise TimeoutError(f"Job {self._job_id} did not complete within {timeout} seconds")

            if self._cancelled:
                raise CancelledError(f"Job {self._job_id} was cancelled")

            if self._exception is not None:
                raise self._exception

            return self._result

    def exception(self, timeout: Optional[float] = None) -> Optional[Exception]:
        """Get the exception raised by the job, if any.

        Args:
            timeout: Maximum time to wait in seconds (None for no limit)

        Returns:
            Exception object if the job failed, None otherwise

        Raises:
            TimeoutError: If timeout is exceeded
            CancelledError: If the job was cancelled
        """
        with self._condition:
            if not self._done:
                if not self._condition.wait(timeout=timeout):
                    raise TimeoutError(f"Job {self._job_id} did not complete within {timeout} seconds")

            if self._cancelled:
                raise CancelledError(f"Job {self._job_id} was cancelled")

            return self._exception

    def add_done_callback(self, fn: Callable[["JobFuture"], Any]):
        """Add a callback to be called when the job completes.

        The callback will be called with the JobFuture as its only argument.
        If the job is already done, the callback is called immediately.

        Args:
            fn: Callback function
        """
        with self._lock:
            if self._done:
                # Job already done, call immediately
                try:
                    fn(self)
                except Exception:
                    pass  # Ignore callback exceptions
            else:
                self._callbacks.append(fn)

    def wait(self, timeout: Optional[float] = None) -> bool:
        """Wait for the job to complete.

        Args:
            timeout: Maximum time to wait in seconds (None for no limit)

        Returns:
            True if the job completed, False if timeout occurred
        """
        with self._condition:
            if self._done:
                return True
            return self._condition.wait(timeout=timeout)

    def as_concurrent_future(self) -> ConcurrentFuture:
        """Convert to a concurrent.futures.Future.

        Returns:
            A concurrent.futures.Future that tracks this job
        """
        future = ConcurrentFuture()

        def done_callback(job_future):
            try:
                result = job_future.result()
                future.set_result(result)
            except CancelledError:
                future.cancel()
            except Exception as e:
                future.set_exception(e)

        self.add_done_callback(done_callback)
        return future

    def _poll_loop(self):
        """Background polling loop to check job status."""
        try:
            while True:
                with self._lock:
                    if self._done:
                        return

                try:
                    job = self._client.get_job_status(self._job_id)

                    if job.state == JobState.COMPLETED:
                        result = self._client.get_job_result(self._job_id)
                        with self._lock:
                            self._result = result
                            self._done = True
                            self._condition.notify_all()
                        self._run_callbacks()
                        return

                    elif job.state == JobState.FAILED:
                        with self._lock:
                            self._exception = ArvakError(f"Job failed: {job.error_message}")
                            self._done = True
                            self._condition.notify_all()
                        self._run_callbacks()
                        return

                    elif job.state == JobState.CANCELED:
                        with self._lock:
                            self._cancelled = True
                            self._done = True
                            self._condition.notify_all()
                        self._run_callbacks()
                        return

                except Exception as e:
                    # Temporary error, keep polling
                    pass

                time.sleep(self._poll_interval)

        except Exception as e:
            with self._lock:
                self._exception = e
                self._done = True
                self._condition.notify_all()
            self._run_callbacks()

    def _run_callbacks(self):
        """Run all registered callbacks."""
        for callback in self._callbacks:
            try:
                callback(self)
            except Exception:
                pass  # Ignore callback exceptions


class CancelledError(Exception):
    """Raised when a job was cancelled."""
    pass


def as_completed(futures: List[JobFuture], timeout: Optional[float] = None):
    """Yield futures as they complete.

    Args:
        futures: List of JobFuture objects
        timeout: Maximum time to wait for all futures (None for no limit)

    Yields:
        JobFuture objects as they complete

    Raises:
        TimeoutError: If timeout is exceeded
    """
    start_time = time.time()
    pending = set(futures)

    while pending:
        # Check for completed futures
        completed = [f for f in pending if f.done()]

        for future in completed:
            pending.remove(future)
            yield future

        if not pending:
            break

        # Check timeout
        if timeout is not None:
            elapsed = time.time() - start_time
            if elapsed >= timeout:
                raise TimeoutError(f"Not all futures completed within {timeout} seconds")

        # Wait a bit before checking again
        time.sleep(0.1)


def wait(futures: List[JobFuture], timeout: Optional[float] = None, return_when: str = "ALL_COMPLETED"):
    """Wait for futures to complete.

    Args:
        futures: List of JobFuture objects
        timeout: Maximum time to wait (None for no limit)
        return_when: When to return ("ALL_COMPLETED", "FIRST_COMPLETED", "FIRST_EXCEPTION")

    Returns:
        Tuple of (done_futures, not_done_futures)

    Raises:
        TimeoutError: If timeout is exceeded and return_when="ALL_COMPLETED"
    """
    start_time = time.time()
    pending = set(futures)
    done = set()

    while pending:
        # Check completed futures
        newly_done = {f for f in pending if f.done()}

        if newly_done:
            done.update(newly_done)
            pending -= newly_done

            # Check return conditions
            if return_when == "FIRST_COMPLETED" and done:
                return done, pending

            if return_when == "FIRST_EXCEPTION":
                for future in newly_done:
                    if future.exception(timeout=0) is not None:
                        return done, pending

        if not pending:
            break

        # Check timeout
        if timeout is not None:
            elapsed = time.time() - start_time
            if elapsed >= timeout:
                if return_when == "ALL_COMPLETED":
                    raise TimeoutError(f"Not all futures completed within {timeout} seconds")
                return done, pending

        # Wait a bit
        time.sleep(0.1)

    return done, pending
