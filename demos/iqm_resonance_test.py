"""IQM Resonance hardware integration test for Arvak.

Compiles and runs a Bell/GHZ circuit on IQM hardware via IQM Resonance.
Arvak handles qubit routing and gate compilation; qiskit-on-iqm handles
submission to the IQM JSON API.

Usage:
    export IQM_TOKEN=<your resonance token>
    .venv/bin/python3 demos/iqm_resonance_test.py

    # Override defaults:
    export IQM_COMPUTER=sirius   # sirius (default), garnet, emerald, crystal
    export IQM_SHOTS=1024

IQM Resonance: https://resonance.meetiqm.com
Starter tier: 30 free credits/month
"""

import os
import sys


def test_compilation():
    """Test Arvak compilation for IQM target (no credentials needed)."""
    print("=" * 60)
    print("Arvak IQM Compilation Test")
    print("=" * 60)

    import arvak
    from qiskit import QuantumCircuit
    from qiskit.qasm3 import dumps

    # Build Bell circuit via Qiskit
    qc = QuantumCircuit(2, 2)
    qc.h(0)
    qc.cx(0, 1)
    qc.measure([0, 1], [0, 1])

    print("\n--- Input circuit ---")
    print(qc.draw())

    # Compile with Arvak for Sirius (16-qubit star topology, PRX + CZ basis)
    qasm_str = dumps(qc)
    arvak_circuit = arvak.from_qasm(qasm_str)
    coupling = arvak.CouplingMap.star(16)
    basis = arvak.BasisGates.iqm()

    compiled = arvak.compile(
        arvak_circuit,
        coupling_map=coupling,
        basis_gates=basis,
        optimization_level=1,
    )

    qasm_out = arvak.to_qasm(compiled)
    print("\n--- Arvak-compiled circuit (PRX + CZ basis) ---")
    print(qasm_out)
    print("Compilation successful!")
    return qasm_out


def test_hardware(shots: int = 1024, computer: str = "sirius"):
    """Run a Bell circuit on IQM Resonance hardware."""
    print("\n" + "=" * 60)
    print(f"IQM Resonance Hardware Test — {computer}")
    print("=" * 60)

    token = os.environ.get("IQM_TOKEN")
    if not token:
        raise RuntimeError(
            "IQM_TOKEN environment variable not set.\n"
            "Get your token from https://resonance.meetiqm.com (account drawer)."
        )

    from iqm.qiskit_iqm import IQMProvider
    from qiskit import QuantumCircuit, transpile

    # Connect to IQM Resonance
    print(f"\nConnecting to IQM Resonance ({computer})...")
    # IQM_TOKEN env var is picked up automatically by iqm-client
    provider = IQMProvider(
        "https://resonance.meetiqm.com/",
        quantum_computer=computer,
    )
    backend = provider.get_backend()
    print(f"Backend: {backend.name}")
    print(f"Qubits:  {backend.num_qubits}")

    # Build Bell circuit
    qc = QuantumCircuit(2, 2)
    qc.h(0)
    qc.cx(0, 1)
    qc.measure([0, 1], [0, 1])

    print(f"\nTranspiling Bell circuit for {backend.name}...")
    qc_transpiled = transpile(qc, backend=backend, optimization_level=1)
    print(qc_transpiled.draw())

    print(f"\nSubmitting ({shots} shots)...")
    job = backend.run(qc_transpiled, shots=shots)
    print(f"Job ID: {job.job_id()}")

    print("Waiting for results...")
    result = job.result()
    counts = result.get_counts()

    print(f"\nResults:")
    total = sum(counts.values())
    for bitstring, count in sorted(counts.items()):
        pct = 100.0 * count / total if total else 0
        print(f"  |{bitstring}⟩: {count:5d}  ({pct:.1f}%)")
    print(f"  Total shots: {total}")

    # Bell state sanity check
    p00 = counts.get("00", 0) / total if total else 0
    p11 = counts.get("11", 0) / total if total else 0
    if abs(p00 - 0.5) < 0.15 and abs(p11 - 0.5) < 0.15:
        print("\nPASS: Bell state distribution looks correct.")
    else:
        print(f"\nWARN: Unexpected distribution (|00⟩={p00:.1%}, |11⟩={p11:.1%})")

    return counts


def main():
    # Compilation test — always runs (no credentials needed)
    test_compilation()

    # Hardware test — only if IQM_TOKEN is set
    token = os.environ.get("IQM_TOKEN")
    if not token:
        print("\n" + "-" * 60)
        print("Hardware test skipped: IQM_TOKEN not set.")
        print("  export IQM_TOKEN=<your resonance token>")
        print("  export IQM_COMPUTER=sirius  # or garnet, emerald, crystal")
        print("  export IQM_SHOTS=1024")
        print("-" * 60)
        return

    computer = os.environ.get("IQM_COMPUTER", "sirius")
    shots = int(os.environ.get("IQM_SHOTS", "1024"))

    test_hardware(shots=shots, computer=computer)


if __name__ == "__main__":
    main()
