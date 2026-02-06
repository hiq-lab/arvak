#!/usr/bin/env python3
"""Example: Using JobFuture for non-blocking job results."""

from arvak_grpc import ArvakClient, as_completed, wait

# Test circuits
BELL_STATE = """
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
"""

GHZ_STATE = """
OPENQASM 3.0;
qubit[3] q;
h q[0];
cx q[0], q[1];
cx q[1], q[2];
"""

SUPERPOSITION = """
OPENQASM 3.0;
qubit[2] q;
h q[0];
h q[1];
"""


def example_basic_future():
    """Basic JobFuture usage."""
    print("=" * 60)
    print("Example 1: Basic JobFuture")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Submit and get future
    print("\nSubmitting job...")
    future = client.submit_qasm_future(BELL_STATE, "simulator", shots=1000)
    print(f"Job ID: {future.job_id}")

    # Register callback
    def on_done(f):
        try:
            result = f.result()
            print(f"\nCallback: Job completed!")
            print(f"  Most frequent: {result.most_frequent()}")
        except Exception as e:
            print(f"Callback: Job failed: {e}")

    future.add_done_callback(on_done)

    # Do other work while job runs
    print("Doing other work...")
    import time
    time.sleep(0.5)

    # Block until complete
    print("Waiting for result...")
    result = future.result(timeout=30)

    print(f"\nResults:")
    for bitstring, count in sorted(result.counts.items()):
        print(f"  {bitstring}: {count}")

    client.close()


def example_multiple_futures():
    """Multiple JobFutures with as_completed."""
    print("\n" + "=" * 60)
    print("Example 2: Multiple JobFutures with as_completed()")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Submit multiple jobs
    print("\nSubmitting 3 jobs...")
    futures = [
        client.submit_qasm_future(BELL_STATE, "simulator", shots=500),
        client.submit_qasm_future(GHZ_STATE, "simulator", shots=500),
        client.submit_qasm_future(SUPERPOSITION, "simulator", shots=500),
    ]

    print(f"Submitted {len(futures)} jobs")

    # Process results as they complete
    print("\nWaiting for completion...")
    for i, future in enumerate(as_completed(futures, timeout=30), 1):
        result = future.result()
        print(f"Job {i} completed: {future.job_id[:8]}... ({len(result.counts)} states)")

    print("\nAll jobs completed!")
    client.close()


def example_wait_first():
    """Using wait() to wait for first completion."""
    print("\n" + "=" * 60)
    print("Example 3: wait() for first completion")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Submit jobs
    print("\nSubmitting 5 jobs...")
    futures = [
        client.submit_qasm_future(BELL_STATE, "simulator", shots=100)
        for _ in range(5)
    ]

    # Wait for first to complete
    print("Waiting for first completion...")
    done, pending = wait(futures, return_when="FIRST_COMPLETED")

    print(f"\nFirst completed: {len(done)} job(s)")
    print(f"Still pending: {len(pending)} job(s)")

    # Get result from first completed
    first_result = next(iter(done)).result()
    print(f"First result: {first_result.job_id[:8]}...")

    # Wait for all remaining
    print("\nWaiting for remaining jobs...")
    done, pending = wait(pending, timeout=30)
    print(f"All {len(done) + len(futures) - len(pending)} jobs completed!")

    client.close()


def example_concurrent_futures_integration():
    """Integration with concurrent.futures."""
    print("\n" + "=" * 60)
    print("Example 4: concurrent.futures integration")
    print("=" * 60)

    from concurrent.futures import ThreadPoolExecutor, as_completed as cf_as_completed

    client = ArvakClient("localhost:50051")

    # Submit jobs and convert to concurrent.futures
    print("\nSubmitting 3 jobs...")
    job_futures = [
        client.submit_qasm_future(BELL_STATE, "simulator", shots=300),
        client.submit_qasm_future(GHZ_STATE, "simulator", shots=300),
        client.submit_qasm_future(SUPERPOSITION, "simulator", shots=300),
    ]

    # Convert to concurrent.futures
    concurrent_futures = [f.as_concurrent_future() for f in job_futures]

    # Use concurrent.futures.as_completed
    print("Processing with concurrent.futures...")
    for future in cf_as_completed(concurrent_futures, timeout=30):
        result = future.result()
        print(f"  Completed: {result.job_id[:8]}... ({result.shots} shots)")

    print("\nAll jobs processed!")
    client.close()


def example_cancel():
    """Canceling jobs with JobFuture."""
    print("\n" + "=" * 60)
    print("Example 5: Canceling jobs")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Submit a job
    print("\nSubmitting job...")
    future = client.submit_qasm_future(BELL_STATE, "simulator", shots=1000)
    print(f"Job ID: {future.job_id}")

    # Try to cancel immediately
    print("Attempting to cancel...")
    cancelled = future.cancel()

    if cancelled:
        print("Job cancelled successfully")
        print(f"Future cancelled: {future.cancelled()}")
    else:
        print("Job could not be cancelled (likely already completed)")
        # Get result anyway
        result = future.result()
        print(f"Got result: {len(result.counts)} states")

    client.close()


def example_batch_futures():
    """Batch submission with JobFutures."""
    print("\n" + "=" * 60)
    print("Example 6: Batch submission with JobFutures")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Submit batch and get futures
    circuits = [
        (BELL_STATE, 200),
        (GHZ_STATE, 200),
        (SUPERPOSITION, 200),
        (BELL_STATE, 400),
        (GHZ_STATE, 400),
    ]

    print(f"\nSubmitting batch of {len(circuits)} jobs...")
    futures = client.submit_batch_future(circuits, "simulator")

    # Track progress
    completed = 0
    for future in as_completed(futures, timeout=30):
        completed += 1
        result = future.result()
        print(f"  [{completed}/{len(futures)}] {result.job_id[:8]}... - {result.shots} shots")

    print(f"\nAll {len(futures)} jobs completed!")
    client.close()


if __name__ == "__main__":
    example_basic_future()
    example_multiple_futures()
    example_wait_first()
    example_concurrent_futures_integration()
    example_cancel()
    example_batch_futures()

    print("\n" + "=" * 60)
    print("All examples completed successfully!")
    print("=" * 60)
