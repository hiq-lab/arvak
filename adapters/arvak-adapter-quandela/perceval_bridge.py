#!/usr/bin/env python3
"""Arvak ↔ Perceval bridge for Quandela cloud submission.

Commands (all output is a JSON object on stdout):
  ping   <platform>
  submit <platform> <shots> <circuit_json>
  status <platform> <job_id>
  result <platform> <job_id> <n_qubits> <circuit_json>
  cancel <platform> <job_id>

Environment:
  PCVL_CLOUD_TOKEN — Quandela cloud token (falls back to
                     ~/.openclaw/credentials/quandela/cloud.key)

Supported gates
  Single-qubit: h, x, y, z, s, sdg, t, tdg, rx, ry, rz, id
  Two-qubit:    cx / cnot (postprocessed CNOT), cz (postprocessed CZ)

Two-qubit gate constraint
  Only adjacent qubit pairs are supported without SWAP insertion:
    ctrl = i, data = i+1
  Non-adjacent pairs produce an error.

Mode layout (dual-rail encoding)
  Qubit q occupies signal modes (2q, 2q+1).  |0>_L = photon in mode 2q,
  |1>_L = photon in mode 2q+1.  Each two-qubit gate inserts 2 ancilla modes
  immediately after the data qubit's signal modes; subsequent qubits shift up
  accordingly.
"""

import json
import math
import os
import sys
import traceback

import perceval as pcvl
from perceval.algorithm import Sampler
from perceval.runtime.rpc_handler import RPCHandler
from perceval.serialization import deserialize as pcvl_deserialize

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

CLOUD_URL = "https://api.cloud.quandela.com"

# ---------------------------------------------------------------------------
# Authentication
# ---------------------------------------------------------------------------


def _get_token() -> str:
    token = os.environ.get("PCVL_CLOUD_TOKEN", "").strip()
    if not token:
        keyfile = os.path.expanduser("~/.openclaw/credentials/quandela/cloud.key")
        if os.path.exists(keyfile):
            with open(keyfile) as fh:
                token = fh.read().strip()
    if not token:
        raise RuntimeError(
            "PCVL_CLOUD_TOKEN not set and "
            "~/.openclaw/credentials/quandela/cloud.key not found"
        )
    return token


# ---------------------------------------------------------------------------
# Gate catalog
# ---------------------------------------------------------------------------

# Maps Arvak gate name → (perceval_catalog_name, [param_kwarg_names])
_1Q_CATALOG: dict = {
    "h":   ("h",    []),
    "x":   ("x",    []),
    "y":   ("y",    []),
    "z":   ("z",    []),
    "s":   ("s",    []),
    "sdg": ("sdag", []),
    "t":   ("t",    []),
    "tdg": ("tdag", []),
    "rx":  ("rx",   ["theta"]),
    "ry":  ("ry",   ["theta"]),
    "rz":  ("rz",   ["phi"]),
    "id":  None,  # identity → 2-mode no-op circuit
}

_2Q_CATALOG: dict = {
    "cx":   "postprocessed cnot",
    "cnot": "postprocessed cnot",
    "cz":   "postprocessed cz",
}


def _build_1q_circuit(name: str, params: list) -> pcvl.Circuit:
    entry = _1Q_CATALOG.get(name)
    if entry is None:
        if name == "id":
            return pcvl.Circuit(2)
        raise ValueError(f"Unsupported single-qubit gate: {name!r}")
    catalog_name, param_keys = entry
    kwargs = dict(zip(param_keys, params))
    return pcvl.catalog[catalog_name].build_circuit(**kwargs)


def _build_2q_circuit(name: str):
    """Return (6-mode Circuit, herald dict) for a two-qubit gate."""
    catalog_name = _2Q_CATALOG.get(name)
    if catalog_name is None:
        raise ValueError(f"Unsupported two-qubit gate: {name!r}")
    proc = pcvl.catalog[catalog_name].build_processor()
    circ = pcvl.catalog[catalog_name].build_circuit()
    return circ, proc.heralds  # heralds: {4: 0, 5: 0}


# ---------------------------------------------------------------------------
# Mode layout
# ---------------------------------------------------------------------------


def _compute_mode_layout(n_qubits: int, gates: list):
    """
    Compute mode offsets for each qubit and ancilla positions for two-qubit gates.

    Returns:
      qubit_modes: list[int]  — starting mode index for each qubit
      ancilla_list: list[dict] — {ctrl, data, anc_start} per two-qubit gate
      n_total: int             — total mode count
    """
    qubit_modes = list(range(0, 2 * n_qubits, 2))
    ancilla_list = []

    for gate in gates:
        if gate["name"] not in _2Q_CATALOG:
            continue
        ctrl, data = gate["qubits"][0], gate["qubits"][1]
        if data != ctrl + 1:
            raise ValueError(
                f"Two-qubit gate {gate['name']!r} on qubits ({ctrl}, {data}): "
                "only adjacent pairs (data = ctrl + 1) are supported."
            )
        anc_start = qubit_modes[data] + 2
        for q in range(data + 1, n_qubits):
            qubit_modes[q] += 2
        ancilla_list.append({"ctrl": ctrl, "data": data, "anc_start": anc_start})

    if n_qubits == 0:
        n_total = 0
    else:
        n_total = max(qubit_modes[q] + 2 for q in range(n_qubits))
    if ancilla_list:
        n_total = max(n_total, max(a["anc_start"] + 2 for a in ancilla_list))

    return qubit_modes, ancilla_list, n_total


def _herald_set(ancilla_list: list) -> set:
    modes = set()
    for a in ancilla_list:
        modes.add(a["anc_start"])
        modes.add(a["anc_start"] + 1)
    return modes


def _non_herald_modes(n_total: int, ancilla_list: list) -> list:
    heralds = _herald_set(ancilla_list)
    return [m for m in range(n_total) if m not in heralds]


# ---------------------------------------------------------------------------
# Processor builder (local or remote)
# ---------------------------------------------------------------------------


def _build_combined_circuit(circuit_json: dict, qubit_modes: list,
                            n_total: int) -> pcvl.Circuit:
    """
    Build a Perceval Circuit (not Processor) from the gate list.

    Uses Circuit.add(offset, component) which correctly places each component
    at an absolute mode offset.  This must be done on a Circuit object — NOT
    directly on a Processor, which uses sequential (appending) composition.
    """
    gates = circuit_json["gates"]
    combined = pcvl.Circuit(n_total) if n_total > 0 else pcvl.Circuit(2)

    for gate in gates:
        name = gate["name"]
        if name in ("measure", "barrier", "reset"):
            continue
        qubits = gate["qubits"]
        params = gate.get("params", [])

        if len(qubits) == 1:
            circ = _build_1q_circuit(name, params)
            combined.add(qubit_modes[qubits[0]], circ)
        elif len(qubits) == 2:
            circ, _ = _build_2q_circuit(name)
            combined.add(qubit_modes[qubits[0]], circ)
        else:
            raise ValueError(
                f"Gate {name!r} on {len(qubits)} qubits is not supported"
            )

    return combined


def _make_local_processor(circuit_json: dict, backend: str = "SLOS") -> pcvl.Processor:
    """
    Build a fully configured local Processor for the given circuit.

    Returns a Processor with heralds, post-selection, and input state set.
    """
    n_qubits = circuit_json["n_qubits"]
    qubit_modes, ancilla_list, n_total = _compute_mode_layout(n_qubits, circuit_json["gates"])

    combined = _build_combined_circuit(circuit_json, qubit_modes, n_total)
    proc = pcvl.Processor(backend, combined)

    for a in ancilla_list:
        proc.add_herald(a["anc_start"], 0)
        proc.add_herald(a["anc_start"] + 1, 0)

    terms = [
        f"[{qubit_modes[q]},{qubit_modes[q]+1}]==1"
        for q in range(n_qubits)
    ]
    if terms:
        proc.set_postselection(pcvl.PostSelect(" & ".join(terms)))

    non_herald = _non_herald_modes(n_total, ancilla_list)
    inp = [0] * len(non_herald)
    for i, m in enumerate(non_herald):
        for q in range(n_qubits):
            if m == qubit_modes[q]:
                inp[i] = 1
                break
    proc.with_input(pcvl.BasicState(inp))

    return proc


def _build_remote_processor(circuit_json: dict, platform: str, token: str):
    """
    Build a RemoteProcessor for cloud submission.

    Strategy: build the full combined Circuit with absolute offsets first
    (Circuit.add is positional), then wrap it in a RemoteProcessor
    initialised to n_total modes.  This avoids the sequential-composition
    semantics of Processor.add / RemoteProcessor.add.
    """
    n_qubits = circuit_json["n_qubits"]
    qubit_modes, ancilla_list, n_total = _compute_mode_layout(
        n_qubits, circuit_json["gates"]
    )
    n_modes = n_total if n_total > 0 else 2

    combined = _build_combined_circuit(circuit_json, qubit_modes, n_total)
    rp = pcvl.RemoteProcessor(name=platform, token=token, m=n_modes)
    rp.add(0, combined)

    for a in ancilla_list:
        rp.add_herald(a["anc_start"], 0)
        rp.add_herald(a["anc_start"] + 1, 0)

    terms = [
        f"[{qubit_modes[q]},{qubit_modes[q]+1}]==1"
        for q in range(n_qubits)
    ]
    if terms:
        rp.set_postselection(pcvl.PostSelect(" & ".join(terms)))

    non_herald = _non_herald_modes(n_total, ancilla_list)
    inp = [0] * len(non_herald)
    for i, m in enumerate(non_herald):
        for q in range(n_qubits):
            if m == qubit_modes[q]:
                inp[i] = 1
                break
    rp.with_input(pcvl.BasicState(inp))
    # One photon per qubit in dual-rail encoding; discard events with fewer.
    rp.min_detected_photons_filter(n_qubits)

    return rp


# ---------------------------------------------------------------------------
# Result decoding
# ---------------------------------------------------------------------------


def _decode_results(raw_results: dict, qubit_modes: list,
                    ancilla_list: list, n_total: int, n_qubits: int) -> dict:
    """Convert Perceval FockState counts → qubit bitstring counts."""
    non_herald = _non_herald_modes(n_total, ancilla_list)
    counts: dict = {}

    for fock_state, count in raw_results.items():
        photons = list(fock_state)
        bits = []
        for q in range(n_qubits):
            m0, m1 = qubit_modes[q], qubit_modes[q] + 1
            try:
                i0 = non_herald.index(m0)
                i1 = non_herald.index(m1)
            except ValueError:
                bits.append("?")
                continue
            if photons[i0] == 1 and photons[i1] == 0:
                bits.append("0")
            elif photons[i0] == 0 and photons[i1] == 1:
                bits.append("1")
            else:
                bits.append("?")
        bitstring = "".join(bits)
        counts[bitstring] = counts.get(bitstring, 0) + int(count)

    return counts


# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------


def cmd_ping(platform: str):
    rpc = RPCHandler(platform, CLOUD_URL, _get_token())
    details = rpc.fetch_platform_details()
    print(json.dumps({"status": details.get("status", "unknown"), "error": None}))


def cmd_submit(platform: str, shots: int, circuit_json_str: str):
    circuit_json = json.loads(circuit_json_str)
    token = _get_token()
    rp = _build_remote_processor(circuit_json, platform, token)
    sampler = Sampler(rp, max_shots_per_call=shots)
    job = sampler.sample_count
    job.execute_async(shots)
    print(json.dumps({"job_id": job.id, "error": None}))


def cmd_status(platform: str, job_id: str):
    rpc = RPCHandler(platform, CLOUD_URL, _get_token())
    resp = rpc.get_job_status(job_id)
    raw = resp.get("status", "UNKNOWN").upper()
    mapping = {
        "WAITING": "queued",
        "RUNNING": "running",
        "SUCCESS": "done",
        "ERROR":   "error",
        "CANCEL_REQUESTED": "cancelled",
        "CANCELED":         "cancelled",
    }
    status = mapping.get(raw, "unknown")
    msg = resp.get("status_message", "")
    print(json.dumps({"status": status, "raw": raw, "message": msg, "error": None}))


def cmd_result(platform: str, job_id: str, n_qubits: int, circuit_json_str: str):
    circuit_json = json.loads(circuit_json_str)
    gates = circuit_json["gates"]
    qubit_modes, ancilla_list, n_total = _compute_mode_layout(n_qubits, gates)

    rpc = RPCHandler(platform, CLOUD_URL, _get_token())
    resp = rpc.get_job_results(job_id)
    results_dict = pcvl_deserialize(json.loads(resp["results"]))
    raw_results = results_dict.get("results", {})

    counts = _decode_results(raw_results, qubit_modes, ancilla_list, n_total, n_qubits)
    print(json.dumps({"counts": counts, "error": None}))


def cmd_cancel(platform: str, job_id: str):
    rpc = RPCHandler(platform, CLOUD_URL, _get_token())
    rpc.cancel_job(job_id)
    print(json.dumps({"ok": True, "error": None}))


# ---------------------------------------------------------------------------
# Entrypoint
# ---------------------------------------------------------------------------


def _main():
    if len(sys.argv) < 2:
        print(json.dumps({"error": "Usage: perceval_bridge.py <command> [args...]"}))
        sys.exit(1)

    cmd = sys.argv[1]
    try:
        if cmd == "ping":
            cmd_ping(sys.argv[2])
        elif cmd == "submit":
            cmd_submit(sys.argv[2], int(sys.argv[3]), sys.argv[4])
        elif cmd == "status":
            cmd_status(sys.argv[2], sys.argv[3])
        elif cmd == "result":
            cmd_result(sys.argv[2], sys.argv[3], int(sys.argv[4]), sys.argv[5])
        elif cmd == "cancel":
            cmd_cancel(sys.argv[2], sys.argv[3])
        else:
            print(json.dumps({"error": f"Unknown command: {cmd!r}"}))
            sys.exit(1)
    except Exception as exc:  # pylint: disable=broad-except
        print(json.dumps({"error": str(exc), "traceback": traceback.format_exc()}))
        sys.exit(1)


if __name__ == "__main__":
    _main()
