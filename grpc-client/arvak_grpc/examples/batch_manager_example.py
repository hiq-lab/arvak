#!/usr/bin/env python3
"""Example: BatchJobManager for concurrent batch operations."""

from arvak_grpc import ArvakClient, BatchJobManager, BatchProgress, print_progress_bar
import time

# Test circuits with varying complexity
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


def example_basic_batch():
    """Basic batch submission and waiting."""
    print("=" * 60)
    print("Example 1: Basic Batch Execution")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    with BatchJobManager(client, max_workers=5) as manager:
        # Prepare batch
        circuits = [
            (BELL_STATE, 500),
            (GHZ_STATE, 500),
            (SUPERPOSITION, 500),
            (BELL_STATE, 1000),
            (GHZ_STATE, 1000),
        ]

        print(f"\nSubmitting batch of {len(circuits)} jobs...")

        # Execute batch with progress bar
        result = manager.execute_batch(
            circuits,
            "simulator",
            progress_callback=print_progress_bar,
        )

        print(f"\nBatch completed!")
        print(f"  Successful: {result.success_count}")
        print(f"  Failed: {result.failure_count}")
        print(f"  Total time: {result.total_time:.2f}s")
        print(f"  Success rate: {result.progress.success_rate:.1f}%")

    client.close()


def example_custom_progress():
    """Custom progress callback."""
    print("\n" + "=" * 60)
    print("Example 2: Custom Progress Callback")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    def my_progress(progress: BatchProgress):
        print(
            f"Progress: {progress.completed}/{progress.total} completed, "
            f"{progress.failed} failed, {progress.running} running "
            f"({progress.elapsed_time:.1f}s elapsed)"
        )

    with BatchJobManager(client, max_workers=10) as manager:
        circuits = [(BELL_STATE, 200) for _ in range(10)]

        print(f"\nSubmitting {len(circuits)} jobs...")
        result = manager.execute_batch(
            circuits,
            "simulator",
            progress_callback=my_progress,
        )

        print(f"\nAll jobs completed in {result.total_time:.2f}s")

    client.close()


def example_as_completed():
    """Process results as they complete."""
    print("\n" + "=" * 60)
    print("Example 3: Process Results as Completed")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    with BatchJobManager(client, max_workers=8) as manager:
        circuits = [
            (BELL_STATE, 300),
            (GHZ_STATE, 300),
            (SUPERPOSITION, 300),
            (BELL_STATE, 600),
            (GHZ_STATE, 600),
        ]

        print(f"\nSubmitting {len(circuits)} jobs...")
        futures = manager.submit_many(circuits, "simulator")

        print("Processing results as they complete...")
        for i, future in enumerate(manager.as_completed(futures), 1):
            result = future.result()
            most_freq = result.most_frequent()
            print(
                f"  Job {i}/{len(circuits)}: {result.job_id[:8]}... "
                f"| {result.shots} shots | Most frequent: {most_freq[0] if most_freq else 'N/A'}"
            )

        print("\nAll results processed!")

    client.close()


def example_map_function():
    """Apply function to each result."""
    print("\n" + "=" * 60)
    print("Example 4: Map Function Over Results")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    def extract_entropy(result):
        """Calculate Shannon entropy from measurement counts."""
        import math

        probs = result.probabilities()
        entropy = -sum(p * math.log2(p) for p in probs.values() if p > 0)
        return entropy

    with BatchJobManager(client, max_workers=5) as manager:
        circuits = [
            (BELL_STATE, 1000),
            (GHZ_STATE, 1000),
            (SUPERPOSITION, 1000),
        ]

        print(f"\nSubmitting {len(circuits)} jobs...")
        futures = manager.submit_many(circuits, "simulator")

        print("Calculating entropy for each result...")
        entropies = manager.map(extract_entropy, futures, timeout=30)

        print("\nEntropy results:")
        for i, entropy in enumerate(entropies, 1):
            print(f"  Circuit {i}: {entropy:.4f} bits")

    client.close()


def example_fail_fast():
    """Demonstrate fail-fast behavior."""
    print("\n" + "=" * 60)
    print("Example 5: Fail-Fast Mode")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    with BatchJobManager(client, max_workers=5) as manager:
        # Mix valid and invalid circuits
        circuits = [
            (BELL_STATE, 200),
            (BELL_STATE, 200),
            ("invalid qasm", 200),  # This will fail
            (BELL_STATE, 200),
            (BELL_STATE, 200),
        ]

        print(f"\nSubmitting batch with one invalid circuit...")
        print("Using fail_fast=True (will stop on first failure)")

        try:
            result = manager.execute_batch(
                circuits,
                "simulator",
                fail_fast=True,
                progress_callback=print_progress_bar,
            )

            print(f"\nPartial completion:")
            print(f"  Successful: {result.success_count}")
            print(f"  Failed: {result.failure_count}")
            print(f"  Status: {result.status.value}")

        except Exception as e:
            print(f"\nCaught exception: {e}")

    client.close()


def example_performance_benchmark():
    """Benchmark concurrent vs sequential execution."""
    print("\n" + "=" * 60)
    print("Example 6: Performance Benchmark")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    num_jobs = 20
    circuits = [(BELL_STATE, 100) for _ in range(num_jobs)]

    # Concurrent execution
    print(f"\nBenchmark: {num_jobs} jobs with max_workers=10")
    with BatchJobManager(client, max_workers=10) as manager:
        start = time.time()
        result = manager.execute_batch(circuits, "simulator")
        concurrent_time = time.time() - start

    print(f"Concurrent execution: {concurrent_time:.2f}s")
    print(f"Average: {concurrent_time / num_jobs:.2f}s per job")
    print(f"Throughput: {num_jobs / concurrent_time:.1f} jobs/second")

    # Sequential execution
    print(f"\nBenchmark: {num_jobs} jobs with max_workers=1")
    with BatchJobManager(client, max_workers=1) as manager:
        start = time.time()
        result = manager.execute_batch(circuits, "simulator")
        sequential_time = time.time() - start

    print(f"Sequential execution: {sequential_time:.2f}s")
    print(f"Average: {sequential_time / num_jobs:.2f}s per job")
    print(f"Throughput: {num_jobs / sequential_time:.1f} jobs/second")

    speedup = sequential_time / concurrent_time
    print(f"\nSpeedup: {speedup:.2f}x faster with concurrency")

    client.close()


def example_large_batch():
    """Handle a large batch efficiently."""
    print("\n" + "=" * 60)
    print("Example 7: Large Batch (100 jobs)")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    with BatchJobManager(client, max_workers=20) as manager:
        # Create 100 jobs
        circuits = [(BELL_STATE, 50) for _ in range(100)]

        print(f"\nSubmitting batch of {len(circuits)} jobs...")
        start = time.time()

        result = manager.execute_batch(
            circuits,
            "simulator",
            timeout=120,
            progress_callback=print_progress_bar,
        )

        elapsed = time.time() - start

        print(f"\nBatch statistics:")
        print(f"  Total jobs: {result.total_count}")
        print(f"  Successful: {result.success_count}")
        print(f"  Failed: {result.failure_count}")
        print(f"  Total time: {elapsed:.2f}s")
        print(f"  Throughput: {result.success_count / elapsed:.1f} jobs/second")
        print(f"  Success rate: {result.progress.success_rate:.1f}%")

    client.close()


if __name__ == "__main__":
    example_basic_batch()
    example_custom_progress()
    example_as_completed()
    example_map_function()
    example_fail_fast()
    example_performance_benchmark()
    example_large_batch()

    print("\n" + "=" * 60)
    print("All batch manager examples completed!")
    print("=" * 60)
