"""Retry policies and resilience features for the Arvak gRPC client."""

import time
import random
from dataclasses import dataclass, field
from typing import Optional, List, Callable
from enum import Enum

import grpc


class RetryStrategy(Enum):
    """Retry strategy types."""
    EXPONENTIAL_BACKOFF = "exponential_backoff"
    LINEAR_BACKOFF = "linear_backoff"
    CONSTANT = "constant"


@dataclass
class RetryPolicy:
    """Configuration for retry behavior.

    Attributes:
        max_attempts: Maximum number of retry attempts (default: 3)
        initial_backoff: Initial backoff delay in seconds (default: 1.0)
        max_backoff: Maximum backoff delay in seconds (default: 60.0)
        backoff_multiplier: Backoff multiplier for exponential strategy (default: 2.0)
        jitter: Add random jitter to backoff (default: True)
        strategy: Retry strategy to use (default: EXPONENTIAL_BACKOFF)
        retryable_status_codes: gRPC status codes to retry (default: transient errors)
    """
    max_attempts: int = 3
    initial_backoff: float = 1.0
    max_backoff: float = 60.0
    backoff_multiplier: float = 2.0
    jitter: bool = True
    strategy: RetryStrategy = RetryStrategy.EXPONENTIAL_BACKOFF
    retryable_status_codes: List[grpc.StatusCode] = field(default_factory=lambda: [
        grpc.StatusCode.UNAVAILABLE,
        grpc.StatusCode.DEADLINE_EXCEEDED,
        grpc.StatusCode.RESOURCE_EXHAUSTED,
        grpc.StatusCode.ABORTED,
    ])

    def should_retry(self, error: grpc.RpcError, attempt: int) -> bool:
        """Check if an error should trigger a retry.

        Args:
            error: The gRPC error
            attempt: Current attempt number (0-indexed)

        Returns:
            True if should retry, False otherwise
        """
        if attempt >= self.max_attempts:
            return False

        if not isinstance(error, grpc.RpcError):
            return False

        return error.code() in self.retryable_status_codes

    def get_backoff_delay(self, attempt: int) -> float:
        """Calculate backoff delay for the given attempt.

        Args:
            attempt: Attempt number (0-indexed)

        Returns:
            Delay in seconds
        """
        if self.strategy == RetryStrategy.EXPONENTIAL_BACKOFF:
            delay = self.initial_backoff * (self.backoff_multiplier ** attempt)
        elif self.strategy == RetryStrategy.LINEAR_BACKOFF:
            delay = self.initial_backoff * (attempt + 1)
        else:  # CONSTANT
            delay = self.initial_backoff

        # Apply max backoff
        delay = min(delay, self.max_backoff)

        # Add jitter
        if self.jitter:
            delay = delay * (0.5 + random.random())

        return delay


@dataclass
class CircuitBreakerConfig:
    """Configuration for circuit breaker pattern.

    Attributes:
        failure_threshold: Number of failures before opening circuit (default: 5)
        success_threshold: Number of successes to close circuit (default: 2)
        timeout: Time in seconds before attempting to close circuit (default: 60.0)
        half_open_max_calls: Max calls allowed in half-open state (default: 1)
    """
    failure_threshold: int = 5
    success_threshold: int = 2
    timeout: float = 60.0
    half_open_max_calls: int = 1


class CircuitState(Enum):
    """Circuit breaker states."""
    CLOSED = "closed"        # Normal operation
    OPEN = "open"           # Rejecting requests
    HALF_OPEN = "half_open"  # Testing if service recovered


class CircuitBreaker:
    """Circuit breaker for preventing cascading failures.

    Tracks failures and opens the circuit when threshold is exceeded,
    preventing additional requests until the service recovers.
    """

    def __init__(self, config: CircuitBreakerConfig):
        """Initialize circuit breaker.

        Args:
            config: Circuit breaker configuration
        """
        self.config = config
        self._state = CircuitState.CLOSED
        self._failure_count = 0
        self._success_count = 0
        self._last_failure_time: Optional[float] = None
        self._half_open_calls = 0

    @property
    def state(self) -> CircuitState:
        """Get current circuit state."""
        return self._state

    def is_open(self) -> bool:
        """Check if circuit is open."""
        if self._state == CircuitState.OPEN:
            # Check if timeout has passed
            if self._last_failure_time is not None:
                elapsed = time.time() - self._last_failure_time
                if elapsed >= self.config.timeout:
                    self._transition_to_half_open()
                    return False
            return True
        return False

    def can_proceed(self) -> bool:
        """Check if request can proceed.

        Returns:
            True if request should be allowed, False otherwise
        """
        if self._state == CircuitState.CLOSED:
            return True

        if self._state == CircuitState.OPEN:
            return not self.is_open()

        # HALF_OPEN state
        if self._half_open_calls < self.config.half_open_max_calls:
            self._half_open_calls += 1
            return True
        return False

    def record_success(self):
        """Record a successful call."""
        if self._state == CircuitState.HALF_OPEN:
            self._success_count += 1
            if self._success_count >= self.config.success_threshold:
                self._transition_to_closed()
        elif self._state == CircuitState.CLOSED:
            self._failure_count = max(0, self._failure_count - 1)

    def record_failure(self):
        """Record a failed call."""
        self._failure_count += 1
        self._last_failure_time = time.time()

        if self._state == CircuitState.HALF_OPEN:
            self._transition_to_open()
        elif self._state == CircuitState.CLOSED:
            if self._failure_count >= self.config.failure_threshold:
                self._transition_to_open()

    def _transition_to_open(self):
        """Transition to OPEN state."""
        self._state = CircuitState.OPEN
        self._success_count = 0
        self._half_open_calls = 0

    def _transition_to_half_open(self):
        """Transition to HALF_OPEN state."""
        self._state = CircuitState.HALF_OPEN
        self._half_open_calls = 0
        self._success_count = 0

    def _transition_to_closed(self):
        """Transition to CLOSED state."""
        self._state = CircuitState.CLOSED
        self._failure_count = 0
        self._success_count = 0
        self._half_open_calls = 0


class CircuitBreakerError(Exception):
    """Raised when circuit breaker is open."""
    pass


def with_retry(retry_policy: Optional[RetryPolicy] = None):
    """Decorator to add retry logic to functions.

    Args:
        retry_policy: Retry policy to use (default: RetryPolicy())

    Example:
        @with_retry(RetryPolicy(max_attempts=5))
        def make_request():
            # Make gRPC call
            pass
    """
    if retry_policy is None:
        retry_policy = RetryPolicy()

    def decorator(func: Callable):
        def wrapper(*args, **kwargs):
            last_error = None

            for attempt in range(retry_policy.max_attempts):
                try:
                    return func(*args, **kwargs)
                except grpc.RpcError as e:
                    last_error = e
                    if not retry_policy.should_retry(e, attempt):
                        raise

                    if attempt < retry_policy.max_attempts - 1:
                        delay = retry_policy.get_backoff_delay(attempt)
                        time.sleep(delay)

            # All retries exhausted
            raise last_error

        return wrapper
    return decorator


def with_circuit_breaker(circuit_breaker: CircuitBreaker):
    """Decorator to add circuit breaker protection.

    Args:
        circuit_breaker: Circuit breaker instance

    Example:
        breaker = CircuitBreaker(CircuitBreakerConfig())

        @with_circuit_breaker(breaker)
        def make_request():
            # Make gRPC call
            pass
    """
    def decorator(func: Callable):
        def wrapper(*args, **kwargs):
            if not circuit_breaker.can_proceed():
                raise CircuitBreakerError(
                    f"Circuit breaker is {circuit_breaker.state.value}"
                )

            try:
                result = func(*args, **kwargs)
                circuit_breaker.record_success()
                return result
            except Exception as e:
                circuit_breaker.record_failure()
                raise

        return wrapper
    return decorator


class ResilientClient:
    """Wrapper that adds retry and circuit breaker to a client.

    Example:
        client = ArvakClient("localhost:50051")
        resilient = ResilientClient(
            client,
            retry_policy=RetryPolicy(max_attempts=5),
            circuit_breaker_config=CircuitBreakerConfig()
        )

        # Now all calls go through retry and circuit breaker
        job_id = resilient.submit_qasm(qasm, "simulator", shots=1000)
    """

    def __init__(
        self,
        client,
        retry_policy: Optional[RetryPolicy] = None,
        circuit_breaker_config: Optional[CircuitBreakerConfig] = None,
    ):
        """Initialize resilient client wrapper.

        Args:
            client: Underlying ArvakClient or AsyncArvakClient
            retry_policy: Retry policy (default: RetryPolicy())
            circuit_breaker_config: Circuit breaker config (default: None)
        """
        self._client = client
        self._retry_policy = retry_policy or RetryPolicy()
        self._circuit_breaker = (
            CircuitBreaker(circuit_breaker_config)
            if circuit_breaker_config
            else None
        )

    def __getattr__(self, name):
        """Proxy method calls to underlying client with retry and circuit breaker."""
        attr = getattr(self._client, name)

        if not callable(attr):
            return attr

        def resilient_method(*args, **kwargs):
            last_error = None

            for attempt in range(self._retry_policy.max_attempts):
                # Check circuit breaker
                if self._circuit_breaker and not self._circuit_breaker.can_proceed():
                    raise CircuitBreakerError(
                        f"Circuit breaker is {self._circuit_breaker.state.value}"
                    )

                try:
                    result = attr(*args, **kwargs)

                    # Record success
                    if self._circuit_breaker:
                        self._circuit_breaker.record_success()

                    return result

                except grpc.RpcError as e:
                    last_error = e

                    # Record failure
                    if self._circuit_breaker:
                        self._circuit_breaker.record_failure()

                    # Check if should retry
                    if not self._retry_policy.should_retry(e, attempt):
                        raise

                    # Calculate backoff
                    if attempt < self._retry_policy.max_attempts - 1:
                        delay = self._retry_policy.get_backoff_delay(attempt)
                        time.sleep(delay)

            # All retries exhausted
            raise last_error

        return resilient_method

    def close(self):
        """Close underlying client."""
        if hasattr(self._client, 'close'):
            self._client.close()

    def __enter__(self):
        """Context manager entry."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit."""
        self.close()
