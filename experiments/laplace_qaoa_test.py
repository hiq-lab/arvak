"""
Laplace-Spectrum Hypothesis: sin(C/2) on graph Laplacian eigenvalues
as bond dimension predictor for QAOA circuits.

Three test graphs (20 nodes, all unweighted):
  a) Erdős-Rényi G(20, 0.3)
  b) Barabási-Albert (m=2) — heterogeneous spectrum
  c) Regular ring (degree=4) — homogeneous spectrum

Expectation: BA shows adaptive advantage, ring does not.
"""

import numpy as np
from math import pi
from time import perf_counter
import quimb.tensor as qtn


# ── Graph generators ──────────────────────────────────────────────

def erdos_renyi(n, p, seed=42):
    rng = np.random.RandomState(seed)
    edges = []
    for i in range(n):
        for j in range(i + 1, n):
            if rng.random() < p:
                edges.append((i, j))
    return edges


def barabasi_albert(n, m, seed=42):
    rng = np.random.RandomState(seed)
    edges = []
    degrees = np.zeros(n, dtype=int)
    # Start with complete graph on m+1 nodes
    for i in range(m + 1):
        for j in range(i + 1, m + 1):
            edges.append((i, j))
            degrees[i] += 1
            degrees[j] += 1
    # Add remaining nodes with preferential attachment
    for new in range(m + 1, n):
        targets = set()
        while len(targets) < m:
            probs = degrees[:new] / degrees[:new].sum()
            t = rng.choice(new, p=probs)
            targets.add(t)
        for t in targets:
            edges.append((new, t))
            degrees[new] += 1
            degrees[t] += 1
    return edges


def regular_ring(n, k):
    """Ring graph where each node connects to k/2 neighbors on each side."""
    edges = []
    for i in range(n):
        for j in range(1, k // 2 + 1):
            edges.append((i, (i + j) % n))
    return edges


# ── Laplace spectrum ──────────────────────────────────────────────

def graph_laplacian_eigenvalues(n, edges):
    L = np.zeros((n, n))
    for i, j in edges:
        L[i, j] -= 1.0
        L[j, i] -= 1.0
        L[i, i] += 1.0
        L[j, j] += 1.0
    eigvals = np.sort(np.linalg.eigvalsh(L))
    return np.maximum(eigvals, 0.01)  # clamp zero eigenvalue


# ── sin(C/2) analysis ────────────────────────────────────────────

def commensurability_residual(oi, oj, q_max=4):
    if abs(oj) < 1e-15 or abs(oi) < 1e-15:
        return pi
    r = oi / oj
    candidates = [abs(r - round(r * q) / q) for q in range(1, q_max + 1) if round(r * q) > 0]
    return min(candidates) if candidates else pi


def sin_c_half(oi, oj, q_max=4):
    return abs(np.sin(commensurability_residual(oi, oj, q_max) / 2))


def bond_weights_from_freqs(freqs):
    n = len(freqs)
    weights = []
    for bond in range(n - 1):
        total = wsum = 0.0
        for a in range(bond + 1):
            for b in range(bond + 1, n):
                d = (bond - a) + (b - bond - 1)
                w = np.exp(-0.5 * d)
                total += sin_c_half(freqs[a], freqs[b]) * w
                wsum += w
        weights.append(total / wsum if wsum > 1e-15 else 0.0)
    return weights


def adaptive_chi(weights, chi_max):
    ws = sum(weights)
    n = len(weights)
    if ws < 1e-15:
        return [chi_max] * n
    return [max(2, int(chi_max * np.sqrt(w / ws * n))) for w in weights]


# ── QAOA circuit on quimb MPS ────────────────────────────────────

def run_qaoa_mps(n, edges, max_bond, p_layers=5, gamma=0.7, beta=0.4):
    circ = qtn.CircuitMPS(n, max_bond=max_bond)
    # Initial superposition
    for i in range(n):
        circ.h(i)
    # QAOA layers
    for layer in range(p_layers):
        g = gamma * (1 + 0.15 * layer)
        b = beta * (1 - 0.05 * layer)
        # Cost: ZZ for ALL edges (including non-NN — quimb handles SWAP insertion)
        for i, j in edges:
            circ.rzz(2 * g, min(i, j), max(i, j))
        # Mixer: RX
        for i in range(n):
            circ.rx(2 * b, i)
    return circ


def run_qaoa_exact(n, edges, p_layers=5, gamma=0.7, beta=0.4):
    """Exact statevector via quimb with huge chi."""
    return run_qaoa_mps(n, edges, max_bond=1024, p_layers=p_layers,
                        gamma=gamma, beta=beta)


def fidelity(psi_a, psi_b):
    a = psi_a / np.linalg.norm(psi_a)
    b = psi_b / np.linalg.norm(psi_b)
    return abs(np.vdot(a, b)) ** 2


# ══════════════════════════════════════════════════════════════════
#  MAIN EXPERIMENT
# ══════════════════════════════════════════════════════════════════

N = 20
GRAPHS = {
    "Erdős-Rényi G(20,0.3)": erdos_renyi(N, 0.3),
    "Barabási-Albert (m=2)": barabasi_albert(N, 2),
    "Regular Ring (k=4)": regular_ring(N, 4),
}

print("=" * 80)
print("  LAPLACE-SPECTRUM HYPOTHESIS: sin(C/2) on graph eigenvalues for QAOA")
print("=" * 80)

results = {}

for name, edges in GRAPHS.items():
    print(f"\n{'─'*70}")
    print(f"  {name}")
    print(f"  {N} nodes, {len(edges)} edges")

    # ── 1. Laplace spectrum ──
    freqs = graph_laplacian_eigenvalues(N, edges)
    print(f"  Laplace eigenvalues: {freqs.round(3)}")
    print(f"  Spectral range: {freqs.min():.3f} - {freqs.max():.3f} ({freqs.max()/freqs.min():.1f}x)")

    # ── 2. sin(C/2) analysis ──
    all_sinc = []
    for i in range(N):
        for j in range(i + 1, N):
            all_sinc.append(sin_c_half(freqs[i], freqs[j]))
    H_var = np.var(all_sinc)
    H_mean = np.mean(all_sinc)
    print(f"  sin(C/2): mean={H_mean:.5f}, var={H_var:.2e}, std/mean={np.std(all_sinc)/H_mean:.3f}")

    # Bond weights
    bw = bond_weights_from_freqs(freqs)
    print(f"  Bond weight range: {min(bw):.5f} - {max(bw):.5f} ({max(bw)/max(min(bw),1e-10):.1f}x)")

    # ── 3. Ground truth (exact) ──
    t0 = perf_counter()
    circ_exact = run_qaoa_exact(N, edges)
    psi_exact = circ_exact.psi.to_dense(["k" + str(i) for i in range(N)]).ravel()
    dt_exact = perf_counter() - t0
    print(f"  Exact simulation: {dt_exact:.2f}s")

    # ── 4. Compare uniform vs adaptive ──
    print(f"\n  {'chi':>5} | {'F_uniform':>10} | {'F_adaptive':>10} | {'delta':>10} | {'adapt_range':>12}")
    print(f"  {'-'*58}")

    graph_results = []
    for chi_max in [4, 8, 16]:
        # Uniform
        circ_u = run_qaoa_mps(N, edges, max_bond=chi_max)
        psi_u = circ_u.psi.to_dense(["k" + str(i) for i in range(N)]).ravel()
        f_u = fidelity(psi_exact, psi_u)

        # Adaptive (Laplace-weighted)
        ad = adaptive_chi(bw, chi_max)
        circ_a = run_qaoa_mps(N, edges, max_bond=max(ad))
        for b in range(N - 1):
            circ_a.psi.compress_between(b, b + 1, max_bond=ad[b])
        psi_a = circ_a.psi.to_dense(["k" + str(i) for i in range(N)]).ravel()
        f_a = fidelity(psi_exact, psi_a)

        delta = f_a - f_u
        marker = " <<" if delta > 0.005 else (" !!" if delta < -0.005 else "")
        print(f"  {chi_max:>5} | {f_u:>10.6f} | {f_a:>10.6f} | {delta:>+10.6f} | {min(ad)}-{max(ad)}{marker}")

        graph_results.append({
            "chi": chi_max, "f_u": f_u, "f_a": f_a, "delta": delta
        })

    results[name] = {
        "edges": len(edges),
        "spectral_range": float(freqs.max() / freqs.min()),
        "sinc_variance": float(H_var),
        "sinc_heterogeneity": float(np.std(all_sinc) / H_mean),
        "bond_weight_range": float(max(bw) / max(min(bw), 1e-10)),
        "fidelity": graph_results,
    }

# ── Summary ──
print(f"\n{'='*80}")
print(f"  SUMMARY")
print(f"{'='*80}")
print(f"  {'Graph':>28} | {'Spectral':>8} | {'H(sinC)':>10} | {'BW range':>8} | {'Δ@chi=8':>8}")
print(f"  {'-'*72}")
for name, r in results.items():
    d8 = [x for x in r["fidelity"] if x["chi"] == 8][0]["delta"]
    print(f"  {name:>28} | {r['spectral_range']:>7.1f}x | {r['sinc_heterogeneity']:>10.4f} | {r['bond_weight_range']:>7.1f}x | {d8:>+8.4f}")

print(f"\n  Interpretation:")
for name, r in results.items():
    d8 = [x for x in r["fidelity"] if x["chi"] == 8][0]["delta"]
    if d8 > 0.005:
        print(f"    {name}: ADAPTIVE WINS (Δ={d8:+.4f}) — heterogeneous spectrum")
    elif d8 < -0.005:
        print(f"    {name}: Uniform wins (Δ={d8:+.4f}) — homogeneous spectrum")
    else:
        print(f"    {name}: ~Equal (Δ={d8:+.4f})")
