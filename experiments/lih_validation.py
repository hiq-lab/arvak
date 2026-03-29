"""
LiH molecular Hamiltonian: sin(C/2) adaptive MPS vs uniform.

Extracts ZZ coupling strengths from the 12-qubit LiH Hamiltonian,
uses them as entangling weights for the channel map, and compares
adaptive vs uniform bond dimension MPS at fixed memory budget.

This is the Paper validation: do molecular coupling heterogeneities
(305x range in ZZ strengths) translate into MPS compression advantage?
"""

import numpy as np
import pennylane as qml
from math import sqrt, pi
from time import perf_counter
import json

# ── Build LiH Hamiltonian ─────────────────────────────────────────

print("Building LiH Hamiltonian (STO-3G, full 12 qubits)...")
symbols = ['Li', 'H']
coords = np.array([0.0, 0.0, 0.0, 0.0, 0.0, 1.546])
H_lih, n_qubits = qml.qchem.molecular_hamiltonian(
    symbols, coords, charge=0, mult=1, basis='sto-3g'
)
print(f"  {n_qubits} qubits, {len(H_lih.operands)} Hamiltonian terms")

# ── Extract ZZ couplings and per-qubit frequencies ────────────────

zz_couplings = []  # (i, j, |coeff|)
single_z = {}      # qubit -> sum of |coeff| for Z terms

for term in H_lih.operands:
    c = abs(float(term.scalar)) if hasattr(term, 'scalar') else 1.0
    base = term.base if hasattr(term, 'base') else term
    wires = list(base.wires)

    if len(wires) == 2:
        zz_couplings.append((wires[0], wires[1], c))
    elif len(wires) == 1:
        single_z[wires[0]] = single_z.get(wires[0], 0.0) + c

# Per-qubit frequency = sum of all coupling strengths involving that qubit
freqs = np.zeros(n_qubits)
for i, j, c in zz_couplings:
    freqs[i] += c
    freqs[j] += c
for q, c in single_z.items():
    freqs[q] += c

# Ensure nonzero
freqs = np.maximum(freqs, 0.01)

print(f"\n  ZZ couplings: {len(zz_couplings)}")
zz_strengths = [c for _, _, c in zz_couplings]
print(f"  ZZ range: {min(zz_strengths):.6f} - {max(zz_strengths):.6f} ({max(zz_strengths)/min(zz_strengths):.0f}x)")
print(f"  Per-qubit freq range: {freqs.min():.4f} - {freqs.max():.4f}")

# ── sin(C/2) analysis ─────────────────────────────────────────────

def commensurability_residual(omega_i, omega_j, max_order=12):
    if abs(omega_j) < 1e-15:
        return pi
    ratio = omega_i / omega_j
    min_dist = float('inf')
    for q in range(1, max_order + 1):
        p = round(ratio * q)
        if p > 0:
            dist = abs(ratio - p / q)
            min_dist = min(min_dist, dist)
    return min_dist

def sin_c_half(omega_i, omega_j):
    c = commensurability_residual(omega_i, omega_j)
    return abs(np.sin(c / 2.0))

print(f"\n  sin(C/2) matrix (sample, first 6 qubits):")
for i in range(min(6, n_qubits)):
    row = []
    for j in range(min(6, n_qubits)):
        if i == j:
            row.append("  --- ")
        else:
            row.append(f" {sin_c_half(freqs[i], freqs[j]):.4f}")
    print(f"    q{i}: {''.join(row)}")

# Per-bond weight with locality decay
bond_weights = []
for bond in range(n_qubits - 1):
    total = 0.0
    weight_sum = 0.0
    for a in range(bond + 1):
        for b in range(bond + 1, n_qubits):
            dist = (bond - a + b - bond - 1)
            w = np.exp(-0.5 * dist)
            total += sin_c_half(freqs[a], freqs[b]) * w
            weight_sum += w
    bw = total / weight_sum if weight_sum > 1e-15 else 0.0
    bond_weights.append(bw)

print(f"\n  Per-bond sin(C/2) weights:")
max_w = max(bond_weights) if bond_weights else 1
for bond in range(n_qubits - 1):
    bar = "#" * int(bond_weights[bond] / max_w * 40) if max_w > 0 else ""
    print(f"    bond {bond:2d}-{bond+1:2d}: {bond_weights[bond]:.5f}  {bar}")

# ── Build Trotter circuit from molecular Hamiltonian ──────────────

print(f"\n  Building Trotter circuit from molecular Hamiltonian...")

# Use quimb MPS for simulation (same as tn_sinc_test.py)
import quimb.tensor as qtn

def run_mps_molecular(n_qubits, zz_couplings, single_z_terms, n_steps, max_bond):
    """Trotter-evolve the molecular Hamiltonian as MPS."""
    circ = qtn.CircuitMPS(n_qubits, max_bond=max_bond)

    dt = 1.0 / n_steps
    for _step in range(n_steps):
        # Single-qubit Z rotations (on-site terms)
        for q, coeff in single_z_terms.items():
            circ.apply_gate("RZ", 2 * coeff * dt, q)

        # Two-qubit ZZ rotations (coupling terms)
        for i, j, coeff in zz_couplings:
            if abs(i - j) == 1:  # nearest-neighbor only for MPS
                circ.apply_gate("RZZ", 2 * coeff * dt, i, j)

        # Kick: RX to create superposition (activates ZZ channels)
        for q in range(n_qubits):
            circ.apply_gate("RX", 0.5, q)

    return circ

def mps_to_dense(circ, n_qubits):
    psi_tn = circ.psi
    return psi_tn.to_dense(["k" + str(i) for i in range(n_qubits)]).ravel()

def fidelity(psi_a, psi_b):
    psi_a = psi_a / np.linalg.norm(psi_a)
    psi_b = psi_b / np.linalg.norm(psi_b)
    return abs(np.vdot(psi_a, psi_b)) ** 2

# ── Ground truth: high-chi MPS ────────────────────────────────────

n_steps = 6
print(f"  Trotter steps: {n_steps}")

print(f"\n  Computing ground truth (chi=256)...", end=" ", flush=True)
t0 = perf_counter()
circ_exact = run_mps_molecular(n_qubits, zz_couplings, single_z, n_steps, max_bond=256)
psi_exact = mps_to_dense(circ_exact, n_qubits)
print(f"done ({perf_counter() - t0:.1f}s)")

# ── Comparison: uniform vs adaptive ──────────────────────────────

# Adaptive chi allocation based on bond weights
def adaptive_chi(bond_weights, chi_max):
    w_sum = sum(bond_weights)
    n_bonds = len(bond_weights)
    if w_sum < 1e-15:
        return [chi_max] * n_bonds
    return [
        max(2, int(chi_max * np.sqrt(w / w_sum * n_bonds)))
        for w in bond_weights
    ]

print(f"\n{'='*70}")
print(f"  LiH 12q MOLECULAR HAMILTONIAN — adaptive vs uniform MPS")
print(f"{'='*70}")
print(f"  {'chi':>5} | {'F_uniform':>10} | {'F_adaptive':>10} | {'delta':>10} | {'adapt_range':>12} | {'winner':>8}")
print(f"  {'-'*68}")

results = []
for chi_max in [4, 8, 16, 32, 64]:
    # Uniform
    circ_u = run_mps_molecular(n_qubits, zz_couplings, single_z, n_steps, max_bond=chi_max)
    psi_u = mps_to_dense(circ_u, n_qubits)
    f_u = fidelity(psi_exact, psi_u)

    # Adaptive: run with max adaptive chi, then compress per bond
    adapt = adaptive_chi(bond_weights, chi_max)
    max_adapt = max(adapt)
    circ_a = run_mps_molecular(n_qubits, zz_couplings, single_z, n_steps, max_bond=max_adapt)
    psi_mps = circ_a.psi
    for bond in range(n_qubits - 1):
        psi_mps.compress_between(bond, bond + 1, max_bond=adapt[bond])
    psi_a = psi_mps.to_dense(["k" + str(i) for i in range(n_qubits)]).ravel()
    f_a = fidelity(psi_exact, psi_a)

    delta = f_a - f_u
    winner = "ADAPT" if delta > 0.001 else ("uniform" if delta < -0.001 else "~equal")
    adapt_range = f"{min(adapt)}-{max(adapt)}"

    print(f"  {chi_max:>5} | {f_u:>10.6f} | {f_a:>10.6f} | {delta:>+10.6f} | {adapt_range:>12} | {winner:>8}")

    results.append({
        "chi_max": chi_max,
        "f_uniform": float(f_u),
        "f_adaptive": float(f_a),
        "delta": float(delta),
        "adapt_range": adapt_range,
    })

# ── Also test with coupling-strength-weighted bonds ───────────────

print(f"\n{'='*70}")
print(f"  VARIANT B: weight by ZZ coupling strength (not sin(C/2))")
print(f"{'='*70}")

# Bond weight = sum of |J_ij| for all ZZ terms crossing this bond
coupling_weights = [0.0] * (n_qubits - 1)
for i, j, c in zz_couplings:
    if abs(i - j) == 1:
        bond = min(i, j)
        coupling_weights[bond] += c

print(f"  Coupling-based bond weights:")
max_cw = max(coupling_weights) if coupling_weights else 1
for bond in range(n_qubits - 1):
    bar = "#" * int(coupling_weights[bond] / max_cw * 40) if max_cw > 0 else ""
    print(f"    bond {bond:2d}-{bond+1:2d}: {coupling_weights[bond]:.5f}  {bar}")

print(f"\n  {'chi':>5} | {'F_uniform':>10} | {'F_coupling':>10} | {'delta':>10} | {'adapt_range':>12} | {'winner':>8}")
print(f"  {'-'*68}")

for chi_max in [4, 8, 16, 32, 64]:
    # Uniform (already computed)
    circ_u = run_mps_molecular(n_qubits, zz_couplings, single_z, n_steps, max_bond=chi_max)
    psi_u = mps_to_dense(circ_u, n_qubits)
    f_u = fidelity(psi_exact, psi_u)

    # Coupling-weighted adaptive
    adapt_c = adaptive_chi(coupling_weights, chi_max)
    max_adapt_c = max(adapt_c)
    circ_c = run_mps_molecular(n_qubits, zz_couplings, single_z, n_steps, max_bond=max_adapt_c)
    psi_mps_c = circ_c.psi
    for bond in range(n_qubits - 1):
        psi_mps_c.compress_between(bond, bond + 1, max_bond=adapt_c[bond])
    psi_c = psi_mps_c.to_dense(["k" + str(i) for i in range(n_qubits)]).ravel()
    f_c = fidelity(psi_exact, psi_c)

    delta = f_c - f_u
    winner = "ADAPT" if delta > 0.001 else ("uniform" if delta < -0.001 else "~equal")
    adapt_range = f"{min(adapt_c)}-{max(adapt_c)}"

    print(f"  {chi_max:>5} | {f_u:>10.6f} | {f_c:>10.6f} | {delta:>+10.6f} | {adapt_range:>12} | {winner:>8}")

# Save
with open("experiments/lih_validation_results.json", "w") as f:
    json.dump({"lih_12q": results, "bond_weights_sinc": bond_weights, "coupling_weights": coupling_weights}, f, indent=2)
print(f"\nResults saved to experiments/lih_validation_results.json")
