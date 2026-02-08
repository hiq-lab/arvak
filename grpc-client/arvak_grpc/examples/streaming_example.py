"""Examples demonstrating streaming gRPC APIs.

This example shows how to use the three streaming RPCs:
1. WatchJob - server streaming for real-time job status updates
2. StreamResults - server streaming for large result sets
3. SubmitBatchStream - bidirectional streaming for batch submission
"""

import asyncio
from arvak_grpc import AsyncArvakClient, JobState

# Example OpenQASM circuits
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
cx q[0], q[2];
"""

RANDOM_CIRCUIT = """
OPENQASM 3.0;
qubit[3] q;
h q[0];
h q[1];
h q[2];
cx q[0], q[1];
cx q[1], q[2];
"""


async def example_watch_job():
    """Example 1: Watch job status updates in real-time."""
    print("=" * 60)
    print("Example 1: WatchJob - Real-time Status Updates")
    print("=" * 60)

    async with AsyncArvakClient("localhost:50051") as client:
        # Submit a job
        print("\n1. Submitting job...")
        job_id = await client.submit_qasm(BELL_STATE, "simulator", shots=10000)
        print(f"   Job submitted: {job_id}")

        # Watch the job status in real-time
        print("\n2. Watching job status updates...")
        async for state, timestamp, error_msg in client.watch_job(job_id):
            print(f"   [{timestamp.strftime('%H:%M:%S')}] State: {state.name}")
            if error_msg:
                print(f"   Error: {error_msg}")
            if state in (JobState.COMPLETED, JobState.FAILED, JobState.CANCELED):
                break

        # Get final result
        if state == JobState.COMPLETED:
            print("\n3. Retrieving result...")
            result = await client.get_job_result(job_id)
            print(f"   Counts: {result.counts}")
            print(f"   Total shots: {result.shots}")


async def example_stream_results():
    """Example 2: Stream large result sets in chunks."""
    print("\n" + "=" * 60)
    print("Example 2: StreamResults - Chunked Result Streaming")
    print("=" * 60)

    async with AsyncArvakClient("localhost:50051") as client:
        # Submit a job with many shots to get large results
        print("\n1. Submitting job with many shots...")
        job_id = await client.submit_qasm(RANDOM_CIRCUIT, "simulator", shots=50000)
        print(f"   Job submitted: {job_id}")

        # Wait for completion
        print("\n2. Waiting for job to complete...")
        result = await client.wait_for_job(job_id)
        print(f"   Job completed with {len(result.counts)} unique outcomes")

        # Stream results in chunks
        print("\n3. Streaming results in chunks (chunk_size=1000)...")
        all_counts = {}
        total_outcomes = 0

        async for counts, is_final, idx, total in client.stream_results(
            job_id, chunk_size=1000
        ):
            all_counts.update(counts)
            total_outcomes += len(counts)
            print(
                f"   Chunk {idx+1}/{total}: {len(counts)} outcomes (cumulative: {total_outcomes})"
            )
            if is_final:
                print(f"   ✓ Streaming complete!")
                break

        print(f"\n4. Verification:")
        print(f"   Total unique outcomes: {len(all_counts)}")
        print(f"   Total counts: {sum(all_counts.values())}")


async def example_submit_batch_stream():
    """Example 3: Submit batch jobs with streaming feedback."""
    print("\n" + "=" * 60)
    print("Example 3: SubmitBatchStream - Bidirectional Streaming")
    print("=" * 60)

    async with AsyncArvakClient("localhost:50051") as client:
        # Create a generator that yields circuits
        circuits = [
            (BELL_STATE, "bell-circuit"),
            (GHZ_STATE, "ghz-circuit"),
            (RANDOM_CIRCUIT, "random-circuit"),
        ]

        async def circuit_generator():
            """Generator that yields circuit submission requests."""
            for i, (qasm, label) in enumerate(circuits):
                print(f"\n   Sending: {label}")
                yield (qasm, "simulator", 5000, "qasm3", f"req-{i}-{label}")
                # Small delay to simulate realistic submission timing
                await asyncio.sleep(0.1)

        print("\n1. Submitting batch with streaming feedback...")
        submitted_jobs = {}
        completed_jobs = {}
        failed_jobs = {}

        # Process streaming results
        async for job_id, req_id, result_type, result_data in client.submit_batch_stream(
            circuit_generator()
        ):
            if result_type == "submitted":
                submitted_jobs[req_id] = job_id
                print(f"   ✓ {req_id}: Job {job_id} submitted")

            elif result_type == "completed":
                completed_jobs[req_id] = result_data
                top_outcomes = sorted(
                    result_data.counts.items(), key=lambda x: x[1], reverse=True
                )[:3]
                print(f"   ✓ {req_id}: Job {job_id} completed")
                print(f"      Top outcomes: {dict(top_outcomes)}")

            elif result_type == "error":
                failed_jobs[req_id] = result_data
                print(f"   ✗ {req_id}: Job {job_id} failed - {result_data}")

        print(f"\n2. Batch Summary:")
        print(f"   Total submitted: {len(submitted_jobs)}")
        print(f"   Completed: {len(completed_jobs)}")
        print(f"   Failed: {len(failed_jobs)}")


async def example_advanced_streaming():
    """Example 4: Advanced streaming - combine multiple streams."""
    print("\n" + "=" * 60)
    print("Example 4: Advanced - Multiple Concurrent Streams")
    print("=" * 60)

    async with AsyncArvakClient("localhost:50051") as client:
        # Submit multiple jobs
        print("\n1. Submitting multiple jobs...")
        job_ids = []
        for i, qasm in enumerate([BELL_STATE, GHZ_STATE, RANDOM_CIRCUIT]):
            job_id = await client.submit_qasm(qasm, "simulator", shots=10000)
            job_ids.append(job_id)
            print(f"   Job {i+1}: {job_id}")

        # Watch all jobs concurrently
        print("\n2. Watching all jobs concurrently...")

        async def watch_single_job(job_id, index):
            """Watch a single job and report updates."""
            async for state, timestamp, _ in client.watch_job(job_id):
                print(
                    f"   Job {index+1}: {state.name} at {timestamp.strftime('%H:%M:%S')}"
                )
                if state in (JobState.COMPLETED, JobState.FAILED, JobState.CANCELED):
                    return state

        # Wait for all jobs to complete concurrently
        states = await asyncio.gather(
            *[watch_single_job(job_id, i) for i, job_id in enumerate(job_ids)]
        )

        print("\n3. All jobs completed!")
        print(f"   Final states: {[s.name for s in states]}")


async def main():
    """Run all streaming examples."""
    print("\n" + "=" * 60)
    print("Arvak gRPC Streaming Examples")
    print("=" * 60)
    print("\nMake sure the Arvak gRPC server is running on localhost:50051")
    print("Run: cargo run --bin arvak-grpc-server\n")

    try:
        # Run all examples
        await example_watch_job()
        await example_stream_results()
        await example_submit_batch_stream()
        await example_advanced_streaming()

        print("\n" + "=" * 60)
        print("All examples completed successfully!")
        print("=" * 60)

    except Exception as e:
        print(f"\n✗ Error: {e}")
        print("\nMake sure the Arvak gRPC server is running:")
        print("  cd crates/arvak-grpc")
        print("  cargo run --bin arvak-grpc-server")


if __name__ == "__main__":
    asyncio.run(main())
