#!/usr/bin/env python3
"""Quantinuum H2-1LE (noiseless emulator) Bell-state test.

Runs a 2-qubit Bell-state circuit on the Quantinuum H2-1LE noiseless emulator
and verifies the ~50/50 distribution of |00⟩ and |11⟩ outcomes.

Prerequisites
-------------
Set the following environment variables:

    export QUANTINUUM_EMAIL="user@example.com"
    export QUANTINUUM_PASSWORD="your_password"

The H2-1LE emulator is noiseless and free — it does NOT consume Quantinuum
hardware credits.

Usage
-----
    python demos/quantinuum_test.py

Optional environment variables
-------------------------------
    QUANTINUUM_DEVICE  — target machine (default: H2-1LE)
    QUANTINUUM_SHOTS   — shot count (default: 200)
"""

import os
import sys

# ---------------------------------------------------------------------------
# Configuration via environment variables
# ---------------------------------------------------------------------------

DEVICE = os.environ.get("QUANTINUUM_DEVICE", "H2-1LE")
SHOTS = int(os.environ.get("QUANTINUUM_SHOTS", "200"))

# ---------------------------------------------------------------------------
# Credential check
# ---------------------------------------------------------------------------

if not os.environ.get("QUANTINUUM_EMAIL"):
    print("ERROR: QUANTINUUM_EMAIL environment variable not set.")
    print("       Register at https://um.qapi.quantinuum.com and set credentials.")
    sys.exit(1)

if not os.environ.get("QUANTINUUM_PASSWORD"):
    print("ERROR: QUANTINUUM_PASSWORD environment variable not set.")
    sys.exit(1)

# ---------------------------------------------------------------------------
# Build Bell-state circuit using Qiskit
# ---------------------------------------------------------------------------

from qiskit import QuantumCircuit  # noqa: E402

qc = QuantumCircuit(2, 2)
qc.h(0)
qc.cx(0, 1)
qc.measure([0, 1], [0, 1])

print(f"Bell-state circuit on Quantinuum {DEVICE}")
print(f"Shots: {SHOTS}")
print()
print(qc.draw(output="text"))
print()

# ---------------------------------------------------------------------------
# Connect to Quantinuum and submit
# ---------------------------------------------------------------------------

from arvak.integrations.qiskit.backend import ArvakProvider  # noqa: E402

provider = ArvakProvider()
backend = provider.get_backend(
    f"quantinuum_{'h2_emulator' if 'H2' in DEVICE and 'LE' not in DEVICE else 'h2'}"
    if DEVICE not in {"H2-1LE"} else "quantinuum_h2"
)

# For direct device targeting, construct the backend directly
from arvak.integrations.qiskit.backend import ArvakQuantinuumBackend  # noqa: E402

backend = ArvakQuantinuumBackend(provider=provider, device_name=DEVICE)
print(f"Backend: {backend.name}")
avail = backend.availability()
print(f"Availability: {'online' if avail.online else 'OFFLINE'} — {avail.status_message}")
print()

if not avail.online:
    print(f"WARNING: {DEVICE} reports offline status: {avail.status_message}")
    print("Continuing anyway (emulators may report 'unknown' when status endpoint is unavailable).")
    print()

# ---------------------------------------------------------------------------
# Validate and submit
# ---------------------------------------------------------------------------

val = backend.validate(qc, shots=SHOTS)
if not val.valid:
    print(f"Validation failed:\n  " + "\n  ".join(val.errors))
    sys.exit(1)

print(f"Submitting Bell-state circuit to {DEVICE}...")
job = backend.run(qc, shots=SHOTS)
print(f"Job ID: {job.job_id()}")
print()

# ---------------------------------------------------------------------------
# Poll and display results
# ---------------------------------------------------------------------------

print("Waiting for results (H2-1LE typically completes in <60s)...")
result = job.result(timeout=300, poll_interval=3)
counts = result.get_counts()

print()
print("Results:")
total = sum(counts.values())
for bitstring, count in sorted(counts.items(), key=lambda x: -x[1]):
    pct = count / total * 100 if total > 0 else 0
    bar = "█" * int(pct / 2)
    print(f"  |{bitstring}⟩: {count:>5} ({pct:>5.1f}%)  {bar}")

print()

# Verify Bell distribution: |00⟩ and |11⟩ each ≈50%
count_00 = counts.get("00", 0)
count_11 = counts.get("11", 0)
others = total - count_00 - count_11

bell_frac = (count_00 + count_11) / total if total > 0 else 0
print(f"Bell fraction |00⟩+|11⟩: {bell_frac:.1%}  (expected ~100% for noiseless emulator)")

if bell_frac < 0.90:
    print("WARNING: Bell fraction is unexpectedly low for a noiseless emulator.")
    print("         This may indicate a circuit or API issue.")
    sys.exit(1)
else:
    print("PASS: Results consistent with Bell state.")
