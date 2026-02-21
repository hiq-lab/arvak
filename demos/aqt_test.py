#!/usr/bin/env python3
"""AQT offline noiseless simulator Bell-state test.

Runs a 2-qubit Bell-state circuit on the AQT offline noiseless simulator
and verifies the ~50/50 distribution of |00⟩ and |11⟩ outcomes.

No AQT account is needed — the offline simulator accepts any token value
(even empty string).

Prerequisites
-------------
Install the Python package:

    cd crates/arvak-python && maturin develop --release

Usage
-----
    python demos/aqt_test.py

Optional environment variables
-------------------------------
    AQT_TOKEN        — AQT Bearer token (default: empty, fine for offline sim)
    AQT_WORKSPACE    — AQT workspace (default: default)
    AQT_RESOURCE     — AQT resource (default: offline_simulator_no_noise)
    AQT_SHOTS        — shot count (default: 200)
"""

import os
import sys

# ---------------------------------------------------------------------------
# Configuration via environment variables
# ---------------------------------------------------------------------------

WORKSPACE = os.environ.get("AQT_WORKSPACE", "default")
RESOURCE = os.environ.get("AQT_RESOURCE", "offline_simulator_no_noise")
SHOTS = int(os.environ.get("AQT_SHOTS", "200"))

# ---------------------------------------------------------------------------
# Build Bell-state circuit using Qiskit
# ---------------------------------------------------------------------------

from qiskit import QuantumCircuit  # noqa: E402

qc = QuantumCircuit(2, 2)
qc.h(0)
qc.cx(0, 1)
qc.measure([0, 1], [0, 1])

print(f"Bell-state circuit on AQT {WORKSPACE}/{RESOURCE}")
print(f"Shots: {SHOTS}")
print()
print(qc.draw(output="text"))
print()

# ---------------------------------------------------------------------------
# Connect to AQT and submit
# ---------------------------------------------------------------------------

from arvak.integrations.qiskit.backend import ArvakAQTBackend  # noqa: E402

backend = ArvakAQTBackend(provider=None, workspace=WORKSPACE, resource=RESOURCE)
print(f"Backend: {backend.name}")
avail = backend.availability()
print(f"Availability: {'online' if avail.online else 'OFFLINE'} — {avail.status_message}")
print()

if not avail.online:
    print(f"WARNING: {RESOURCE} reports offline. Cannot proceed.")
    sys.exit(1)

# ---------------------------------------------------------------------------
# Validate and submit
# ---------------------------------------------------------------------------

val = backend.validate(qc, shots=SHOTS)
if not val.valid:
    print("Validation failed:\n  " + "\n  ".join(val.errors))
    sys.exit(1)

print(f"Submitting Bell-state circuit to AQT {RESOURCE}...")
job = backend.run(qc, shots=SHOTS)
print(f"Job ID: {job.job_id()}")
print()

# ---------------------------------------------------------------------------
# Poll and display results
# ---------------------------------------------------------------------------

print("Waiting for results (offline simulator typically completes in <10s)...")
result = job.result(timeout=120, poll_interval=2)
counts = result.get_counts()

print()
print("Results:")
total = sum(counts.values())
for bitstring, count in sorted(counts.items(), key=lambda x: -x[1]):
    pct = count / total * 100 if total > 0 else 0
    bar = "\u2588" * int(pct / 2)
    print(f"  |{bitstring}\u27e9: {count:>5} ({pct:>5.1f}%)  {bar}")

print()

# Verify Bell distribution: |00⟩ and |11⟩ each ≈50%
count_00 = counts.get("00", 0)
count_11 = counts.get("11", 0)
bell_frac = (count_00 + count_11) / total if total > 0 else 0
print(f"Bell fraction |00\u27e9+|11\u27e9: {bell_frac:.1%}  (expected ~100% for noiseless sim)")

if bell_frac < 0.90:
    print("WARNING: Bell fraction unexpectedly low. Check circuit or API.")
    sys.exit(1)
else:
    print("PASS: Results consistent with Bell state.")
