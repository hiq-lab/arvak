#!/usr/bin/env python3
"""Example: Submit a simple quantum circuit and retrieve results."""

from arvak_grpc import ArvakClient

# Bell state circuit
BELL_STATE_QASM = """
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
"""


def main():
    # Create client
    client = ArvakClient("localhost:50051")

    try:
        # List available backends
        print("Available backends:")
        backends = client.list_backends()
        for backend in backends:
            print(f"  - {backend.backend_id}: {backend.name} ({backend.max_qubits} qubits)")

        # Submit job
        print("\nSubmitting Bell state circuit...")
        job_id = client.submit_qasm(BELL_STATE_QASM, "simulator", shots=1000)
        print(f"Job submitted: {job_id}")

        # Wait for completion
        print("Waiting for job to complete...")
        result = client.wait_for_job(job_id, max_wait=30)

        # Display results
        print(f"\nResults (total shots: {result.shots}):")
        for bitstring, count in sorted(result.counts.items()):
            prob = count / result.shots
            print(f"  {bitstring}: {count} ({prob:.3f})")

        # Most frequent result
        most_freq = result.most_frequent()
        if most_freq:
            print(f"\nMost frequent: {most_freq[0]} ({most_freq[1]:.3f})")

    finally:
        client.close()


if __name__ == "__main__":
    main()
