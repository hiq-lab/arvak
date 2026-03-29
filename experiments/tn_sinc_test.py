"""
Tensor Network Truncation guided by sin(C/2) commensurability filter.

Test hypothesis: adaptive bond dimensions based on the commensurability
residual |sin(C_ij/2)| from Hinderink (2026) allow higher fidelity at
the same memory budget compared to uniform bond dimension MPS.

Test system: Heisenberg chain with sqrt-prime on-site fields,
Trotter-evolved for t=1.0.

    H = sum_{i<j} J (XX + YY + ZZ)  +  sum_i h_i Z_i
    h_i = sqrt(p_i),  p_i = i-th prime

Variants:
  A) Statevector (exact ground truth)
  B) MPS with uniform chi
  C) MPS with adaptive chi ~ |sin(C_ij/2)|, same total memory budget
"""

import numpy as np
from math import gcd, sqrt, pi
from fractions import Fraction
from time import perf_counter
import json

# ── Primes & frequencies ──────────────────────────────────────────────

def primes(n):
    """First n primes."""
    out, candidate = [], 2
    while len(out) < n:
        if all(candidate % p for p in out):
            out.append(candidate)
        candidate += 1
    return out


def commensurability_residual(omega_i, omega_j, max_order=10):
    """
    C_ij = min distance of omega_i/omega_j to any low-order rational p/q
    with 1 <= q <= max_order.
    """
    ratio = omega_i / omega_j
    min_dist = float("inf")
    for q in range(1, max_order + 1):
        p = round(ratio * q)
        if p > 0:
            dist = abs(ratio - p / q)
            min_dist = min(min_dist, dist)
    return min_dist


def sin_c_half(omega_i, omega_j, max_order=10):
    """The commensurability filter |sin(C_ij/2)|."""
    c = commensurability_residual(omega_i, omega_j, max_order)
    return abs(np.sin(c / 2))


# ── Circuit construction (Trotter 2nd order) ──────────────────────────

def build_qkr_circuit_gates(n_qubits, K=2.0, n_steps=8):
    """
    Build Quantum Kicked Rotor circuit: U = (T · V)^n_steps.

    T = free propagator = product of RZ (single-qubit) and ZZ (pairwise)
        from p^2 in binary encoding. Phases theta_ij = h * 2^{i+j}.
    V = kick operator = RX rotations (creates superposition, activates
        the ZZ entanglement channels).

    This is the exact structure from the pair-counting paper:
    - N single-qubit Rz gates (from diagonal terms in p^2)
    - N(N-1)/2 pairwise ZZ gates (from cross-terms s_i * s_j)
    - N Rx kicks (the non-diagonal operator that activates channels)

    K controls the kick strength. K > K_c triggers scrambling.
    """
    ps = primes(n_qubits)
    omegas = [sqrt(p) for p in ps]
    hbar = 1.0  # effective Planck constant

    gates = []
    for step in range(n_steps):
        # ── T: free propagator exp(-i*hbar*p^2/2) ──
        # Single-qubit phases: phi_i from 4^i terms
        for i in range(n_qubits):
            phi = hbar * (4 ** i) / 2
            gates.append(("RZ", phi, (i,)))

        # Pairwise ZZ phases: theta_ij = hbar * 2^{1+i+j}
        for i in range(n_qubits):
            for j in range(i + 1, n_qubits):
                theta = hbar * (2 ** (1 + i + j)) / 2
                gates.append(("ZZ", theta, (i, j)))

        # ── V: kick operator ──
        # RX rotations with frequency-dependent angles
        # (analogous to the Bessel kick matrix)
        for i in range(n_qubits):
            gates.append(("RX", K * omegas[i], (i,)))

    return gates, omegas


# ── Statevector simulation (exact) ────────────────────────────────────

def _gate_matrix(name, angle):
    """Return 2x2 or 4x4 unitary for the given gate."""
    if name == "RX":
        c, s = np.cos(angle / 2), np.sin(angle / 2)
        return np.array([
            [c, -1j*s],
            [-1j*s, c]
        ])
    elif name == "RZ":
        return np.array([
            [np.exp(-1j * angle / 2), 0],
            [0, np.exp(1j * angle / 2)]
        ])
    elif name == "XX":
        c, s = np.cos(angle), np.sin(angle)
        return np.array([
            [c, 0, 0, -1j*s],
            [0, c, -1j*s, 0],
            [0, -1j*s, c, 0],
            [-1j*s, 0, 0, c]
        ])
    elif name == "YY":
        c, s = np.cos(angle), np.sin(angle)
        return np.array([
            [c, 0, 0, 1j*s],
            [0, c, -1j*s, 0],
            [0, -1j*s, c, 0],
            [1j*s, 0, 0, c]
        ])
    elif name == "ZZ":
        return np.array([
            [np.exp(-1j * angle), 0, 0, 0],
            [0, np.exp(1j * angle), 0, 0],
            [0, 0, np.exp(1j * angle), 0],
            [0, 0, 0, np.exp(-1j * angle)]
        ])
    raise ValueError(f"Unknown gate: {name}")


def statevector_sim(n_qubits, gates):
    """Exact statevector simulation. Returns state vector."""
    dim = 2 ** n_qubits
    psi = np.zeros(dim, dtype=complex)
    psi[0] = 1.0  # |000...0>

    for name, angle, qubits in gates:
        U = _gate_matrix(name, angle)
        if len(qubits) == 1:
            # Single-qubit gate
            q = qubits[0]
            psi_new = np.zeros_like(psi)
            for idx in range(dim):
                bit = (idx >> (n_qubits - 1 - q)) & 1
                idx0 = idx & ~(1 << (n_qubits - 1 - q))
                idx1 = idx0 | (1 << (n_qubits - 1 - q))
                if bit == 0:
                    psi_new[idx0] += U[0, 0] * psi[idx0] + U[0, 1] * psi[idx1]
                    psi_new[idx1] += U[1, 0] * psi[idx0] + U[1, 1] * psi[idx1]
            psi = psi_new
        else:
            # Two-qubit gate
            q0, q1 = qubits
            psi_new = np.zeros_like(psi)
            for idx in range(dim):
                b0 = (idx >> (n_qubits - 1 - q0)) & 1
                b1 = (idx >> (n_qubits - 1 - q1)) & 1
                basis = b0 * 2 + b1
                # Clear both bits
                mask0 = 1 << (n_qubits - 1 - q0)
                mask1 = 1 << (n_qubits - 1 - q1)
                base_idx = idx & ~mask0 & ~mask1
                for b0_new in range(2):
                    for b1_new in range(2):
                        target = base_idx
                        if b0_new:
                            target |= mask0
                        if b1_new:
                            target |= mask1
                        new_basis = b0_new * 2 + b1_new
                        psi_new[target] += U[new_basis, basis] * psi[idx]
            psi = psi_new

    return psi


# ── MPS simulation via quimb ──────────────────────────────────────────

def mps_sim(n_qubits, gates, max_bond, adaptive_bonds=None):
    """
    MPS simulation using quimb's CircuitMPS.

    adaptive_bonds: if provided, dict mapping (i,j) -> local_max_bond.
    For quimb CircuitMPS, we use a single max_bond but track what
    an adaptive scheme would allow. The comparison is done by running
    uniform at the AVERAGE adaptive chi.
    """
    import quimb.tensor as qtn

    circ = qtn.CircuitMPS(n_qubits, max_bond=max_bond)

    for name, angle, qubits in gates:
        if name == "RX":
            circ.apply_gate("RX", angle, *qubits)
        elif name == "RZ":
            circ.apply_gate("RZ", angle, *qubits)
        elif name == "ZZ":
            circ.apply_gate("RZZ", 2 * angle, *qubits)

    return circ


def mps_to_dense(circ, n_qubits):
    """Convert MPS state to dense vector for fidelity comparison."""
    psi_tn = circ.psi
    # Contract to dense
    psi_dense = psi_tn.to_dense(["k" + str(i) for i in range(n_qubits)])
    return psi_dense.ravel()


# ── Adaptive bond analysis ────────────────────────────────────────────

def compute_adaptive_bonds(omegas, chi_max):
    """
    Compute adaptive bond dimensions using |sin(C_ij/2)| weighting.
    Returns dict of (i, j) -> chi_local and the average chi.
    """
    n = len(omegas)
    weights = {}
    for i in range(n - 1):
        # MPS bond between qubit i and i+1
        # The weight should reflect how much entanglement flows
        # across this bond. Sum contributions from all pairs (a, b)
        # where a <= i and b > i.
        w = 0.0
        count = 0
        for a in range(i + 1):
            for b in range(i + 1, n):
                w += sin_c_half(omegas[a], omegas[b])
                count += 1
        weights[i] = w / count if count > 0 else 1.0

    # Normalize: total budget = (n-1) * chi_max
    # Distribute proportionally to weights
    w_sum = sum(weights.values())
    bonds = {}
    for i in range(n - 1):
        chi_local = max(2, int(chi_max * (n - 1) * weights[i] / w_sum))
        bonds[i] = chi_local

    avg_chi = sum(bonds.values()) / len(bonds)
    return bonds, avg_chi, weights


# ── Fidelity ──────────────────────────────────────────────────────────

def fidelity(psi_exact, psi_approx):
    """State fidelity |<exact|approx>|^2."""
    psi_approx = psi_approx.ravel()
    psi_exact = psi_exact.ravel()
    # Normalize
    psi_approx = psi_approx / np.linalg.norm(psi_approx)
    psi_exact = psi_exact / np.linalg.norm(psi_exact)
    return abs(np.vdot(psi_exact, psi_approx)) ** 2


# ── Main experiment ───────────────────────────────────────────────────

def run_experiment_custom(freqs, chi_values, K=1.5, n_steps=10, label=""):
    """Run experiment with custom frequencies to test sin(C/2) variation."""
    n_qubits = len(freqs)
    print(f"\n{'='*60}")
    print(f"  {label}")
    print(f"  N = {n_qubits}, K = {K}, steps = {n_steps}")
    print(f"{'='*60}")

    omegas = freqs
    print(f"  Frequencies: {[f'{w:.3f}' for w in omegas]}")

    # Build QKR circuit with custom frequencies
    hbar = 1.0
    gates = []
    for step in range(n_steps):
        for i in range(n_qubits):
            phi = hbar * (4 ** i) / 2
            gates.append(("RZ", phi, (i,)))
        for i in range(n_qubits):
            for j in range(i + 1, n_qubits):
                theta = hbar * (2 ** (1 + i + j)) / 2
                gates.append(("ZZ", theta, (i, j)))
        for i in range(n_qubits):
            gates.append(("RX", K * omegas[i], (i,)))

    print(f"  Total gates: {len(gates)}")

    # sin(C/2) matrix
    print(f"\n  sin(C_ij/2) matrix:")
    for i in range(n_qubits):
        row = []
        for j in range(n_qubits):
            if i == j:
                row.append("  --- ")
            else:
                val = sin_c_half(omegas[i], omegas[j])
                row.append(f" {val:.3f}")
        print(f"    q{i}: {''.join(row)}")

    # Exact statevector
    print(f"\n  Computing exact statevector...", end=" ", flush=True)
    t0 = perf_counter()
    psi_exact = statevector_sim(n_qubits, gates)
    sv_time = perf_counter() - t0
    print(f"done ({sv_time:.2f}s)")

    # Adaptive analysis: per-bond sin(C/2) weight
    # Bond i separates {0..i} from {i+1..n-1}
    # Weight = average sin(C_ab/2) for all a<=i, b>i
    bond_weights = {}
    for bond in range(n_qubits - 1):
        w = 0.0
        count = 0
        for a in range(bond + 1):
            for b in range(bond + 1, n_qubits):
                w += sin_c_half(omegas[a], omegas[b])
                count += 1
        bond_weights[bond] = w / count if count else 1.0

    print(f"\n  Per-bond sin(C/2) weight (entanglement prediction):")
    max_w = max(bond_weights.values())
    for bond in range(n_qubits - 1):
        w = bond_weights[bond]
        bar = "#" * int(w / max_w * 40) if max_w > 0 else ""
        print(f"    bond {bond:2d}-{bond+1:2d}: {w:.5f}  {bar}")

    results = {"label": label, "n_qubits": n_qubits, "omegas": omegas, "runs": []}

    for chi_max in chi_values:
        # ── A: Uniform chi ──
        t0 = perf_counter()
        circ_u = mps_sim(n_qubits, gates, max_bond=chi_max)
        time_u = perf_counter() - t0
        psi_u = mps_to_dense(circ_u, n_qubits)
        f_u = fidelity(psi_exact, psi_u)
        bonds_u = [circ_u.psi[i].shape[-1] if i < n_qubits - 1 else 0
                    for i in range(n_qubits - 1)]
        mem_u = sum(d * d for d in bonds_u)

        # ── B: Adaptive chi — redistribute SAME total budget ──
        # Total memory budget from uniform: sum of chi^2 for each bond
        # Redistribute proportionally to sin(C/2) weight
        w_sum = sum(bond_weights.values())
        adaptive_chis = {}
        for bond in range(n_qubits - 1):
            # Allocate chi proportional to weight, keeping total chi^2 budget equal
            frac = bond_weights[bond] / w_sum
            # chi_bond such that sum(chi_bond^2) = sum(chi_max^2)
            # Simple: chi_bond = chi_max * sqrt(frac * (n-1))
            chi_bond = max(2, int(chi_max * sqrt(frac * (n_qubits - 1))))
            adaptive_chis[bond] = chi_bond

        mem_adaptive_budget = sum(c * c for c in adaptive_chis.values())

        # Run with per-bond truncation using quimb's TN manipulation
        # Since CircuitMPS doesn't support per-bond chi, we use
        # the MAXIMUM adaptive chi and then compress each bond individually
        max_adaptive = max(adaptive_chis.values())
        t0 = perf_counter()
        circ_a = mps_sim(n_qubits, gates, max_bond=max_adaptive)

        # Post-compress: truncate each bond to its adaptive limit
        psi_mps = circ_a.psi
        for bond in range(n_qubits - 1):
            target_chi = adaptive_chis[bond]
            psi_mps.compress_between(bond, bond + 1, max_bond=target_chi)

        time_a = perf_counter() - t0
        psi_a = psi_mps.to_dense(["k" + str(i) for i in range(n_qubits)]).ravel()
        f_a = fidelity(psi_exact, psi_a)
        bonds_a = [psi_mps[i].shape[-1] if i < n_qubits - 1 else 0
                    for i in range(n_qubits - 1)]
        mem_a = sum(d * d for d in bonds_a)

        print(f"\n  --- chi_max = {chi_max} ---")
        print(f"    UNIFORM:   F={f_u:.8f}  bonds={bonds_u}  mem={mem_u}")
        print(f"    ADAPTIVE:  F={f_a:.8f}  bonds={bonds_a}  mem={mem_a}")
        print(f"    Adaptive chi targets: {list(adaptive_chis.values())}")
        delta = f_a - f_u
        mem_ratio = mem_a / mem_u if mem_u > 0 else 0
        print(f"    Delta F = {delta:+.8f}, memory ratio = {mem_ratio:.2f}")
        if delta > 0.001:
            print(f"    >> ADAPTIVE WINS by {delta:.4f} at {mem_ratio:.0%} memory")
        elif delta < -0.001:
            print(f"    >> Uniform wins by {-delta:.4f}")
        else:
            print(f"    >> Roughly equal")

        results["runs"].append({
            "chi_max": chi_max,
            "f_uniform": float(f_u),
            "f_adaptive": float(f_a),
            "mem_uniform": mem_u,
            "mem_adaptive": mem_a,
            "bonds_uniform": bonds_u,
            "bonds_adaptive": bonds_a,
            "adaptive_chis": list(adaptive_chis.values()),
        })

    return results


def run_experiment(n_qubits, chi_values, K=2.0, n_steps=8):
    """Run the full experiment for a given qubit count."""
    print(f"\n{'='*60}")
    print(f"  N = {n_qubits} qubits, K = {K}, steps = {n_steps}")
    print(f"{'='*60}")

    # Build circuit
    gates, omegas = build_qkr_circuit_gates(n_qubits, K=K, n_steps=n_steps)
    print(f"  Frequencies (sqrt primes): {[f'{w:.3f}' for w in omegas]}")
    print(f"  Total gates: {len(gates)}")
    print(f"  Entangling pairs: {n_qubits * (n_qubits - 1) // 2}")

    # Commensurability matrix
    print(f"\n  sin(C_ij/2) matrix (commensurability filter):")
    for i in range(n_qubits):
        row = []
        for j in range(n_qubits):
            if i == j:
                row.append("  --- ")
            else:
                row.append(f" {sin_c_half(omegas[i], omegas[j]):.3f}")
        print(f"    q{i}: {''.join(row)}")

    # Exact statevector
    print(f"\n  Computing exact statevector...", end=" ", flush=True)
    t0 = perf_counter()
    psi_exact = statevector_sim(n_qubits, gates)
    sv_time = perf_counter() - t0
    print(f"done ({sv_time:.2f}s)")

    # Adaptive bond analysis
    bonds, avg_chi, weights = compute_adaptive_bonds(omegas, max(chi_values))
    print(f"\n  Adaptive bond weights (sin(C/2) across each cut):")
    for i in range(n_qubits - 1):
        bar = "#" * int(weights[i] * 40)
        print(f"    bond {i}-{i+1}: {weights[i]:.4f}  chi={bonds[i]:3d}  {bar}")

    results = {"n_qubits": n_qubits, "omegas": omegas, "runs": []}

    for chi in chi_values:
        # Uniform MPS
        print(f"\n  --- chi = {chi} (uniform) ---")
        t0 = perf_counter()
        circ_uniform = mps_sim(n_qubits, gates, max_bond=chi)
        mps_time = perf_counter() - t0
        psi_mps = mps_to_dense(circ_uniform, n_qubits)
        f_uniform = fidelity(psi_exact, psi_mps)

        # Get actual max bond dimensions used
        bond_dims_uniform = [
            circ_uniform.psi[i].shape[-1]
            for i in range(n_qubits - 1)
        ]
        mem_uniform = sum(d ** 2 for d in bond_dims_uniform)

        print(f"    Fidelity:   {f_uniform:.8f}")
        print(f"    Bond dims:  {bond_dims_uniform}")
        print(f"    Memory:     {mem_uniform} (relative)")
        print(f"    Time:       {mps_time:.2f}s")

        # Adaptive MPS — use average adaptive chi as uniform chi
        # This gives the same total memory budget
        adaptive_bonds, avg, _ = compute_adaptive_bonds(omegas, chi)
        avg_chi_int = max(2, int(avg))

        print(f"\n  --- chi = {avg_chi_int} (adaptive-equivalent budget) ---")
        print(f"    (adaptive would use chi per bond: {list(adaptive_bonds.values())})")
        t0 = perf_counter()
        circ_adaptive = mps_sim(n_qubits, gates, max_bond=avg_chi_int)
        adapt_time = perf_counter() - t0
        psi_adapt = mps_to_dense(circ_adaptive, n_qubits)
        f_adaptive = fidelity(psi_exact, psi_adapt)

        bond_dims_adapt = [
            circ_adaptive.psi[i].shape[-1]
            for i in range(n_qubits - 1)
        ]
        mem_adaptive = sum(d ** 2 for d in bond_dims_adapt)

        print(f"    Fidelity:   {f_adaptive:.8f}")
        print(f"    Bond dims:  {bond_dims_adapt}")
        print(f"    Memory:     {mem_adaptive} (relative)")
        print(f"    Time:       {adapt_time:.2f}s")

        # The key question: at the SAME memory budget,
        # does adaptive allocation give better fidelity?
        delta_f = f_uniform - f_adaptive
        print(f"\n    >> Delta fidelity (uniform - budget-matched): {delta_f:+.8f}")
        if delta_f < 0:
            print(f"    >> ADAPTIVE WINS by {-delta_f:.2e}")
        else:
            print(f"    >> Uniform wins by {delta_f:.2e}")

        results["runs"].append({
            "chi": chi,
            "f_uniform": float(f_uniform),
            "f_adaptive_equiv": float(f_adaptive),
            "avg_chi_adaptive": avg_chi_int,
            "adaptive_bonds": list(adaptive_bonds.values()),
            "mem_uniform": mem_uniform,
            "mem_adaptive": mem_adaptive,
            "bond_dims_uniform": bond_dims_uniform,
        })

    return results


def run_qaoa_experiment(n_qubits=14, chi_values=[8, 16, 32, 64], p=2):
    """
    QAOA MaxCut on a graph with mixed structure:
    - A dense cluster (qubits 0-6, many edges → commensurate-like)
    - A sparse random part (qubits 7-13, few edges → different dynamics)
    - Some cross-edges between clusters

    The "frequencies" for sin(C/2) are derived from vertex degrees
    in the cost Hamiltonian: omega_i = degree(i).
    """
    print(f"\n{'='*60}")
    print(f"  QAOA MaxCut {n_qubits}q, p={p}")
    print(f"{'='*60}")

    # Build graph: dense cluster + sparse part + cross-edges
    np.random.seed(42)
    edges = []
    # Dense cluster: qubits 0-6 (high connectivity)
    half = n_qubits // 2
    for i in range(half):
        for j in range(i + 1, half):
            if np.random.random() < 0.7:
                edges.append((i, j))
    # Sparse chain: qubits 7-13
    for i in range(half, n_qubits - 1):
        edges.append((i, i + 1))
    # Cross-edges
    for _ in range(3):
        a = np.random.randint(0, half)
        b = np.random.randint(half, n_qubits)
        edges.append((a, b))

    print(f"  Edges: {len(edges)}")

    # --- Frequency extraction methods ---
    # Method: Graph Laplacian eigenvalues
    # L = D - A, eigenvalues capture the graph's natural oscillation modes.
    # Assign eigenvalue lambda_i to qubit i (sorted by qubit index).
    L = np.zeros((n_qubits, n_qubits))
    for i, j in edges:
        L[i, j] -= 1.0
        L[j, i] -= 1.0
        L[i, i] += 1.0
        L[j, j] += 1.0
    eigvals = np.sort(np.linalg.eigvalsh(L))
    # Skip the zero eigenvalue (trivial), shift rest to be positive
    omegas_laplacian = [max(0.01, float(v)) for v in eigvals]

    omegas = omegas_laplacian
    print(f"  Laplacian eigenvalues: {[f'{w:.3f}' for w in omegas]}")

    # Build QAOA circuit: p layers of (cost + mixer)
    gamma, beta = 0.7, 0.4
    gates = []
    # Initial superposition
    for i in range(n_qubits):
        gates.append(("RX", pi, (i,)))  # H = RX(pi) up to phase

    for layer in range(p):
        g = gamma * (1 + 0.3 * layer)  # vary slightly per layer
        b = beta * (1 - 0.1 * layer)
        # Cost unitary: ZZ for each edge
        for i, j in edges:
            gates.append(("ZZ", g, (i, j)))
        # Mixer: RX on each qubit
        for i in range(n_qubits):
            gates.append(("RX", 2 * b, (i,)))

    print(f"  Total gates: {len(gates)}")

    # sin(C/2) between vertex degrees
    print(f"\n  sin(C/2) sample (degree-based):")
    for i in range(min(n_qubits, 8)):
        row = []
        for j in range(min(n_qubits, 8)):
            if i == j:
                row.append("  --- ")
            else:
                row.append(f" {sin_c_half(omegas[i], omegas[j]):.3f}")
        print(f"    q{i}: {''.join(row)}")

    # Exact statevector
    print(f"\n  Computing exact statevector...", end=" ", flush=True)
    t0 = perf_counter()
    psi_exact = statevector_sim(n_qubits, gates)
    sv_time = perf_counter() - t0
    print(f"done ({sv_time:.2f}s)")

    # Per-bond weights
    bond_weights = {}
    for bond in range(n_qubits - 1):
        w = 0.0
        count = 0
        for a in range(bond + 1):
            for b in range(bond + 1, n_qubits):
                w += sin_c_half(omegas[a], omegas[b])
                count += 1
        bond_weights[bond] = w / count if count else 1.0

    print(f"\n  Per-bond sin(C/2) weight:")
    max_w = max(bond_weights.values()) if bond_weights else 1
    for bond in range(n_qubits - 1):
        w = bond_weights[bond]
        bar = "#" * int(w / max_w * 40) if max_w > 0 else ""
        print(f"    bond {bond:2d}-{bond+1:2d}: {w:.5f}  {bar}")

    results = {"label": f"QAOA MaxCut {n_qubits}q", "n_qubits": n_qubits, "runs": []}

    for chi_max in chi_values:
        # Uniform
        circ_u = mps_sim(n_qubits, gates, max_bond=chi_max)
        psi_u = mps_to_dense(circ_u, n_qubits)
        f_u = fidelity(psi_exact, psi_u)
        bonds_u = [circ_u.psi[i].shape[-1] for i in range(n_qubits - 1)]
        mem_u = sum(d * d for d in bonds_u)

        # Adaptive
        w_sum = sum(bond_weights.values())
        adaptive_chis = {}
        for bond in range(n_qubits - 1):
            frac = bond_weights[bond] / w_sum if w_sum > 1e-15 else 1.0 / (n_qubits - 1)
            chi_bond = max(2, int(chi_max * sqrt(frac * (n_qubits - 1))))
            adaptive_chis[bond] = chi_bond
        max_adaptive = max(adaptive_chis.values())
        circ_a = mps_sim(n_qubits, gates, max_bond=max_adaptive)
        psi_mps = circ_a.psi
        for bond in range(n_qubits - 1):
            psi_mps.compress_between(bond, bond + 1, max_bond=adaptive_chis[bond])
        psi_a = psi_mps.to_dense(["k" + str(i) for i in range(n_qubits)]).ravel()
        f_a = fidelity(psi_exact, psi_a)
        bonds_a = [psi_mps[i].shape[-1] for i in range(n_qubits - 1)]
        mem_a = sum(d * d for d in bonds_a)

        delta = f_a - f_u
        mem_ratio = mem_a / mem_u if mem_u > 0 else 0
        print(f"\n  chi={chi_max}: UNIFORM F={f_u:.6f} | ADAPTIVE F={f_a:.6f} | delta={delta:+.6f} mem={mem_ratio:.2f}")
        if delta > 0.001:
            print(f"    >> ADAPTIVE WINS by {delta:.4f}")
        elif delta < -0.001:
            print(f"    >> Uniform wins by {-delta:.4f}")

        results["runs"].append({
            "chi_max": chi_max, "f_uniform": float(f_u), "f_adaptive": float(f_a),
            "mem_uniform": mem_u, "mem_adaptive": mem_a, "delta": float(delta),
        })

    return results


if __name__ == "__main__":
    all_results = {}

    # ── QAOA MaxCut with Laplacian frequencies ──
    for n in [10, 14, 18]:
        result = run_qaoa_experiment(n_qubits=n, chi_values=[8, 16, 32, 64])
        all_results[f"qaoa_laplacian_{n}"] = result

    # Save results
    out_path = "experiments/tn_sinc_results.json"
    with open(out_path, "w") as f:
        json.dump(all_results, f, indent=2, default=str)
    print(f"\nResults saved to {out_path}")
