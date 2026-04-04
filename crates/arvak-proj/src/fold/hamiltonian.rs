/// Local Hilbert space dimension per residue.
/// d=3: coil/helix/sheet
/// d=9: 3×3 Ramachandran grid (default)
pub const D_DEFAULT: usize = 9;

pub struct GoModelParams {
    pub d: usize,
    pub backbone_coupling: f64,
    pub contact_strength: f64,
    pub local_field: f64,
    pub transverse_coupling: f64,
}

impl Default for GoModelParams {
    fn default() -> Self {
        Self {
            d: D_DEFAULT,
            backbone_coupling: 1.0,
            contact_strength: 0.5,
            local_field: 0.1,
            transverse_coupling: 0.3,
        }
    }
}

pub struct LongRangeTerm {
    pub i: usize,
    pub j: usize,
    pub op_left: Vec<f64>,
    pub op_right: Vec<f64>,
    pub strength: f64,
}

pub struct ProteinHamiltonian {
    pub n_sites: usize,
    pub d: usize,
    pub local_terms: Vec<Vec<f64>>,
    pub nn_terms: Vec<Vec<f64>>,
    pub long_range_terms: Vec<LongRangeTerm>,
}

impl ProteinHamiltonian {
    pub fn from_protein(
        chain: &super::pdb::ProteinChain,
        contacts: &super::contact::ContactMap,
        comm: &super::commensurability::CommensurabilityResult,
        params: &GoModelParams,
    ) -> Self {
        let n = chain.len();
        let d = params.d;

        // Local terms: diagonal Ramachandran preference + transverse tunneling
        let mut local_terms = Vec::with_capacity(n);
        for _site in 0..n {
            let mut h = vec![0.0; d * d];
            // Diagonal: energy levels. State 0 = unfolded (high), others = partially folded
            for s in 0..d {
                let d_minus_1 = if d > 1 { (d - 1) as f64 } else { 1.0 };
                h[s * d + s] = params.local_field * (s as f64) / d_minus_1;
            }
            // Off-diagonal: transverse field (tunneling between conformational states)
            for s in 0..d.saturating_sub(1) {
                h[s * d + (s + 1)] = -params.transverse_coupling;
                h[(s + 1) * d + s] = -params.transverse_coupling;
            }
            local_terms.push(h);
        }

        // Nearest-neighbor terms: backbone cooperative folding (Ising-like)
        let mut nn_terms = Vec::with_capacity(n.saturating_sub(1));
        for _bond in 0..n.saturating_sub(1) {
            let d2 = d * d;
            let mut h_nn = vec![0.0; d2 * d2];
            for si in 0..d {
                for sj in 0..d {
                    let row = si * d + sj;
                    if si == sj {
                        h_nn[row * d2 + row] = -params.backbone_coupling;
                    }
                }
            }
            nn_terms.push(h_nn);
        }

        // Long-range terms from native contacts
        let d_ref = 8.0; // reference distance in Å
        let mut long_range_terms = Vec::new();
        for (idx, contact) in contacts.contacts.iter().enumerate() {
            let c_score = comm.contact_scores.get(idx).copied().unwrap_or(0.5);
            let denom = contact.distance / d_ref;
            let strength = if denom.abs() > 1e-15 {
                params.contact_strength * c_score / denom
            } else {
                params.contact_strength * c_score
            };

            // Projection onto native-like states (non-coil)
            let mut op_left = vec![0.0; d * d];
            for s in 0..d {
                op_left[s * d + s] = if s > 0 { 1.0 } else { 0.0 };
            }
            let op_right = op_left.clone();

            long_range_terms.push(LongRangeTerm {
                i: contact.i,
                j: contact.j,
                op_left,
                op_right,
                strength,
            });
        }

        ProteinHamiltonian {
            n_sites: n,
            d,
            local_terms,
            nn_terms,
            long_range_terms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_params_are_sane() {
        let p = GoModelParams::default();
        assert_eq!(p.d, D_DEFAULT);
        assert!(p.backbone_coupling > 0.0);
        assert!(p.contact_strength > 0.0);
    }

    #[test]
    fn local_term_is_hermitian() {
        let params = GoModelParams {
            d: 3,
            ..GoModelParams::default()
        };
        let d = params.d;
        // Build a single local term manually
        let mut h = vec![0.0; d * d];
        for s in 0..d {
            let d_minus_1 = if d > 1 { (d - 1) as f64 } else { 1.0 };
            h[s * d + s] = params.local_field * (s as f64) / d_minus_1;
        }
        for s in 0..d.saturating_sub(1) {
            h[s * d + (s + 1)] = -params.transverse_coupling;
            h[(s + 1) * d + s] = -params.transverse_coupling;
        }
        // Check symmetry (real Hermitian = symmetric)
        for i in 0..d {
            for j in 0..d {
                assert!(
                    (h[i * d + j] - h[j * d + i]).abs() < 1e-15,
                    "h[{i},{j}] != h[{j},{i}]"
                );
            }
        }
    }
}
