"""Gate-to-pulse calibration via empirical comparison.

Shoots the same QASM3 circuits at a gate-model backend (qBraid/IonQ)
and at Pulser (Qadence), compares measurement distributions, and
extracts the correct pulse parameters for each gate.

The output is a calibration table: gate → (area, phase, post_phase_shift)
that can be compiled into Rust rules for arvak's native compiler.

Usage:
    from arvak.integrations.pulser.calibrate import run_calibration
    rules = run_calibration(api_key="qbr_...")
"""

from __future__ import annotations

import math
import itertools
from dataclasses import dataclass, field
from typing import Any

import numpy as np


@dataclass
class PulseParams:
    """Pulse parameters for a single gate."""
    area: float
    phase: float
    post_phase_shift: float = 0.0
    detuning: float = 0.0

    def to_dict(self) -> dict[str, float]:
        return {
            "area": self.area,
            "phase": self.phase,
            "post_phase_shift": self.post_phase_shift,
            "detuning": self.detuning,
        }


@dataclass
class CalibrationResult:
    """Result of calibrating one gate."""
    gate: str
    qasm_circuit: str
    ground_truth: dict[str, int]
    best_params: PulseParams
    best_fidelity: float
    all_trials: list[dict[str, Any]] = field(default_factory=list)


def _distribution_fidelity(
    counts_a: dict[str, int], counts_b: dict[str, int]
) -> float:
    """Classical fidelity (Bhattacharyya coefficient) between two distributions."""
    all_keys = set(counts_a) | set(counts_b)
    total_a = sum(counts_a.values())
    total_b = sum(counts_b.values())

    if total_a == 0 or total_b == 0:
        return 0.0

    bc = 0.0
    for key in all_keys:
        p = counts_a.get(key, 0) / total_a
        q = counts_b.get(key, 0) / total_b
        bc += math.sqrt(p * q)

    return bc


# --- Test circuits: one per gate, designed to reveal wrong parameters ---

_SINGLE_QUBIT_TESTS = {
    "h": {
        "qasm": (
            "OPENQASM 3.0;\n"
            "qubit[1] q;\nbit[1] c;\n"
            "h q[0];\n"
            "c[0] = measure q[0];\n"
        ),
        "expected_approx": {"0": 500, "1": 500},
    },
    "x": {
        "qasm": (
            "OPENQASM 3.0;\n"
            "qubit[1] q;\nbit[1] c;\n"
            "x q[0];\n"
            "c[0] = measure q[0];\n"
        ),
        "expected_approx": {"1": 1000},
    },
    "h_h": {
        "qasm": (
            "OPENQASM 3.0;\n"
            "qubit[1] q;\nbit[1] c;\n"
            "h q[0];\nh q[0];\n"
            "c[0] = measure q[0];\n"
        ),
        "expected_approx": {"0": 1000},
    },
    "x_x": {
        "qasm": (
            "OPENQASM 3.0;\n"
            "qubit[1] q;\nbit[1] c;\n"
            "x q[0];\nx q[0];\n"
            "c[0] = measure q[0];\n"
        ),
        "expected_approx": {"0": 1000},
    },
    "h_x_h": {
        "qasm": (
            "OPENQASM 3.0;\n"
            "qubit[1] q;\nbit[1] c;\n"
            "h q[0];\nx q[0];\nh q[0];\n"
            "c[0] = measure q[0];\n"
        ),
        "expected_approx": {"0": 500, "1": 500},
    },
}

_TWO_QUBIT_TESTS = {
    "bell_hcx": {
        "qasm": (
            "OPENQASM 3.0;\n"
            "qubit[2] q;\nbit[2] c;\n"
            "h q[0];\ncx q[0], q[1];\n"
            "c[0] = measure q[0];\nc[1] = measure q[1];\n"
        ),
        "expected_approx": {"00": 500, "11": 500},
    },
    "cx_only": {
        "qasm": (
            "OPENQASM 3.0;\n"
            "qubit[2] q;\nbit[2] c;\n"
            "x q[0];\ncx q[0], q[1];\n"
            "c[0] = measure q[0];\nc[1] = measure q[1];\n"
        ),
        "expected_approx": {"11": 1000},
    },
    "cx_identity": {
        "qasm": (
            "OPENQASM 3.0;\n"
            "qubit[2] q;\nbit[2] c;\n"
            "cx q[0], q[1];\n"
            "c[0] = measure q[0];\nc[1] = measure q[1];\n"
        ),
        "expected_approx": {"00": 1000},
    },
}


def get_ground_truth_qbraid(
    qasm_circuits: dict[str, str],
    api_key: str,
    shots: int = 2000,
    device_id: str = "azure:ionq:sim:simulator",
) -> dict[str, dict[str, int]]:
    """Get ground truth results from a gate-model simulator via qBraid.

    Args:
        qasm_circuits: {name: qasm3_string}
        api_key: qBraid API key
        shots: number of shots per circuit
        device_id: qBraid device to use as ground truth

    Returns:
        {name: {bitstring: count}}
    """
    import warnings
    warnings.filterwarnings("ignore", category=RuntimeWarning)

    from qbraid.runtime import QbraidProvider

    provider = QbraidProvider(api_key=api_key)
    device = provider.get_device(device_id)

    results = {}
    for name, qasm in qasm_circuits.items():
        job = device.run(qasm, shots=shots)
        result = job.result()
        counts = result.data.get_counts()
        results[name] = dict(counts)
        print(f"  ground truth [{name}]: {counts}")

    return results


def get_pulser_result(
    n_qubits: int,
    gate_ops: list[tuple[str, list[int]]],
    pulse_rules: dict[str, PulseParams],
    entangle_params: dict[str, float],
    shots: int = 2000,
    spacing: float = 8.0,
) -> dict[str, int]:
    """Run a circuit through Pulser local emulator with given pulse parameters.

    Args:
        n_qubits: number of qubits
        gate_ops: [(gate_name, [qubit_indices]), ...]
        pulse_rules: {gate_name: PulseParams}
        entangle_params: parameters for the entangling operation
        shots: number of samples
        spacing: atom spacing in micrometers

    Returns:
        {bitstring: count}
    """
    from pulser import DigitalAnalogDevice, Register, Sequence
    from pulser.waveforms import BlackmanWaveform
    from pulser.pulse import Pulse
    from pulser_simulation import QutipEmulator

    atoms = {f"q{i}": (i * spacing, 0.0) for i in range(n_qubits)}
    reg = Register(atoms)
    seq = Sequence(reg, DigitalAnalogDevice)

    raman_declared = False
    rydberg_declared = False
    current_target = None

    for gate_name, qubits in gate_ops:
        if gate_name in pulse_rules:
            params = pulse_rules[gate_name]

            if not raman_declared:
                seq.declare_channel(
                    "raman", "raman_local", initial_target=f"q{qubits[0]}"
                )
                raman_declared = True
                current_target = qubits[0]
            elif current_target != qubits[0]:
                seq.target(f"q{qubits[0]}", "raman")
                current_target = qubits[0]

            duration = _safe_duration(params.area, seq.declared_channels["raman"])
            wf = BlackmanWaveform(duration, params.area)
            seq.add(
                Pulse.ConstantDetuning(wf, detuning=params.detuning, phase=params.phase),
                "raman",
            )
            if params.post_phase_shift != 0.0:
                seq.phase_shift(params.post_phase_shift, f"q{qubits[0]}")

        elif gate_name in ("cx", "cz", "cnot"):
            if not rydberg_declared:
                seq.declare_channel("rydberg", "rydberg_global")
                rydberg_declared = True

            area = entangle_params.get("area", math.pi)
            phase = entangle_params.get("phase", 0.0)
            detuning = entangle_params.get("detuning", 0.0)
            duration = _safe_duration(area, seq.declared_channels["rydberg"])
            wf = BlackmanWaveform(duration, area)
            seq.add(
                Pulse.ConstantDetuning(wf, detuning=detuning, phase=phase),
                "rydberg",
            )

    seq.measure("digital")

    sim = QutipEmulator.from_sequence(seq)
    results = sim.run()
    counts = results.sample_final_state(N_samples=shots)

    return dict(counts)


def _safe_duration(area: float, channel: Any) -> int:
    """Find minimum pulse duration within channel amplitude limits."""
    from pulser.waveforms import BlackmanWaveform

    max_amp = float(channel.max_amp)
    clock = int(channel.clock_period)
    min_dur = int(channel.min_duration)

    for dur in range(min_dur, 10000, clock):
        bw = BlackmanWaveform(dur, area)
        peak = float(np.max(np.abs(np.array(bw.samples))))
        if peak <= max_amp:
            return dur

    return 1000


def _parse_ops(qasm: str) -> tuple[int, list[tuple[str, list[int]]]]:
    """Parse QASM3 into (n_qubits, [(gate, [qubits])])."""
    import re

    n_qubits = 1
    ops = []
    for line in qasm.splitlines():
        line = line.strip().rstrip(";")
        if not line or line.startswith(("//", "OPENQASM")):
            continue
        if line.startswith("qubit["):
            n_qubits = int(line.split("[")[1].split("]")[0])
            continue
        if line.startswith(("bit", "creg", "qreg", "include")):
            continue
        if "= measure" in line or line.startswith("measure"):
            continue

        match = re.match(r"(\w+)(?:\([^)]*\))?\s+(.+)", line)
        if match:
            gate = match.group(1).lower()
            qubit_indices = [int(x) for x in re.findall(r"q\[(\d+)\]", match.group(2))]
            if qubit_indices:
                ops.append((gate, qubit_indices))

    return n_qubits, ops


def sweep_single_qubit_gates(
    ground_truth: dict[str, dict[str, int]],
    shots: int = 2000,
) -> dict[str, CalibrationResult]:
    """Sweep pulse parameters for single-qubit gates against ground truth.

    Searches over area and phase combinations to find parameters that
    produce measurement distributions matching the gate-model results.
    """
    # Parameter grid for single-qubit gates
    areas = np.linspace(0.1, 2 * math.pi, 24)
    phases = np.linspace(0, 2 * math.pi, 16, endpoint=False)
    post_shifts = [0.0, math.pi / 4, math.pi / 2, math.pi, 3 * math.pi / 2]

    results: dict[str, CalibrationResult] = {}

    for test_name, test_info in _SINGLE_QUBIT_TESTS.items():
        if test_name not in ground_truth:
            continue

        qasm = test_info["qasm"]
        truth = ground_truth[test_name]
        n_qubits, ops = _parse_ops(qasm)

        # Which gates do we need to calibrate?
        gate_names = list({g for g, _ in ops})

        print(f"\n  Calibrating [{test_name}] gates={gate_names}")
        best_fidelity = 0.0
        best_params: dict[str, PulseParams] = {}
        trials = []

        # For single-gate circuits, sweep parameters for that gate
        if len(gate_names) == 1:
            gate = gate_names[0]
            for area, phase, pps in itertools.product(areas, phases, post_shifts):
                p = PulseParams(area=float(area), phase=float(phase), post_phase_shift=float(pps))
                try:
                    counts = get_pulser_result(
                        n_qubits, ops,
                        pulse_rules={gate: p},
                        entangle_params={},
                        shots=shots,
                    )
                    fid = _distribution_fidelity(truth, counts)
                    trials.append({"params": p.to_dict(), "fidelity": fid, "counts": counts})

                    if fid > best_fidelity:
                        best_fidelity = fid
                        best_params = {gate: p}
                        if fid > 0.99:
                            break
                except Exception:
                    continue

        if best_params:
            gate = gate_names[0]
            results[test_name] = CalibrationResult(
                gate=gate,
                qasm_circuit=qasm,
                ground_truth=truth,
                best_params=best_params[gate],
                best_fidelity=best_fidelity,
                all_trials=trials[-10:],  # keep last 10
            )
            print(f"    best fidelity: {best_fidelity:.4f}")
            print(f"    params: {best_params[gate].to_dict()}")

    return results


def sweep_entangling_gate(
    ground_truth: dict[str, dict[str, int]],
    single_qubit_rules: dict[str, PulseParams],
    shots: int = 2000,
) -> dict[str, CalibrationResult]:
    """Sweep entangling gate parameters using known single-qubit rules."""
    areas = np.linspace(0.5, 4 * math.pi, 32)
    phases = np.linspace(0, 2 * math.pi, 16, endpoint=False)
    detunings = np.linspace(-10.0, 10.0, 8)

    results: dict[str, CalibrationResult] = {}

    for test_name, test_info in _TWO_QUBIT_TESTS.items():
        if test_name not in ground_truth:
            continue

        qasm = test_info["qasm"]
        truth = ground_truth[test_name]
        n_qubits, ops = _parse_ops(qasm)

        print(f"\n  Calibrating entangling [{test_name}]")
        best_fidelity = 0.0
        best_entangle: dict[str, float] = {}
        trials = []

        for area, phase, det in itertools.product(areas, phases, detunings):
            ep = {"area": float(area), "phase": float(phase), "detuning": float(det)}
            try:
                counts = get_pulser_result(
                    n_qubits, ops,
                    pulse_rules=single_qubit_rules,
                    entangle_params=ep,
                    shots=shots,
                )
                fid = _distribution_fidelity(truth, counts)
                trials.append({"entangle_params": ep, "fidelity": fid, "counts": counts})

                if fid > best_fidelity:
                    best_fidelity = fid
                    best_entangle = ep
                    if fid > 0.99:
                        break
            except Exception:
                continue

        if best_entangle:
            results[test_name] = CalibrationResult(
                gate="cx",
                qasm_circuit=qasm,
                ground_truth=truth,
                best_params=PulseParams(**best_entangle),
                best_fidelity=best_fidelity,
                all_trials=trials[-10:],
            )
            print(f"    best fidelity: {best_fidelity:.4f}")
            print(f"    params: {best_entangle}")

    return results


def run_calibration(
    api_key: str,
    shots: int = 2000,
    device_id: str = "azure:ionq:sim:simulator",
) -> dict[str, Any]:
    """Full calibration pipeline.

    1. Get ground truth from gate-model backend (qBraid)
    2. Sweep single-qubit gate pulse parameters
    3. Sweep entangling gate parameters using calibrated single-qubit gates
    4. Return calibration table ready for Rust codegen

    Args:
        api_key: qBraid API key
        shots: shots per circuit
        device_id: gate-model device for ground truth

    Returns:
        Calibration rules dict ready for export to Rust
    """
    print("=== Phase 1: Ground truth from gate-model backend ===")
    all_qasm = {}
    for name, info in {**_SINGLE_QUBIT_TESTS, **_TWO_QUBIT_TESTS}.items():
        all_qasm[name] = info["qasm"]

    ground_truth = get_ground_truth_qbraid(all_qasm, api_key, shots, device_id)

    print("\n=== Phase 2: Calibrate single-qubit gates ===")
    single_results = sweep_single_qubit_gates(ground_truth, shots)

    # Extract best single-qubit rules
    single_rules: dict[str, PulseParams] = {}
    for test_name, result in single_results.items():
        if result.best_fidelity > 0.95:
            single_rules[result.gate] = result.best_params

    print(f"\n  Calibrated gates: {list(single_rules.keys())}")

    print("\n=== Phase 3: Calibrate entangling gate ===")
    entangle_results = sweep_entangling_gate(ground_truth, single_rules, shots)

    # Compile final rules
    rules = {
        "single_qubit": {
            gate: params.to_dict() for gate, params in single_rules.items()
        },
        "entangling": {},
    }
    for test_name, result in entangle_results.items():
        if result.best_fidelity > 0.90:
            rules["entangling"][result.gate] = result.best_params.to_dict()

    print("\n=== Calibration complete ===")
    print(f"Rules: {rules}")

    return rules
