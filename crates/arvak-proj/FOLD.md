# arvak-proj `fold` — Tensor-Network Protein Folding

Protein folding simulation using Matrix Product States with adaptive bond dimension from sin(C/2) commensurability analysis. Part of the Arvak quantum compilation stack.

## What it does

Takes a UniProt ID (or PDB file), builds a coarse-grained Go-model Hamiltonian, encodes it as a Matrix Product Operator (MPO), and finds the ground state via TDVP — the same algorithm used in condensed matter physics for strongly correlated quantum systems, applied here to the protein folding energy landscape.

The key innovation from Arvak: **sin(C/2) commensurability analysis** allocates bond dimension adaptively. Contacts between residues whose vibrational modes are resonant get high precision; incommensurable contacts get compressed. This is the same technique that gives Arvak's quantum circuit simulator its 135x speedup on Barabasi-Albert topologies.

```rust
use arvak_proj::fold::{alphafold, hamiltonian, mpo, tdvp};

// UniProt ID → ground state in <1 second
let entry = alphafold::AlphaFoldEntry::fetch("P01308")?; // Insulin
let ham = entry.build_hamiltonian(&hamiltonian::GoModelParams::default(), 5.0);
let chi = entry.adaptive_chi(4, 64);
let h_mpo = mpo::MPO::from_hamiltonian(&ham, None);
let mut solver = tdvp::TDVP::new(h_mpo, tdvp::TDVPConfig {
    chi_profile: chi,
    ..Default::default()
});
let result = solver.solve();
```

## Architecture

```
                  ┌──────────────────────────────────────────┐
                  │           Input Sources                    │
                  │  UniProt ID ──→ AlphaFold API (pLDDT+PAE) │
                  │  PDB file  ──→ Cα parser                  │
                  └────────────────────┬─────────────────────┘
                                       │
               ┌───────────────────────▼────────────────────────┐
               │            Analysis Layer                       │
               │  ContactMap ← PAE matrix or distance cutoff     │
               │  ANM        ← Hessian eigendecomposition        │
               │  sin(C/2)   ← mode frequency ratios             │
               │  pLDDT      ← AlphaFold confidence (shortcut)   │
               └───────────────────────┬────────────────────────┘
                                       │
               ┌───────────────────────▼────────────────────────┐
               │          Hamiltonian + MPO                      │
               │  Go-model: local (Ramachandran) + backbone      │
               │            + long-range contacts (PAE/sin(C/2)) │
               │  MPO: Finite State Automaton construction       │
               │       bond dim w = 2 + active contacts per bond │
               └───────────────────────┬────────────────────────┘
                                       │
               ┌───────────────────────▼────────────────────────┐
               │              Solvers                            │
               │  TDVP  ← primary (0.85s, no SWAPs)            │
               │  TEBD  ← fallback (7.2s, SWAP networks)       │
               │  DMRG  ← baseline (1017s, ground state)       │
               └───────────────────────┬────────────────────────┘
                                       │
               ┌───────────────────────▼────────────────────────┐
               │          Output                                 │
               │  Ground state MPS → energy, bond dimensions     │
               │  Trajectory analysis → folding intermediates    │
               │  MPO → directly usable as VQE ansatz on QPU     │
               └────────────────────────────────────────────────┘
```

## Modules

| Module | File | Purpose |
|--------|------|---------|
| `alphafold` | `alphafold.rs` | Fetch structures + PAE + pLDDT from AlphaFold EBI API |
| `pdb` | `pdb.rs` | Minimal Cα-only PDB parser |
| `contact` | `contact.rs` | Native contact map, crossing profile, relative contact order |
| `anm` | `anm.rs` | Anisotropic Network Model (Hessian eigendecomposition) |
| `commensurability` | `commensurability.rs` | sin(C/2) scoring → adaptive bond dimension allocation |
| `hamiltonian` | `hamiltonian.rs` | Go-model: local + backbone + long-range contact terms |
| `mpo` | `mpo.rs` | MPO construction via Finite State Automaton |
| `tdvp` | `tdvp.rs` | 1-site TDVP solver (Haegeman et al. 2016) |
| `tebd` | `tebd.rs` | Imaginary-time TEBD with SWAP networks (fallback) |
| `dmrg` | `dmrg.rs` | Two-site DMRG ground state solver (baseline) |
| `trajectory` | `trajectory.rs` | Folding pathway analysis, intermediate detection |

## Performance

Benchmarked on 1FME (BBA protein, 28 residues, 42 native contacts, d=3, chi in [4,16]):

| Solver | Wall time (release) | Speedup |
|--------|-------------------|---------|
| DMRG | 1017 s | 1x |
| TEBD (SWAP, uniform chi) | 11.8 s | 86x |
| TEBD (SWAP, adaptive chi) | 7.2 s | 141x |
| **TDVP (no SWAP)** | **0.85 s** | **1196x** |

AlphaFold pipeline (Insulin P01308, 110 residues, 75 contacts):

| Step | Time |
|------|------|
| AlphaFold API fetch | ~1 s |
| Hamiltonian + MPO build | < 1 ms |
| TDVP solve | 37 ms |
| **Total** | **~1 s** |

## Why TDVP, not TEBD

TEBD applies two-site gates sequentially. Long-range contacts require SWAP networks to bring distant residues adjacent — each contact costs O(separation) SVD operations. For 42 contacts with average separation ~10: **840 SVDs per Trotter step**.

TDVP never moves sites. It sweeps through the chain, solving a local effective Hamiltonian at each site. The MPO bond indices carry all long-range information through environment blocks. For 28 sites: **28 Krylov exponentials per sweep**. No SWAPs, no Trotter error.

Reference: Haegeman, Lubich, Oseledets, Vandereycken, Verstraete. *Unifying time evolution and optimization with matrix product states.* Phys. Rev. B 94, 165116 (2016).

## Two input paths

### Path 1: AlphaFold (recommended)

```rust
let entry = AlphaFoldEntry::fetch("P04637")?; // any UniProt ID
let ham = entry.build_hamiltonian(&params, 5.0);
let chi = entry.adaptive_chi(4, 64);
```

- No experimental structure needed — works for all 214M UniProt proteins
- PAE matrix replaces binary contact cutoff (continuous confidence)
- pLDDT replaces ANM eigendecomposition for chi allocation (O(N) vs O(N^3))
- Coupling strength J_ij derived from PAE + pLDDT

### Path 2: PDB + ANM (full physics)

```rust
let chain = ProteinChain::from_pdb("1FME.pdb", None)?;
let contacts = ContactMap::from_chain(&chain, 8.0, 3);
let anm = ANMResult::compute(&chain, 15.0, 1.0, None);
let comm = CommensurabilityResult::compute(&anm, &contacts, 8);
let chi = comm.to_adaptive_chi(4, 64);
```

- Uses ANM mode frequency ratios for sin(C/2) commensurability
- Captures dynamical resonances that pLDDT cannot see
- Recommended for precision runs (Stage 3 in screening pipeline)

## Connection to quantum hardware

The MPO constructed by this module is directly usable as a variational ansatz for VQE on quantum hardware via the Arvak compilation stack. The adaptive chi profile becomes the circuit depth profile: bonds needing high chi become deep subcircuits, bonds needing low chi become shallow.

This is the core value proposition: the same commensurability analysis that makes **classical** MPS simulation efficient also makes **quantum** simulation efficient, because both are limited by entanglement at the same bonds.

## Tests

25 unit + integration tests, all passing in release mode.

```bash
cargo test -p arvak-proj --release -- fold::
```

Integration tests use real PDB data from `/demos/PDB-Data/` (1FME, 2A3D, 2F21) and live AlphaFold API calls (Insulin P01308).

## Stats

- 12 source files, 4338 lines of Rust
- 762 lines of integration tests
- Dependencies: nalgebra (eigendecomposition), faer (SVD), serde_json (AlphaFold API)
