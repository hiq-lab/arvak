"""Scaleway/IQM hardware integration test for Arvak.

Tests the compilation pipeline (Qiskit -> Arvak IR -> IQM basis gates)
and optionally submits to Scaleway QaaS if credentials are available.

Usage:
    .venv/bin/python3 demos/scaleway_test.py

Environment variables (for hardware submission):
    SCALEWAY_SECRET_KEY   - Scaleway API secret key
    SCALEWAY_PROJECT_ID   - Scaleway project ID
    SCALEWAY_SESSION_ID   - Active QaaS session ID (create via console or API)
    SCALEWAY_PLATFORM     - Optional: QPU-GARNET-20PQ (default), QPU-SIRIUS-24PQ, QPU-EMERALD-54PQ

How to get a session ID:
    1. Log in to console.scaleway.com
    2. Go to Quantum Computing > Sessions
    3. Create a new session for QPU-GARNET-20PQ (or desired platform)
    4. Copy the session ID and export SCALEWAY_SESSION_ID=<id>

    Or via API:
        curl -s -X POST https://api.scaleway.com/qaas/v1alpha1/sessions \\
          -H "X-Auth-Token: $SCALEWAY_SECRET_KEY" \\
          -H "Content-Type: application/json" \\
          -d '{"platform_id":"<platform-id>","project_id":"$SCALEWAY_PROJECT_ID","name":"arvak-test"}'
"""

import json
import os
import sys

import arvak


def build_bell_circuit_qasm() -> str:
    """Build a 2-qubit Bell state circuit as QASM3."""
    return """OPENQASM 3.0;
qubit[2] q;
bit[2] c;
h q[0];
cx q[0], q[1];
c[0] = measure q[0];
c[1] = measure q[1];
"""


def test_compilation():
    """Test Arvak compilation for IQM target (no credentials needed)."""
    print("=" * 60)
    print("Arvak Scaleway/IQM Compilation Test")
    print("=" * 60)

    # Parse Bell circuit
    qasm_input = build_bell_circuit_qasm()
    print("\n--- Input Circuit (QASM3) ---")
    print(qasm_input)

    circuit = arvak.from_qasm(qasm_input)

    # Compile for IQM: PRX + CZ basis, star topology with 20 qubits
    coupling_map = arvak.CouplingMap.star(20)
    basis_gates = arvak.BasisGates.iqm()

    compiled = arvak.compile(
        circuit,
        coupling_map=coupling_map,
        basis_gates=basis_gates,
        optimization_level=1,
    )

    # Emit compiled QASM3
    qasm_out = arvak.to_qasm(compiled)
    qasm_out = qasm_out.replace(
        "OPENQASM 3.0;",
        'OPENQASM 3.0;\ninclude "stdgates.inc";',
        1,
    )

    print("--- Compiled Circuit (IQM basis: PRX + CZ) ---")
    print(qasm_out)
    print("Compilation successful!")
    return qasm_out


def check_session_status(secret_key: str, session_id: str) -> dict:
    """Check the status of a Scaleway QaaS session."""
    import requests

    resp = requests.get(
        f"https://api.scaleway.com/qaas/v1alpha1/sessions/{session_id}",
        headers={
            "X-Auth-Token": secret_key,
            "Content-Type": "application/json",
        },
        timeout=30,
    )
    if not resp.ok:
        raise RuntimeError(
            f"Failed to fetch session {session_id}: {resp.status_code} {resp.text}"
        )
    return resp.json()


def list_platforms(secret_key: str, project_id: str) -> list:
    """List available Scaleway QaaS platforms."""
    import requests

    resp = requests.get(
        "https://api.scaleway.com/qaas/v1alpha1/platforms",
        headers={
            "X-Auth-Token": secret_key,
            "Content-Type": "application/json",
        },
        params={"project_id": project_id},
        timeout=30,
    )
    if not resp.ok:
        raise RuntimeError(
            f"Failed to list platforms: {resp.status_code} {resp.text}"
        )
    body = resp.json()
    # API returns {"platforms": [...]} or a list directly
    return body.get("platforms", body) if isinstance(body, dict) else body


def create_session(secret_key: str, project_id: str, platform_id: str,
                   name: str = "arvak-test") -> dict:
    """Create a new Scaleway QaaS session and return the session dict."""
    import requests

    resp = requests.post(
        "https://api.scaleway.com/qaas/v1alpha1/sessions",
        headers={
            "X-Auth-Token": secret_key,
            "Content-Type": "application/json",
        },
        json={
            "platform_id": platform_id,
            "project_id": project_id,
            "name": name,
        },
        timeout=30,
    )
    if not resp.ok:
        raise RuntimeError(
            f"Failed to create session: {resp.status_code} {resp.text}"
        )
    return resp.json()


def terminate_session(secret_key: str, session_id: str) -> None:
    """Terminate (close) a Scaleway QaaS session."""
    import requests

    resp = requests.post(
        f"https://api.scaleway.com/qaas/v1alpha1/sessions/{session_id}/terminate",
        headers={
            "X-Auth-Token": secret_key,
            "Content-Type": "application/json",
        },
        timeout=30,
    )
    if not resp.ok:
        print(f"  Warning: could not terminate session {session_id}: {resp.text}")


def test_hardware_submission(shots: int = 1024):
    """Submit a Bell circuit to Scaleway/IQM hardware."""
    print("\n" + "=" * 60)
    print("Arvak Scaleway/IQM Hardware Submission")
    print("=" * 60)

    secret_key = os.environ["SCALEWAY_SECRET_KEY"]
    project_id = os.environ["SCALEWAY_PROJECT_ID"]
    session_id = os.environ["SCALEWAY_SESSION_ID"]
    platform = os.environ.get("SCALEWAY_PLATFORM", "QPU-GARNET-20PQ")

    # Wait for session to be ready
    print(f"\nWaiting for session {session_id} to be ready...")
    import time as _time
    for attempt in range(24):  # up to 2 minutes
        try:
            session_info = check_session_status(secret_key, session_id)
        except RuntimeError as e:
            print(f"  WARNING: Could not check session: {e}")
            break
        status = session_info.get("status", "unknown")
        platform_from_session = session_info.get("platform_id", "unknown")
        print(f"  Status: {status}  (platform: {platform_from_session}, name: {session_info.get('name', '?')})")
        if status in ("running", "started", "ready"):
            break
        if status in ("stopped", "terminated", "error", "failed"):
            raise RuntimeError(f"Session {session_id} is {status} — cannot submit jobs.")
        _time.sleep(5)

    from arvak.integrations.qiskit import ArvakProvider

    provider = ArvakProvider()

    platform_to_backend = {
        "QPU-GARNET-20PQ": "scaleway_garnet",
        "QPU-SIRIUS-24PQ": "scaleway_sirius",
        "QPU-EMERALD-54PQ": "scaleway_emerald",
    }
    backend_name = platform_to_backend.get(platform, "scaleway_garnet")

    print(f"\nTarget backend: {backend_name} ({platform})")

    backend = provider.get_backend(backend_name)
    print(f"Backend:        {backend}")

    # Build Bell circuit via Qiskit
    from qiskit import QuantumCircuit

    qc = QuantumCircuit(2, 2)
    qc.h(0)
    qc.cx(0, 1)
    qc.measure([0, 1], [0, 1])

    print(f"\nSubmitting Bell circuit ({shots} shots)...")
    job = backend.run(qc, shots=shots)
    print(f"Job ID: {job.job_id()}")

    print("Waiting for results...")
    result = job.result(timeout=600, poll_interval=5)

    counts = result.get_counts()
    print(f"\nResults:")
    if not counts:
        print("  (no results returned)")
    else:
        total = sum(counts.values())
        for bitstring, count in sorted(counts.items()):
            pct = 100.0 * count / total if total else 0
            print(f"  |{bitstring}⟩: {count:5d}  ({pct:.1f}%)")
        print(f"  Total shots: {total}")

        # Sanity check: Bell state should be ~50% |00⟩ and ~50% |11⟩
        p00 = counts.get("00", 0) / total if total else 0
        p11 = counts.get("11", 0) / total if total else 0
        if abs(p00 - 0.5) < 0.1 and abs(p11 - 0.5) < 0.1:
            print("\nPASS: Bell state distribution looks correct.")
        else:
            print(f"\nWARN: Unexpected distribution (|00⟩={p00:.1%}, |11⟩={p11:.1%})")

    return counts


def resolve_session_id(secret_key: str, project_id: str, platform_name: str) -> tuple:
    """Return (session_id, created) — creates a session if none is set."""
    session_id = os.environ.get("SCALEWAY_SESSION_ID")
    if session_id:
        return session_id, False

    print(f"\nNo SCALEWAY_SESSION_ID set. Creating a new session for {platform_name}...")

    # List platforms to find the correct platform_id
    try:
        platforms = list_platforms(secret_key, project_id)
    except RuntimeError as e:
        raise RuntimeError(
            f"Cannot list platforms (check credentials): {e}"
        ) from e

    print(f"  Available platforms:")
    platform_id = None
    for p in platforms:
        name = p.get("name", "?")
        pid = p.get("id", "?")
        status = p.get("availability", p.get("status", "?"))
        print(f"    {name:30s}  id={pid}  status={status}")
        if name == platform_name:
            platform_id = pid

    if not platform_id:
        raise RuntimeError(
            f"Platform '{platform_name}' not found in available platforms. "
            f"Check SCALEWAY_PLATFORM env var."
        )

    session = create_session(
        secret_key, project_id, platform_id, name="arvak-test"
    )
    sid = session["id"]
    print(f"  Session created: {sid}")
    print(f"  (To reuse this session, export SCALEWAY_SESSION_ID={sid})")
    return sid, True


def main():
    # Always test compilation (no credentials needed)
    test_compilation()

    # Test hardware submission only if credentials are set
    secret_key = os.environ.get("SCALEWAY_SECRET_KEY")
    project_id = os.environ.get("SCALEWAY_PROJECT_ID")

    if not secret_key or not project_id:
        missing = []
        if not secret_key:
            missing.append("SCALEWAY_SECRET_KEY")
        if not project_id:
            missing.append("SCALEWAY_PROJECT_ID")

        print("\n" + "-" * 60)
        print("Hardware test skipped: missing credentials:")
        for var in missing:
            print(f"  export {var}=<value>")
        print()
        print("Optional (session auto-created if not set):")
        print("  export SCALEWAY_SESSION_ID=<id>")
        print("  export SCALEWAY_PLATFORM=QPU-GARNET-20PQ  # default")
        print("  export SCALEWAY_SHOTS=1024                 # default")
        print("-" * 60)
        return

    platform = os.environ.get("SCALEWAY_PLATFORM", "QPU-GARNET-20PQ")
    shots = int(os.environ.get("SCALEWAY_SHOTS", "1024"))

    created_session = False
    session_id = None
    try:
        session_id, created_session = resolve_session_id(
            secret_key, project_id, platform
        )
        os.environ["SCALEWAY_SESSION_ID"] = session_id
        test_hardware_submission(shots=shots)
    except Exception:
        raise
    finally:
        if created_session and session_id:
            print(f"\nTerminating auto-created session {session_id}...")
            terminate_session(secret_key, session_id)
            print("Session terminated.")


if __name__ == "__main__":
    main()
