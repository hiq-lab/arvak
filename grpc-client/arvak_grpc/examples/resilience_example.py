#!/usr/bin/env python3
"""Example: Retry logic and circuit breaker patterns."""

from arvak_grpc import (
    ArvakClient,
    RetryPolicy,
    RetryStrategy,
    CircuitBreaker,
    CircuitBreakerConfig,
    CircuitBreakerError,
    ResilientClient,
)
import time

BELL_STATE = """
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
"""


def example_retry_policy():
    """Example: Using RetryPolicy."""
    print("=" * 60)
    print("Example 1: RetryPolicy with exponential backoff")
    print("=" * 60)

    # Create retry policy
    retry_policy = RetryPolicy(
        max_attempts=5,
        initial_backoff=0.5,
        max_backoff=10.0,
        backoff_multiplier=2.0,
        strategy=RetryStrategy.EXPONENTIAL_BACKOFF,
        jitter=True,
    )

    print(f"\nRetry Policy:")
    print(f"  Max attempts: {retry_policy.max_attempts}")
    print(f"  Initial backoff: {retry_policy.initial_backoff}s")
    print(f"  Strategy: {retry_policy.strategy.value}")

    print("\nBackoff delays for each attempt:")
    for attempt in range(5):
        delay = retry_policy.get_backoff_delay(attempt)
        print(f"  Attempt {attempt + 1}: ~{delay:.2f}s")


def example_circuit_breaker():
    """Example: Circuit breaker pattern."""
    print("\n" + "=" * 60)
    print("Example 2: Circuit Breaker")
    print("=" * 60)

    # Create circuit breaker
    breaker = CircuitBreaker(
        CircuitBreakerConfig(
            failure_threshold=3,
            success_threshold=2,
            timeout=5.0,
        )
    )

    print(f"\nInitial state: {breaker.state.value}")

    # Simulate failures
    print("\nSimulating failures...")
    for i in range(4):
        breaker.record_failure()
        print(f"  Failure {i + 1}: state = {breaker.state.value}")

    print(f"\nCircuit is open: {breaker.is_open()}")

    # Try to proceed (should fail)
    if not breaker.can_proceed():
        print("Request blocked - circuit is open!")

    # Wait for timeout
    print(f"\nWaiting {breaker.config.timeout}s for timeout...")
    time.sleep(breaker.config.timeout + 0.1)

    print(f"After timeout: state = {breaker.state.value}")
    print(f"Can proceed: {breaker.can_proceed()}")

    # Record successes to close circuit
    print("\nRecording successes...")
    breaker.record_success()
    print(f"  After 1 success: {breaker.state.value}")
    breaker.record_success()
    print(f"  After 2 successes: {breaker.state.value}")


def example_resilient_client():
    """Example: ResilientClient wrapper."""
    print("\n" + "=" * 60)
    print("Example 3: ResilientClient (Retry + Circuit Breaker)")
    print("=" * 60)

    # Create base client
    base_client = ArvakClient("localhost:50051")

    # Wrap with resilience
    client = ResilientClient(
        base_client,
        retry_policy=RetryPolicy(
            max_attempts=3,
            initial_backoff=1.0,
            strategy=RetryStrategy.EXPONENTIAL_BACKOFF,
        ),
        circuit_breaker_config=CircuitBreakerConfig(
            failure_threshold=5,
            timeout=30.0,
        ),
    )

    try:
        # All method calls now have automatic retry and circuit breaker
        print("\nListing backends (with retry)...")
        backends = client.list_backends()
        print(f"Found {len(backends)} backend(s)")

        print("\nSubmitting job (with retry)...")
        job_id = client.submit_qasm(BELL_STATE, "simulator", shots=1000)
        print(f"Job ID: {job_id}")

        print("\nWaiting for result...")
        result = client.wait_for_job(job_id, max_wait=30)
        print(f"Success! Got {len(result.counts)} measurement states")

    except CircuitBreakerError as e:
        print(f"\nCircuit breaker opened: {e}")
    except Exception as e:
        print(f"\nError after retries: {e}")
    finally:
        client.close()


def example_retry_strategies():
    """Example: Different retry strategies."""
    print("\n" + "=" * 60)
    print("Example 4: Retry Strategies Comparison")
    print("=" * 60)

    strategies = [
        RetryStrategy.EXPONENTIAL_BACKOFF,
        RetryStrategy.LINEAR_BACKOFF,
        RetryStrategy.CONSTANT,
    ]

    for strategy in strategies:
        policy = RetryPolicy(
            max_attempts=5,
            initial_backoff=1.0,
            backoff_multiplier=2.0,
            strategy=strategy,
            jitter=False,  # Disable jitter for clearer comparison
        )

        print(f"\n{strategy.value}:")
        delays = [policy.get_backoff_delay(i) for i in range(5)]
        print(f"  Delays: {[f'{d:.1f}s' for d in delays]}")


def example_context_manager():
    """Example: ResilientClient as context manager."""
    print("\n" + "=" * 60)
    print("Example 5: ResilientClient Context Manager")
    print("=" * 60)

    retry_policy = RetryPolicy(max_attempts=3, initial_backoff=0.5)

    print("\nUsing context manager...")
    with ResilientClient(
        ArvakClient("localhost:50051"),
        retry_policy=retry_policy,
    ) as client:
        backends = client.list_backends()
        print(f"Found {len(backends)} backend(s)")

        # Submit a quick job
        job_id = client.submit_qasm(BELL_STATE, "simulator", shots=100)
        print(f"Job submitted: {job_id[:8]}...")

    print("Client automatically closed!")


def example_custom_retry():
    """Example: Custom retry behavior."""
    print("\n" + "=" * 60)
    print("Example 6: Custom Retry Configuration")
    print("=" * 60)

    import grpc

    # Create custom retry policy that retries more errors
    custom_policy = RetryPolicy(
        max_attempts=5,
        initial_backoff=0.5,
        max_backoff=30.0,
        backoff_multiplier=3.0,
        strategy=RetryStrategy.EXPONENTIAL_BACKOFF,
        retryable_status_codes=[
            grpc.StatusCode.UNAVAILABLE,
            grpc.StatusCode.DEADLINE_EXCEEDED,
            grpc.StatusCode.RESOURCE_EXHAUSTED,
            grpc.StatusCode.ABORTED,
            grpc.StatusCode.INTERNAL,  # Also retry internal errors
        ],
    )

    print(f"\nCustom retry policy:")
    print(f"  Max attempts: {custom_policy.max_attempts}")
    print(f"  Retryable codes: {len(custom_policy.retryable_status_codes)}")
    print(f"  Backoff: {custom_policy.initial_backoff}s to {custom_policy.max_backoff}s")

    with ResilientClient(
        ArvakClient("localhost:50051"),
        retry_policy=custom_policy,
    ) as client:
        backends = client.list_backends()
        print(f"\nSuccessfully listed {len(backends)} backend(s) with custom policy")


if __name__ == "__main__":
    example_retry_policy()
    example_circuit_breaker()
    example_resilient_client()
    example_retry_strategies()
    example_context_manager()
    example_custom_retry()

    print("\n" + "=" * 60)
    print("All resilience examples completed!")
    print("=" * 60)
