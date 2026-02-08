#!/usr/bin/env python3
"""Example: Submit multiple jobs in a batch."""

from arvak_grpc import ArvakClient

# Different circuits to test
CIRCUITS = [
    # Bell state
    """
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
""",
    # GHZ state
    """
OPENQASM 3.0;
qubit[3] q;
h q[0];
cx q[0], q[1];
cx q[1], q[2];
""",
    # Superposition
    """
OPENQASM 3.0;
qubit[2] q;
h q[0];
h q[1];
""",
]


def main():
    client = ArvakClient("localhost:50051")

    try:
        # Submit batch
        print("Submitting batch of 3 circuits...")
        job_ids = client.submit_batch(
            [(circuit, 1000) for circuit in CIRCUITS],
            backend_id="simulator",
            format="qasm3",
        )
        print(f"Submitted {len(job_ids)} jobs")

        # Wait for all jobs
        for i, job_id in enumerate(job_ids, 1):
            print(f"\nWaiting for job {i}/{len(job_ids)}: {job_id}")
            result = client.wait_for_job(job_id, max_wait=30)

            print(f"Results for job {i}:")
            for bitstring, count in sorted(result.counts.items(), key=lambda x: -x[1])[:5]:
                prob = count / result.shots
                print(f"  {bitstring}: {count} ({prob:.3f})")

        print("\nAll jobs completed successfully!")

    finally:
        client.close()


if __name__ == "__main__":
    main()
