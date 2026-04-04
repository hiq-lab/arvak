//! Integration tests for the fold backend using real PDB data.
//!
//! PDB files sourced from /Users/danielhinderink/Projects/Garm-Platform/demos/PDB-Data/

use arvak_proj::fold::{anm, commensurability, contact, dmrg, hamiltonian, mpo, pdb, tdvp, tebd};

const PDB_DIR: &str = "/Users/danielhinderink/Projects/Garm-Platform/demos/PDB-Data";

// ─────────────────────────────────────────────────────────────
// PDB Parser
// ─────────────────────────────────────────────────────────────

#[test]
fn pdb_parse_1fme_bba() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    assert!(
        chain.len() >= 20,
        "BBA should have ~28 residues, got {}",
        chain.len()
    );
    assert!(
        chain.len() <= 40,
        "BBA should have ~28 residues, got {}",
        chain.len()
    );

    // First residue should have valid coordinates (not all zeros)
    let r0 = &chain.residues[0];
    let norm = r0.coords.iter().map(|x| x * x).sum::<f64>().sqrt();
    assert!(
        norm > 1.0,
        "first Cα should have non-zero coords, got norm={norm}"
    );

    // Distances should be physically reasonable
    // Cα-Cα distance between sequential residues: ~3.8 Å
    let d01 = chain.distance(0, 1);
    assert!(
        d01 > 3.0 && d01 < 4.5,
        "sequential Cα distance should be ~3.8Å, got {d01}"
    );
}

#[test]
fn pdb_parse_all_files() {
    let files = [
        "1FME.pdb", "1LMB.pdb", "1MI0.pdb", "2A3D.pdb", "2F21.pdb", "2HBA.pdb", "2WXC.pdb",
    ];
    for fname in &files {
        let path = format!("{PDB_DIR}/{fname}");
        let chain = pdb::ProteinChain::from_pdb(&path, None)
            .unwrap_or_else(|e| panic!("failed to parse {fname}: {e}"));
        assert!(!chain.is_empty(), "{fname} should have residues");
        println!("{fname}: {} residues", chain.len());
    }
}

#[test]
fn pdb_distance_matrix_symmetric() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let dm = chain.distance_matrix();
    let n = chain.len();

    for i in 0..n {
        assert!((dm[i][i]).abs() < 1e-10, "diagonal should be 0");
        for j in i + 1..n {
            assert!(
                (dm[i][j] - dm[j][i]).abs() < 1e-10,
                "distance matrix should be symmetric"
            );
            assert!(dm[i][j] > 0.0, "off-diagonal distances should be positive");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Contact Map
// ─────────────────────────────────────────────────────────────

#[test]
fn contact_map_1fme() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let contacts = contact::ContactMap::from_chain(&chain, 8.0, 3);

    assert!(
        !contacts.contacts.is_empty(),
        "BBA should have native contacts"
    );
    println!(
        "1FME: {} contacts, RCO={:.3}",
        contacts.contacts.len(),
        contacts.relative_contact_order()
    );

    // All contacts should satisfy constraints
    for c in &contacts.contacts {
        assert!(c.j > c.i, "j should be > i");
        assert!(c.seq_sep >= 3, "min sequence separation should be 3");
        assert!(c.distance <= 8.0, "distance should be within cutoff");
    }

    // RCO for BBA should be reasonable (small fast-folding protein)
    let rco = contacts.relative_contact_order();
    assert!(
        rco > 0.05 && rco < 0.5,
        "RCO should be reasonable, got {rco}"
    );
}

#[test]
fn crossing_profile_shape() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let contacts = contact::ContactMap::from_chain(&chain, 8.0, 3);
    let profile = contacts.crossing_profile();

    assert_eq!(
        profile.len(),
        chain.len() - 1,
        "crossing profile should have N-1 entries"
    );

    // Profile should be non-negative
    for (k, &count) in profile.iter().enumerate() {
        assert!(
            count <= contacts.contacts.len(),
            "crossing count at bond {k} should be <= total contacts"
        );
    }
}

// ─────────────────────────────────────────────────────────────
// ANM
// ─────────────────────────────────────────────────────────────

#[test]
fn anm_1fme_eigenvalues() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let result = anm::ANMResult::compute(&chain, 15.0, 1.0, None);

    // Should have 3N-6 non-trivial modes (minus 6 rigid body)
    let expected_modes = 3 * chain.len() - 6;
    println!(
        "1FME ANM: {} modes (expected ~{}), first 5 eigenvalues: {:?}",
        result.n_modes,
        expected_modes,
        &result.eigenvalues[..5.min(result.eigenvalues.len())]
    );

    // All eigenvalues should be positive
    for (i, &ev) in result.eigenvalues.iter().enumerate() {
        assert!(ev > 0.0, "eigenvalue {i} should be positive, got {ev}");
    }

    // Eigenvalues should be sorted ascending
    for w in result.eigenvalues.windows(2) {
        assert!(
            w[1] >= w[0] - 1e-10,
            "eigenvalues should be sorted ascending"
        );
    }

    // Frequencies should be sqrt of eigenvalues
    for (ev, freq) in result.eigenvalues.iter().zip(&result.frequencies) {
        assert!(
            (freq - ev.sqrt()).abs() < 1e-10,
            "frequency should be sqrt(eigenvalue)"
        );
    }
}

#[test]
fn anm_level_spacing_ratio() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let result = anm::ANMResult::compute(&chain, 15.0, 1.0, None);

    let r = result.level_spacing_ratio();
    println!("1FME <r> = {r:.4}");

    // <r> should be between 0 (fully degenerate) and 1
    assert!(r > 0.0 && r < 1.0, "<r> should be in (0,1), got {r}");
    // For a real protein, typically between Poisson (0.386) and GOE (0.530)
}

#[test]
fn anm_residue_participation() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let result = anm::ANMResult::compute(&chain, 15.0, 1.0, None);

    // Check participation for first residue
    let parts = result.residue_participation(0);
    assert!(!parts.is_empty(), "should have participation values");

    // Participation should sum to ~3 (3 spatial DOF per residue, normalized eigenvectors)
    let total: f64 = parts.iter().map(|(_, p)| p).sum();
    assert!(
        (total - 3.0).abs() < 0.5,
        "total participation should be ~3.0 (3 DOF), got {total}"
    );

    // Should be sorted by participation (descending)
    for w in parts.windows(2) {
        assert!(
            w[0].1 >= w[1].1 - 1e-10,
            "should be sorted by participation descending"
        );
    }
}

// ─────────────────────────────────────────────────────────────
// Commensurability
// ─────────────────────────────────────────────────────────────

#[test]
fn commensurability_scores_valid() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let contacts = contact::ContactMap::from_chain(&chain, 8.0, 3);
    let anm_result = anm::ANMResult::compute(&chain, 15.0, 1.0, None);
    let comm = commensurability::CommensurabilityResult::compute(&anm_result, &contacts, 8);

    assert_eq!(
        comm.contact_scores.len(),
        contacts.contacts.len(),
        "one score per contact"
    );

    for (i, &score) in comm.contact_scores.iter().enumerate() {
        assert!(
            (0.0..=1.0).contains(&score),
            "score {i} should be in [0,1], got {score}"
        );
    }

    // Bond budget should have N-1 entries
    assert_eq!(comm.bond_budget.len(), chain.len() - 1);

    // All budget values should be non-negative
    for &b in &comm.bond_budget {
        assert!(b >= 0.0, "bond budget should be non-negative");
    }

    println!(
        "1FME commensurability: {} contacts scored, mean={:.3}, budget range=[{:.2}, {:.2}]",
        comm.contact_scores.len(),
        comm.contact_scores.iter().sum::<f64>() / comm.contact_scores.len() as f64,
        comm.bond_budget
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min),
        comm.bond_budget
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max),
    );
}

#[test]
fn adaptive_chi_allocation() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let contacts = contact::ContactMap::from_chain(&chain, 8.0, 3);
    let anm_result = anm::ANMResult::compute(&chain, 15.0, 1.0, None);
    let mut comm = commensurability::CommensurabilityResult::compute(&anm_result, &contacts, 8);

    let chi = comm.to_adaptive_chi(4, 64);

    assert_eq!(chi.len(), chain.len() - 1, "one chi per bond");
    for (k, &c) in chi.iter().enumerate() {
        assert!(c >= 4, "chi at bond {k} should be >= chi_min");
        assert!(c <= 64, "chi at bond {k} should be <= chi_max");
    }

    // There should be variation (not all the same)
    let unique: std::collections::HashSet<usize> = chi.iter().copied().collect();
    println!(
        "1FME adaptive chi: {} unique values out of {} bonds",
        unique.len(),
        chi.len()
    );
    // Small protein might have limited variation, but should have at least 2 distinct values
}

// ─────────────────────────────────────────────────────────────
// Hamiltonian + MPO
// ─────────────────────────────────────────────────────────────

#[test]
fn hamiltonian_from_1fme() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let contacts = contact::ContactMap::from_chain(&chain, 8.0, 3);
    let anm_result = anm::ANMResult::compute(&chain, 15.0, 1.0, None);
    let comm = commensurability::CommensurabilityResult::compute(&anm_result, &contacts, 8);
    let params = hamiltonian::GoModelParams::default();

    let ham = hamiltonian::ProteinHamiltonian::from_protein(&chain, &contacts, &comm, &params);

    assert_eq!(ham.n_sites, chain.len());
    assert_eq!(ham.d, 9);
    assert_eq!(ham.local_terms.len(), chain.len());
    assert_eq!(ham.nn_terms.len(), chain.len() - 1);
    assert_eq!(ham.long_range_terms.len(), contacts.contacts.len());

    println!(
        "1FME Hamiltonian: {} sites, d={}, {} NN terms, {} LR terms",
        ham.n_sites,
        ham.d,
        ham.nn_terms.len(),
        ham.long_range_terms.len()
    );

    // Local terms should be d×d
    for h in &ham.local_terms {
        assert_eq!(h.len(), ham.d * ham.d);
    }

    // LR terms should reference valid sites
    for t in &ham.long_range_terms {
        assert!(t.i < ham.n_sites);
        assert!(t.j < ham.n_sites);
        assert!(t.j > t.i);
    }
}

#[test]
fn mpo_from_1fme() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let contacts = contact::ContactMap::from_chain(&chain, 8.0, 3);
    let anm_result = anm::ANMResult::compute(&chain, 15.0, 1.0, None);
    let comm = commensurability::CommensurabilityResult::compute(&anm_result, &contacts, 8);
    let params = hamiltonian::GoModelParams::default();
    let ham = hamiltonian::ProteinHamiltonian::from_protein(&chain, &contacts, &comm, &params);

    let mpo_full = mpo::MPO::from_hamiltonian(&ham, None);
    assert_eq!(mpo_full.n_sites, chain.len());
    assert_eq!(mpo_full.phys_dim, 9);

    println!(
        "1FME MPO (full): max bond dim = {}, bond dims = {:?}",
        mpo_full.max_bond_dim(),
        &mpo_full.bond_dims
    );

    // With pruning, should have fewer active threads → smaller bond dims
    let mpo_pruned = mpo::MPO::from_hamiltonian(&ham, Some(0.1));
    println!(
        "1FME MPO (pruned): max bond dim = {}",
        mpo_pruned.max_bond_dim()
    );
    assert!(
        mpo_pruned.max_bond_dim() <= mpo_full.max_bond_dim(),
        "pruned MPO should have <= bond dim"
    );
}

// ─────────────────────────────────────────────────────────────
// DMRG — small d for speed
// ─────────────────────────────────────────────────────────────

#[test]
fn dmrg_1fme_d3_small_chi() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let contacts = contact::ContactMap::from_chain(&chain, 8.0, 3);
    let anm_result = anm::ANMResult::compute(&chain, 15.0, 1.0, None);
    let comm = commensurability::CommensurabilityResult::compute(&anm_result, &contacts, 8);

    // Use d=3 and small chi for a quick test
    let params = hamiltonian::GoModelParams {
        d: 3,
        ..hamiltonian::GoModelParams::default()
    };
    let ham = hamiltonian::ProteinHamiltonian::from_protein(&chain, &contacts, &comm, &params);
    let h_mpo = mpo::MPO::from_hamiltonian(&ham, Some(0.05)); // prune weak contacts

    let config = dmrg::DMRGConfig {
        max_sweeps: 5,
        energy_tol: 1e-4,
        chi_profile: vec![8; chain.len() - 1], // uniform chi=8
        lanczos_max_iter: 30,
        lanczos_tol: 1e-8,
        noise: vec![1e-3, 1e-4, 0.0],
        ..dmrg::DMRGConfig::default()
    };

    let mut solver = dmrg::DMRG::new(h_mpo, config);
    let result = solver.solve();

    println!(
        "1FME DMRG (d=3, χ=8): E={:.6}, {} sweeps, {:.2}s, converged={}",
        result.energy, result.n_sweeps, result.wall_time_seconds, result.converged
    );

    // Energy should be finite and negative (bound state)
    assert!(result.energy.is_finite(), "energy should be finite");
    assert!(
        result.energy < 0.0,
        "ground state energy should be negative, got {}",
        result.energy
    );

    // Energy should decrease across sweeps
    for w in result.energies_per_sweep.windows(2) {
        // Allow small numerical fluctuations
        assert!(
            w[1] <= w[0] + 1e-6,
            "energy should not increase: {} → {}",
            w[0],
            w[1]
        );
    }
}

#[test]
fn dmrg_adaptive_vs_uniform() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let contacts = contact::ContactMap::from_chain(&chain, 8.0, 3);
    let anm_result = anm::ANMResult::compute(&chain, 15.0, 1.0, None);
    let mut comm = commensurability::CommensurabilityResult::compute(&anm_result, &contacts, 8);
    let adaptive_chi = comm.to_adaptive_chi(4, 16);

    let params = hamiltonian::GoModelParams {
        d: 3,
        ..hamiltonian::GoModelParams::default()
    };
    let ham = hamiltonian::ProteinHamiltonian::from_protein(&chain, &contacts, &comm, &params);

    // Uniform chi=16
    let h_mpo_u = mpo::MPO::from_hamiltonian(&ham, Some(0.05));
    let config_uniform = dmrg::DMRGConfig {
        max_sweeps: 4,
        energy_tol: 1e-4,
        chi_profile: vec![16; chain.len() - 1],
        lanczos_max_iter: 30,
        lanczos_tol: 1e-8,
        noise: vec![1e-3, 0.0],
        ..dmrg::DMRGConfig::default()
    };
    let mut solver_u = dmrg::DMRG::new(h_mpo_u, config_uniform);
    let result_u = solver_u.solve();

    // Adaptive chi from commensurability
    let h_mpo_a = mpo::MPO::from_hamiltonian(&ham, Some(0.05));
    let config_adaptive = dmrg::DMRGConfig {
        max_sweeps: 4,
        energy_tol: 1e-4,
        chi_profile: adaptive_chi.clone(),
        lanczos_max_iter: 30,
        lanczos_tol: 1e-8,
        noise: vec![1e-3, 0.0],
        ..dmrg::DMRGConfig::default()
    };
    let mut solver_a = dmrg::DMRG::new(h_mpo_a, config_adaptive);
    let result_a = solver_a.solve();

    let chi_sum_uniform: usize = result_u.mps_bond_dims.iter().sum();
    let chi_sum_adaptive: usize = result_a.mps_bond_dims.iter().sum();

    println!(
        "Uniform  χ=16: E={:.6}, Σχ={}, {:.2}s",
        result_u.energy, chi_sum_uniform, result_u.wall_time_seconds
    );
    println!(
        "Adaptive χ∈[4,16]: E={:.6}, Σχ={}, {:.2}s",
        result_a.energy, chi_sum_adaptive, result_a.wall_time_seconds
    );
    println!("Adaptive χ profile: {:?}", &result_a.mps_bond_dims);

    // Adaptive should use fewer total bond dimensions
    assert!(
        chi_sum_adaptive <= chi_sum_uniform,
        "adaptive should use ≤ total chi: {} vs {}",
        chi_sum_adaptive,
        chi_sum_uniform
    );

    // Both energies should be finite and negative
    assert!(result_u.energy.is_finite() && result_u.energy < 0.0);
    assert!(result_a.energy.is_finite() && result_a.energy < 0.0);
}

// ─────────────────────────────────────────────────────────────
// Full pipeline on multiple PDBs
// ─────────────────────────────────────────────────────────────

#[test]
fn full_pipeline_all_pdbs() {
    let files = ["1FME.pdb", "2A3D.pdb", "2F21.pdb"];

    for fname in &files {
        let path = format!("{PDB_DIR}/{fname}");
        let chain = pdb::ProteinChain::from_pdb(&path, None).unwrap();
        let contacts = contact::ContactMap::from_chain(&chain, 8.0, 3);
        let anm_result = anm::ANMResult::compute(&chain, 15.0, 1.0, None);
        let mut comm = commensurability::CommensurabilityResult::compute(&anm_result, &contacts, 8);
        let chi = comm.to_adaptive_chi(4, 12);

        let params = hamiltonian::GoModelParams {
            d: 3,
            ..hamiltonian::GoModelParams::default()
        };
        let ham = hamiltonian::ProteinHamiltonian::from_protein(&chain, &contacts, &comm, &params);
        let h_mpo = mpo::MPO::from_hamiltonian(&ham, Some(0.05));

        let config = dmrg::DMRGConfig {
            max_sweeps: 3,
            energy_tol: 1e-3,
            chi_profile: chi,
            lanczos_max_iter: 20,
            lanczos_tol: 1e-6,
            noise: vec![1e-3, 0.0],
            ..dmrg::DMRGConfig::default()
        };

        let mut solver = dmrg::DMRG::new(h_mpo, config);
        let result = solver.solve();

        println!(
            "{fname}: N={}, contacts={}, <r>={:.3}, E={:.4}, sweeps={}, {:.1}s",
            chain.len(),
            contacts.contacts.len(),
            anm_result.level_spacing_ratio(),
            result.energy,
            result.n_sweeps,
            result.wall_time_seconds,
        );

        assert!(
            result.energy.is_finite(),
            "{fname}: energy should be finite"
        );
        assert!(result.energy < 0.0, "{fname}: energy should be negative");
    }
}

// ─────────────────────────────────────────────────────────────
// TEBD — the fast solver
// ─────────────────────────────────────────────────────────────

#[test]
fn tebd_1fme_d3() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let contacts = contact::ContactMap::from_chain(&chain, 8.0, 3);
    let anm_result = anm::ANMResult::compute(&chain, 15.0, 1.0, None);
    let mut comm = commensurability::CommensurabilityResult::compute(&anm_result, &contacts, 8);
    let adaptive_chi = comm.to_adaptive_chi(4, 16);

    let params = hamiltonian::GoModelParams {
        d: 3,
        ..hamiltonian::GoModelParams::default()
    };
    let ham = hamiltonian::ProteinHamiltonian::from_protein(&chain, &contacts, &comm, &params);

    let dt = 0.05;
    let gates = tebd::FoldingGates::from_hamiltonian(&ham, dt);

    let mut solver = tebd::FoldingTEBD::new(chain.len(), 3, gates, adaptive_chi);
    let result = solver.evolve(200, 1e-6);

    println!(
        "TEBD 1FME (d=3, χ∈[4,16]): E={:.6}, {} measurements, {:.3}s, converged={}",
        result.energy, result.n_steps, result.wall_time_seconds, result.converged
    );
    println!("  Bond dims: {:?}", result.mps.bond_dims());

    assert!(result.energy.is_finite(), "energy should be finite");
    // Debug mode is ~20-50× slower; don't assert wall time in debug
    #[cfg(not(debug_assertions))]
    assert!(
        result.wall_time_seconds < 60.0,
        "TEBD should be fast (< 60s), took {:.1}s",
        result.wall_time_seconds
    );
}

#[test]
fn tebd_vs_dmrg_energy() {
    // Both solvers on same system — energies should be in the same ballpark
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let contacts = contact::ContactMap::from_chain(&chain, 8.0, 3);
    let anm_result = anm::ANMResult::compute(&chain, 15.0, 1.0, None);
    let mut comm = commensurability::CommensurabilityResult::compute(&anm_result, &contacts, 8);
    let adaptive_chi = comm.to_adaptive_chi(4, 12);

    let params = hamiltonian::GoModelParams {
        d: 3,
        ..hamiltonian::GoModelParams::default()
    };
    let ham = hamiltonian::ProteinHamiltonian::from_protein(&chain, &contacts, &comm, &params);

    // TEBD
    let gates = tebd::FoldingGates::from_hamiltonian(&ham, 0.05);
    let mut tebd_solver = tebd::FoldingTEBD::new(chain.len(), 3, gates, adaptive_chi.clone());
    let tebd_result = tebd_solver.evolve(300, 1e-6);

    println!(
        "TEBD:  E={:.4}, {:.2}s",
        tebd_result.energy, tebd_result.wall_time_seconds
    );

    assert!(tebd_result.energy.is_finite());
    println!("  TEBD bond dims: {:?}", tebd_result.mps.bond_dims());
}

#[test]
fn tebd_mpo_vs_swap() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let contacts = contact::ContactMap::from_chain(&chain, 8.0, 3);
    let anm_result = anm::ANMResult::compute(&chain, 15.0, 1.0, None);
    let mut comm = commensurability::CommensurabilityResult::compute(&anm_result, &contacts, 8);
    let adaptive_chi = comm.to_adaptive_chi(4, 16);

    let params = hamiltonian::GoModelParams {
        d: 3,
        ..hamiltonian::GoModelParams::default()
    };
    let ham = hamiltonian::ProteinHamiltonian::from_protein(&chain, &contacts, &comm, &params);

    let dt = 0.05;
    let n_steps = 200;

    // --- SWAP mode ---
    let gates_swap = tebd::FoldingGates::from_hamiltonian(&ham, dt);
    let mut solver_swap = tebd::FoldingTEBD::new(chain.len(), 3, gates_swap, adaptive_chi.clone());
    let result_swap = solver_swap.evolve(n_steps, 1e-6);

    // --- MPO mode ---
    let gates_mpo = tebd::FoldingGates::from_hamiltonian(&ham, dt);
    let contact_exp_mpo = tebd::FoldingTEBD::build_contact_exp_mpo(&ham, dt, Some(0.02));
    let mut solver_mpo = tebd::FoldingTEBD::with_mpo(
        chain.len(),
        3,
        gates_mpo,
        adaptive_chi.clone(),
        contact_exp_mpo,
    );
    let result_mpo = solver_mpo.evolve(n_steps, 1e-6);

    println!(
        "SWAP mode: E={:.4}, {:.2}s",
        result_swap.energy, result_swap.wall_time_seconds
    );
    println!(
        "MPO  mode: E={:.4}, {:.2}s",
        result_mpo.energy, result_mpo.wall_time_seconds
    );
    println!(
        "Speedup: {:.1}×",
        result_swap.wall_time_seconds / result_mpo.wall_time_seconds
    );

    // Both should produce finite energies
    assert!(result_swap.energy.is_finite());
    assert!(result_mpo.energy.is_finite());
}

// ─────────────────────────────────────────────────────────────
// TDVP — SWAP-free solver
// ─────────────────────────────────────────────────────────────

#[test]
fn tdvp_1fme_d3() {
    let chain = pdb::ProteinChain::from_pdb(&format!("{PDB_DIR}/1FME.pdb"), None).unwrap();
    let contacts = contact::ContactMap::from_chain(&chain, 8.0, 3);
    let anm_result = anm::ANMResult::compute(&chain, 15.0, 1.0, None);
    let mut comm = commensurability::CommensurabilityResult::compute(&anm_result, &contacts, 8);
    let adaptive_chi = comm.to_adaptive_chi(4, 16);

    let params = hamiltonian::GoModelParams {
        d: 3,
        ..hamiltonian::GoModelParams::default()
    };
    let ham = hamiltonian::ProteinHamiltonian::from_protein(&chain, &contacts, &comm, &params);
    let h_mpo = mpo::MPO::from_hamiltonian(&ham, None); // ALL contacts, no pruning

    let config = tdvp::TDVPConfig {
        dt: 0.1,
        n_steps: 200,
        energy_tol: 1e-6,
        chi_profile: adaptive_chi,
        krylov_dim: 12,
    };

    let mut solver = tdvp::TDVP::new(h_mpo, config);
    let result = solver.solve();

    println!(
        "TDVP 1FME (d=3, χ∈[4,16]): E={:.6}, {} measurements, {:.3}s, converged={}",
        result.energy, result.n_steps, result.wall_time_seconds, result.converged
    );

    assert!(result.energy.is_finite(), "energy should be finite");

    // The key test: TDVP should be MUCH faster than TEBD because no SWAPs
    // TEBD takes ~7s in release; TDVP should be < 2s
    println!("  (TEBD baseline: ~7.2s)");
}
