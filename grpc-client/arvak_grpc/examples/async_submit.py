#!/usr/bin/env python3
"""Example: Async client with concurrent job submission."""

import asyncio
from arvak_grpc import AsyncArvakClient

# Bell state circuit
BELL_STATE_QASM = """
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
"""

# GHZ state circuit
GHZ_STATE_QASM = """
OPENQASM 3.0;
qubit[3] q;
h q[0];
cx q[0], q[1];
cx q[1], q[2];
"""

# Superposition circuit
SUPERPOSITION_QASM = """
OPENQASM 3.0;
qubit[2] q;
h q[0];
h q[1];
"""


async def submit_and_wait(client, qasm, name, shots=1000):
    """Submit a circuit and wait for results."""
    print(f"\n[{name}] Submitting...")
    job_id = await client.submit_qasm(qasm, "simulator", shots=shots)
    print(f"[{name}] Job ID: {job_id}")

    # Wait with progress callback
    def progress(job):
        print(f"[{name}] Status: {job.state.name}")

    result = await client.wait_for_job(
        job_id, poll_interval=0.5, progress_callback=progress
    )

    print(f"[{name}] Results:")
    for bitstring, count in sorted(result.counts.items(), key=lambda x: -x[1])[:3]:
        prob = count / result.shots
        print(f"  {bitstring}: {count} ({prob:.3f})")

    return result


async def main():
    """Run async example with concurrent jobs."""
    # Create async client with connection pooling
    async with AsyncArvakClient("localhost:50051", pool_size=5) as client:
        # List backends
        print("Available backends:")
        backends = await client.list_backends()
        for backend in backends:
            print(f"  - {backend.backend_id}: {backend.name}")

        # Submit multiple jobs concurrently
        print("\n" + "=" * 60)
        print("Submitting 3 jobs concurrently...")
        print("=" * 60)

        # Create tasks for concurrent execution
        tasks = [
            submit_and_wait(client, BELL_STATE_QASM, "Bell State", shots=1000),
            submit_and_wait(client, GHZ_STATE_QASM, "GHZ State", shots=1000),
            submit_and_wait(client, SUPERPOSITION_QASM, "Superposition", shots=1000),
        ]

        # Wait for all jobs to complete
        results = await asyncio.gather(*tasks)

        print("\n" + "=" * 60)
        print(f"All {len(results)} jobs completed successfully!")
        print("=" * 60)

        # Summary
        for i, result in enumerate(results, 1):
            most_freq = result.most_frequent()
            if most_freq:
                print(f"Job {i}: Most frequent = {most_freq[0]} ({most_freq[1]:.3f})")


async def benchmark_concurrent_submissions():
    """Benchmark concurrent job submissions."""
    import time

    circuits = [(BELL_STATE_QASM, 100) for _ in range(20)]

    async with AsyncArvakClient("localhost:50051") as client:
        print("\nBenchmark: Submitting 20 jobs concurrently...")
        start = time.time()

        # Submit all jobs concurrently
        job_ids = await client.submit_batch(circuits, "simulator")

        elapsed = time.time() - start
        print(f"Submitted {len(job_ids)} jobs in {elapsed:.3f}s")
        print(f"Rate: {len(job_ids)/elapsed:.1f} jobs/second")

        # Wait for all to complete
        print("\nWaiting for completion...")
        tasks = [client.wait_for_job(job_id) for job_id in job_ids]
        results = await asyncio.gather(*tasks)

        total_elapsed = time.time() - start
        print(f"All jobs completed in {total_elapsed:.3f}s")
        print(f"Throughput: {len(results)/total_elapsed:.1f} jobs/second")


if __name__ == "__main__":
    # Run main example
    asyncio.run(main())

    # Run benchmark
    asyncio.run(benchmark_concurrent_submissions())
