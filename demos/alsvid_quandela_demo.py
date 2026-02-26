#!/usr/bin/env python3
"""Alsvid → Quandela Altair end-to-end enrollment demo.

Demonstrates the full PUF enrollment pipeline:

  1. Parse a vibration CSV (TwinCAT Scope format or standard time_s/acceleration_g)
  2. Run alsvid spectral analysis to identify compressor type and fundamental frequency
  3. POST to the alsvid-lab /signature API (mock=true — no QPU required)
  4. Receive the AlsvidEnrollment JSON record
  5. Show exactly what QuandelaBackend::ingest_alsvid_enrollment() would receive in Rust

The demo is fully offline: alsvid uses synthetic HOM visibility (mock=true) and no
Quandela QPU connection is needed. Requires the alsvid-lab server running locally.

Prerequisites
-------------
Start the alsvid-lab server:

    cd ~/Projects/alsvid-lab
    docker compose up --build
    # or: uvicorn app.main:app --port 8080

Usage
-----
    # Standard CSV (time_s, acceleration_g columns):
    python demos/alsvid_quandela_demo.py --csv path/to/vibration.csv

    # TwinCAT Scope format (auto-detected):
    python demos/alsvid_quandela_demo.py --csv "Druckdaten RDK101 1 Minute.csv"

    # Use built-in synthetic data (no CSV required):
    python demos/alsvid_quandela_demo.py --synthetic

Optional environment variables
-------------------------------
    ALSVID_URL   — alsvid-lab base URL (default: http://localhost:8080)
    N_PHASES     — vibration phases to sample (default: 8)
"""

from __future__ import annotations

import argparse
import io
import json
import math
import os
import sys
from pathlib import Path

import numpy as np
import requests

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

ALSVID_URL = os.environ.get("ALSVID_URL", "http://localhost:8080")
N_PHASES = int(os.environ.get("N_PHASES", "8"))

# ---------------------------------------------------------------------------
# CSV helpers
# ---------------------------------------------------------------------------

def _is_twincap(text: str) -> bool:
    """Detect TwinCAT Scope format by header structure."""
    return text.startswith("Name\t") and "SampleTime[ms]" in text[:500]


def _parse_twincap(text: str) -> bytes:
    """Convert TwinCAT Scope CSV to alsvid-compatible time_s/acceleration_g CSV."""
    lines = text.split("\r\n")

    sample_time_ms = 3.0
    for line in lines[:25]:
        if line.startswith("SampleTime[ms]\t"):
            sample_time_ms = float(line.split("\t")[1])

    timestamps, values = [], []
    for line in lines[24:]:
        parts = line.strip().split("\t")
        if len(parts) == 2:
            try:
                timestamps.append(int(parts[0]))
                values.append(float(parts[1].replace(",", ".")))
            except ValueError:
                pass

    t0 = timestamps[0]
    time_s = [(ts - t0) * 1e-7 for ts in timestamps]

    out_lines = ["time_s,acceleration_g"]
    for t, a in zip(time_s, values):
        out_lines.append(f"{t:.6f},{a:.6f}")
    return "\n".join(out_lines).encode()


def _load_csv_bytes(csv_path: str) -> tuple[bytes, str]:
    """Load CSV file, converting from TwinCAT format if needed."""
    raw = Path(csv_path).read_bytes()
    text = raw.decode("utf-8", errors="replace")
    filename = Path(csv_path).name

    if _is_twincap(text):
        print(f"  Detected TwinCAT Scope format — converting to alsvid CSV")
        return _parse_twincap(text), filename
    return raw, filename


def _synthetic_csv_bytes(duration_s: float = 2.0, sample_hz: float = 200.0,
                          fundamental_hz: float = 1.25) -> bytes:
    """Generate a synthetic Stirling-compressor pressure waveform."""
    t = np.arange(0, duration_s, 1.0 / sample_hz)
    # Triangular wave (odd harmonics) — Stirling bellows signature
    signal = np.zeros_like(t)
    for n in [1, 3, 5, 7]:
        signal += ((-1) ** ((n - 1) // 2) / n**2) * np.sin(2 * math.pi * n * fundamental_hz * t)
    signal *= 4.0 / math.pi**2  # normalise to ~1 bar pp

    lines = ["time_s,acceleration_g"] + [f"{ti:.4f},{ai:.6f}" for ti, ai in zip(t, signal)]
    return "\n".join(lines).encode()


# ---------------------------------------------------------------------------
# Alsvid API call
# ---------------------------------------------------------------------------

def call_alsvid_signature(csv_bytes: bytes, filename: str, n_phases: int) -> dict:
    """POST vibration CSV to alsvid /signature and return the parsed JSON body."""
    url = f"{ALSVID_URL}/signature"
    response = requests.post(
        url,
        data={"qpu": "quandela_altair", "mock": "true", "n_phases": str(n_phases)},
        files={"file": (filename, io.BytesIO(csv_bytes), "text/csv")},
        timeout=30,
    )
    response.raise_for_status()
    return response.json()


# ---------------------------------------------------------------------------
# Display helpers
# ---------------------------------------------------------------------------

def _section(title: str) -> None:
    print(f"\n{'─' * 60}")
    print(f"  {title}")
    print(f"{'─' * 60}")


def _print_enrollment(puf: dict) -> None:
    """Print the AlsvidEnrollment fields as they would appear in Rust."""
    print(f"  AlsvidEnrollment {{")
    print(f"      installation_id:          \"{puf['installation_id']}\"")
    print(f"      compressor_type:           \"{puf['compressor_type']}\"")
    print(f"      fingerprint_hash:          \"{puf['fingerprint_hash']}\"")
    print(f"      enrolled_at:               {puf['enrolled_at']}")
    print(f"      enrollment_shots:          {puf['enrollment_shots']}")
    print(f"      intra_distance_threshold:  {puf.get('intra_distance_threshold')}")
    print(f"      fundamental_hz:            {puf['fundamental_hz']}")
    hom = puf.get("hom_visibility_by_phase", [])
    hom_fmt = "[" + ", ".join(f"{v:.4f}" for v in hom) + "]"
    print(f"      hom_visibility_by_phase:   {hom_fmt}")
    print(f"  }}")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> None:
    parser = argparse.ArgumentParser(description="Alsvid → Quandela enrollment demo")
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--csv", metavar="PATH", help="Vibration CSV file (standard or TwinCAT format)")
    group.add_argument("--synthetic", action="store_true", help="Use synthetic Stirling waveform")
    args = parser.parse_args()

    print("Alsvid × Quandela Altair — End-to-End Enrollment Demo")
    print("======================================================")
    print(f"Alsvid URL : {ALSVID_URL}")
    print(f"QPU        : quandela_altair (mock=true)")
    print(f"N phases   : {N_PHASES}")

    # --- Step 1: Prepare vibration data ---
    _section("Step 1 — Vibration data")
    if args.synthetic or (not args.csv):
        print("  Using synthetic Stirling waveform (1.25 Hz fundamental, 2 s, 200 Hz)")
        csv_bytes = _synthetic_csv_bytes()
        filename = "synthetic_stirling.csv"
    else:
        print(f"  Loading: {args.csv}")
        csv_bytes, filename = _load_csv_bytes(args.csv)
        n_samples = csv_bytes.count(b"\n")
        print(f"  {n_samples} rows prepared for upload")

    # --- Step 2: POST to alsvid ---
    _section("Step 2 — POST /signature (alsvid-lab)")
    print(f"  → {ALSVID_URL}/signature  [qpu=quandela_altair  mock=true  n_phases={N_PHASES}]")
    try:
        body = call_alsvid_signature(csv_bytes, filename, N_PHASES)
    except requests.exceptions.ConnectionError:
        print(f"\n  ERROR: Cannot connect to alsvid-lab at {ALSVID_URL}")
        print("  Start the server with:")
        print("    cd ~/Projects/alsvid-lab && docker compose up")
        print("  or:  uvicorn app.main:app --port 8080")
        sys.exit(1)
    except requests.exceptions.HTTPError as exc:
        print(f"\n  ERROR: {exc}\n  Response: {exc.response.text[:400]}")
        sys.exit(1)

    puf = body["puf"]
    print(f"  ✓ 200 OK")
    print(f"  Compressor type : {puf['compressor_type']}")
    print(f"  Fundamental Hz  : {puf['fundamental_hz']}")
    print(f"  Fingerprint     : {puf['fingerprint_hash'][:16]}…")

    # --- Step 3: Show the AlsvidEnrollment struct ---
    _section("Step 3 — AlsvidEnrollment (Rust struct equivalent)")
    _print_enrollment(puf)

    # --- Step 4: HOM visibility by phase ---
    _section("Step 4 — HOM visibility by phase")
    hom = puf.get("hom_visibility_by_phase", [])
    for i, v in enumerate(hom):
        phase = i / max(len(hom), 1)
        bar = "█" * int(v * 30)
        print(f"  phase {phase:.2f}  V={v:.4f}  {bar}")
    modulation = max(hom) - min(hom) if hom else 0.0
    print(f"\n  Modulation (max−min): {modulation:.4f}")
    print(f"  Mean visibility:      {sum(hom)/len(hom):.4f}" if hom else "")

    # --- Step 5: What happens in Rust ---
    _section("Step 5 — Rust: QuandelaBackend::ingest_alsvid_enrollment()")
    print("""  let enrollment: AlsvidEnrollment = serde_json::from_value(response["puf"])?;
  backend.ingest_alsvid_enrollment(enrollment);

  // After ingest:
  //   cooling_profile.puf_enrollment   → populated (fingerprint_hash, enrolled_at, …)
  //   cooling_profile.transfer_function → Vec<TransferFunctionSample> with
  //                                        visibility_modulation per phase
  //   cooling_profile.compressor        → GiffordMcMahon (Altair 4K cold head)
  //                                        (overrides alsvid compressor_type,
  //                                         which describes the compressor drive,
  //                                         not the cold head thermodynamic cycle)""")

    # --- Step 6: Full JSON for inspection ---
    _section("Step 6 — Full /signature response (puf object)")
    print(json.dumps(puf, indent=2))

    print("\n✓ Demo complete.\n")


if __name__ == "__main__":
    main()
