"""IonQ integration test — Bell state on the IonQ cloud simulator.

Usage:
    export IONQ_API_KEY="your-api-key"   # from https://cloud.ionq.com
    python demos/ionq_test.py

The free tier includes unlimited simulator runs (up to 29 qubits).
"""

import os
import sys
import time

# --- check API key -----------------------------------------------------------
if not os.environ.get("IONQ_API_KEY"):
    print("ERROR: Set IONQ_API_KEY environment variable.")
    print("Get a free API key at https://cloud.ionq.com")
    sys.exit(1)

# --- build Bell circuit -------------------------------------------------------
from qiskit import QuantumCircuit

qc = QuantumCircuit(2, 2)
qc.h(0)
qc.cx(0, 1)
qc.measure([0, 1], [0, 1])
print("Circuit:")
print(qc.draw(output="text"))

# --- submit to IonQ via Arvak ------------------------------------------------
from arvak.integrations.qiskit import ArvakProvider

provider = ArvakProvider()
backend = provider.get_backend("ionq_simulator")
print(f"\nBackend: {backend}")

# Check availability
avail = backend.availability()
print(f"Availability: online={avail.online}, status={avail.status_message}")
if not avail.online:
    print("Backend is not available. Exiting.")
    sys.exit(1)

# Validate
validation = backend.validate(qc)
print(f"Validation: valid={validation.valid}")
if not validation.valid:
    print(f"  Errors: {validation.errors}")
    sys.exit(1)

# Submit
SHOTS = 1000
print(f"\nSubmitting Bell state ({SHOTS} shots)...")
job = backend.run(qc, shots=SHOTS)
print(f"Job ID: {job.job_id()}")

# Wait for results
print("Waiting for results...")
result = job.result(timeout=120, poll_interval=2)
counts = result.get_counts()
print(f"\nResults ({sum(counts.values())} shots):")
for bitstring in sorted(counts):
    print(f"  |{bitstring}> : {counts[bitstring]}")

# Verify Bell state
bell_fraction = (counts.get("00", 0) + counts.get("11", 0)) / sum(counts.values())
print(f"\nBell fraction (|00> + |11>): {bell_fraction:.1%}")
if bell_fraction >= 0.90:
    print("PASS: Bell state verified (>= 90%)")
else:
    print(f"WARN: Bell fraction {bell_fraction:.1%} is below 90%")
