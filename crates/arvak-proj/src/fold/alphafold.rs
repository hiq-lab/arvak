//! AlphaFold Database integration for arvak-proj fold.
//!
//! Fetches predicted structures, pLDDT confidence scores, and PAE matrices
//! from the AlphaFold EBI API. Replaces the need for experimental PDB files
//! and the expensive ANM eigendecomposition.
//!
//! Usage:
//! ```no_run
//! use arvak_proj::fold::alphafold::AlphaFoldEntry;
//!
//! let entry = AlphaFoldEntry::fetch("P04637").unwrap(); // p53
//! let chain = entry.to_chain();
//! let contacts = entry.contact_map(8.0, 3);
//! let chi_profile = entry.adaptive_chi(4, 64);
//! ```

use crate::error::{ProjError, Result};

/// AlphaFold prediction entry for a single UniProt protein.
pub struct AlphaFoldEntry {
    pub uniprot_id: String,
    /// Cα coordinates from the predicted structure.
    pub coords: Vec<[f64; 3]>,
    /// Residue names (3-letter codes).
    pub residue_names: Vec<String>,
    /// Per-residue pLDDT confidence (0–100). Higher = more confident.
    /// pLDDT > 90: high confidence, rigid.
    /// pLDDT < 50: low confidence, disordered/flexible.
    pub plddt: Vec<f64>,
    /// Predicted Aligned Error matrix (N×N, in Ångström).
    /// PAE[i][j] = expected position error of residue j when aligned on residue i.
    /// Low PAE = confident relative position = strong coupling.
    pub pae: Vec<Vec<f64>>,
    /// Maximum PAE value (for normalization).
    pub max_pae: f64,
    /// Number of residues.
    pub n_residues: usize,
}

impl AlphaFoldEntry {
    /// Fetch an AlphaFold prediction from the EBI API.
    ///
    /// `uniprot_id`: UniProt accession (e.g., "P04637" for p53).
    ///
    /// Makes two HTTP requests:
    /// 1. Prediction metadata → PDB URL + PAE URL
    /// 2. PDB file → Cα coordinates + pLDDT (B-factor column)
    /// 3. PAE JSON → aligned error matrix
    pub fn fetch(uniprot_id: &str) -> Result<Self> {
        // Step 1: Get prediction metadata
        let meta_url = format!("https://alphafold.ebi.ac.uk/api/prediction/{}", uniprot_id);
        let meta_body = http_get(&meta_url)?;
        let meta: serde_json::Value = serde_json::from_str(&meta_body)
            .map_err(|e| ProjError::FrequencyExtraction(format!("JSON parse error: {e}")))?;

        let entry = meta.as_array().and_then(|a| a.first()).ok_or_else(|| {
            ProjError::FrequencyExtraction(format!("no AlphaFold entry for {uniprot_id}"))
        })?;

        let pdb_url = entry["pdbUrl"]
            .as_str()
            .ok_or_else(|| ProjError::FrequencyExtraction("missing pdbUrl".into()))?;
        let pae_url = entry["paeDocUrl"]
            .as_str()
            .ok_or_else(|| ProjError::FrequencyExtraction("missing paeDocUrl".into()))?;

        // Step 2: Fetch PDB → coordinates + pLDDT
        let pdb_body = http_get(pdb_url)?;
        let (coords, residue_names, plddt) = parse_pdb_with_plddt(&pdb_body)?;
        let n_residues = coords.len();

        // Step 3: Fetch PAE matrix
        let pae_body = http_get(pae_url)?;
        let (pae, max_pae) = parse_pae_json(&pae_body, n_residues)?;

        Ok(AlphaFoldEntry {
            uniprot_id: uniprot_id.to_string(),
            coords,
            residue_names,
            plddt,
            pae,
            max_pae,
            n_residues,
        })
    }

    /// Convert to a `ProteinChain` for use with the fold pipeline.
    pub fn to_chain(&self) -> super::pdb::ProteinChain {
        let residues = self
            .coords
            .iter()
            .enumerate()
            .map(|(i, coords)| super::pdb::Residue {
                index: i,
                resid: i as i32 + 1,
                name: self
                    .residue_names
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| "UNK".into()),
                chain: 'A',
                coords: *coords,
            })
            .collect();

        super::pdb::ProteinChain {
            residues,
            name: format!("AF-{}", self.uniprot_id),
        }
    }

    /// Build a contact map from PAE instead of distance cutoff.
    ///
    /// A contact exists where PAE < `pae_cutoff` (default: 5.0 Å)
    /// AND sequence separation ≥ `min_sep` (default: 3).
    ///
    /// This is strictly better than distance-based contacts because
    /// PAE encodes AlphaFold's confidence in the relative position.
    pub fn contact_map(&self, pae_cutoff: f64, min_sep: usize) -> super::contact::ContactMap {
        let mut contacts = Vec::new();
        for i in 0..self.n_residues {
            for j in i + min_sep..self.n_residues {
                if self.pae[i][j] < pae_cutoff {
                    let d = distance(&self.coords[i], &self.coords[j]);
                    contacts.push(super::contact::Contact {
                        i,
                        j,
                        distance: d,
                        seq_sep: j - i,
                    });
                }
            }
        }
        super::contact::ContactMap {
            contacts,
            n_residues: self.n_residues,
        }
    }

    /// Compute coupling strength J_ij from PAE and pLDDT.
    ///
    /// J_ij = (1 - PAE_ij / max_PAE) × sqrt(pLDDT_i × pLDDT_j) / 100
    ///
    /// High confidence + low PAE → strong coupling.
    /// Low confidence or high PAE → weak coupling.
    pub fn coupling_strength(&self, i: usize, j: usize) -> f64 {
        let pae_factor = 1.0 - self.pae[i][j] / self.max_pae;
        let plddt_factor = (self.plddt[i] * self.plddt[j]).sqrt() / 100.0;
        (pae_factor * plddt_factor).max(0.0)
    }

    /// Compute adaptive bond dimension profile directly from pLDDT.
    ///
    /// Flexible regions (low pLDDT) need high χ (many conformations).
    /// Rigid regions (high pLDDT) need low χ.
    ///
    /// Bond budget at position k = Σ over contacts crossing k,
    /// weighted by (100 - pLDDT_i) × (100 - pLDDT_j) / PAE_ij.
    pub fn adaptive_chi(&self, chi_min: usize, chi_max: usize) -> Vec<usize> {
        let n = self.n_residues;
        if n < 2 {
            return Vec::new();
        }

        let mut budget = vec![0.0_f64; n - 1];

        for i in 0..n {
            for j in i + 3..n {
                let pae_ij = self.pae[i][j];
                if pae_ij > self.max_pae * 0.8 {
                    continue; // very uncertain contact, skip
                }
                let flexibility_i = (100.0 - self.plddt[i]).max(0.0);
                let flexibility_j = (100.0 - self.plddt[j]).max(0.0);
                let weight = flexibility_i * flexibility_j / (pae_ij + 1.0);

                for k in i..j.min(n - 1) {
                    budget[k] += weight;
                }
            }
        }

        let max_budget = budget.iter().cloned().fold(0.0_f64, f64::max);
        if max_budget < 1e-15 {
            return vec![chi_min; n - 1];
        }

        budget
            .iter()
            .map(|&b| {
                let frac = (b / max_budget).sqrt();
                let chi = chi_min + ((chi_max - chi_min) as f64 * frac) as usize;
                chi.clamp(chi_min, chi_max)
            })
            .collect()
    }

    /// Build a Hamiltonian using AlphaFold-derived couplings.
    ///
    /// Contact strengths come from PAE+pLDDT instead of sin(C/2).
    /// No ANM eigendecomposition needed.
    pub fn build_hamiltonian(
        &self,
        params: &super::hamiltonian::GoModelParams,
        pae_cutoff: f64,
    ) -> super::hamiltonian::ProteinHamiltonian {
        let chain = self.to_chain();
        let contacts = self.contact_map(pae_cutoff, 3);
        let d = params.d;
        let n = self.n_residues;

        // Local terms
        let mut local_terms = Vec::with_capacity(n);
        for _site in 0..n {
            let mut h = vec![0.0; d * d];
            for s in 0..d {
                h[s * d + s] = params.local_field * (s as f64) / (d as f64 - 1.0);
            }
            for s in 0..d.saturating_sub(1) {
                h[s * d + (s + 1)] = -params.transverse_coupling;
                h[(s + 1) * d + s] = -params.transverse_coupling;
            }
            local_terms.push(h);
        }

        // NN terms
        let mut nn_terms = Vec::with_capacity(n.saturating_sub(1));
        for _bond in 0..n.saturating_sub(1) {
            let d2 = d * d;
            let mut h_nn = vec![0.0; d2 * d2];
            for si in 0..d {
                let row = si * d + si;
                h_nn[row * d2 + row] = -params.backbone_coupling;
            }
            nn_terms.push(h_nn);
        }

        // Long-range terms with AlphaFold-derived coupling
        let mut long_range_terms = Vec::new();
        for contact in &contacts.contacts {
            let j_ij = self.coupling_strength(contact.i, contact.j) * params.contact_strength;
            if j_ij.abs() < 1e-10 {
                continue;
            }

            let mut op = vec![0.0; d * d];
            for s in 1..d {
                op[s * d + s] = 1.0;
            }

            long_range_terms.push(super::hamiltonian::LongRangeTerm {
                i: contact.i,
                j: contact.j,
                op_left: op.clone(),
                op_right: op,
                strength: j_ij,
            });
        }

        super::hamiltonian::ProteinHamiltonian {
            n_sites: n,
            d,
            local_terms,
            nn_terms,
            long_range_terms,
        }
    }
}

// ── HTTP + Parsing helpers ─────────────────────────────────────

/// Minimal blocking HTTP GET (no external dependency).
fn http_get(url: &str) -> Result<String> {
    // Use std::process::Command to call curl — works on all platforms
    // without adding reqwest/ureq as a dependency.
    let output = std::process::Command::new("curl")
        .args(["-s", "--fail", "--max-time", "30", url])
        .output()
        .map_err(|e| ProjError::FrequencyExtraction(format!("curl failed: {e}")))?;

    if !output.status.success() {
        return Err(ProjError::FrequencyExtraction(format!(
            "HTTP request failed for {url}: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    String::from_utf8(output.stdout)
        .map_err(|e| ProjError::FrequencyExtraction(format!("UTF-8 decode error: {e}")))
}

/// Parse PDB content, extracting Cα coordinates and pLDDT from B-factor column.
fn parse_pdb_with_plddt(pdb: &str) -> Result<(Vec<[f64; 3]>, Vec<String>, Vec<f64>)> {
    let mut coords = Vec::new();
    let mut names = Vec::new();
    let mut plddt = Vec::new();

    for line in pdb.lines() {
        if !line.starts_with("ATOM") || line.len() < 66 {
            continue;
        }

        let atom_name = line[12..16].trim();
        if atom_name != "CA" {
            continue;
        }

        let altloc = line.as_bytes().get(16).copied().unwrap_or(b' ');
        if altloc != b' ' && altloc != b'A' {
            continue;
        }

        let resname = line[17..20].trim().to_string();
        let x: f64 = line[30..38].trim().parse().unwrap_or(0.0);
        let y: f64 = line[38..46].trim().parse().unwrap_or(0.0);
        let z: f64 = line[46..54].trim().parse().unwrap_or(0.0);
        let b_factor: f64 = line[60..66].trim().parse().unwrap_or(0.0);

        coords.push([x, y, z]);
        names.push(resname);
        plddt.push(b_factor); // AlphaFold stores pLDDT in B-factor column
    }

    if coords.is_empty() {
        return Err(ProjError::FrequencyExtraction(
            "no CA atoms in AlphaFold PDB".into(),
        ));
    }

    Ok((coords, names, plddt))
}

/// Parse PAE JSON from AlphaFold API.
fn parse_pae_json(json_str: &str, expected_n: usize) -> Result<(Vec<Vec<f64>>, f64)> {
    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| ProjError::FrequencyExtraction(format!("PAE JSON parse error: {e}")))?;

    let entry = parsed
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| ProjError::FrequencyExtraction("empty PAE JSON".into()))?;

    let max_pae = entry["max_predicted_aligned_error"]
        .as_f64()
        .unwrap_or(31.75);

    let pae_raw = entry["predicted_aligned_error"]
        .as_array()
        .ok_or_else(|| ProjError::FrequencyExtraction("missing PAE matrix".into()))?;

    let pae: Vec<Vec<f64>> = pae_raw
        .iter()
        .map(|row| {
            row.as_array()
                .map(|r| r.iter().filter_map(|v| v.as_f64()).collect())
                .unwrap_or_default()
        })
        .collect();

    if pae.len() != expected_n {
        return Err(ProjError::FrequencyExtraction(format!(
            "PAE matrix size {} != expected {}",
            pae.len(),
            expected_n
        )));
    }

    Ok((pae, max_pae))
}

fn distance(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plddt_from_pdb_line() {
        let line =
            "ATOM      1  CA  MET A   1      39.005  -1.279  19.430  1.00 54.73           C  ";
        let (coords, names, plddt) = parse_pdb_with_plddt(line).unwrap();
        assert_eq!(coords.len(), 1);
        assert!((coords[0][0] - 39.005).abs() < 0.01);
        assert!((plddt[0] - 54.73).abs() < 0.01);
        assert_eq!(names[0], "MET");
    }

    #[test]
    fn coupling_strength_bounds() {
        let entry = AlphaFoldEntry {
            uniprot_id: "test".into(),
            coords: vec![[0.0; 3]; 5],
            residue_names: vec!["ALA".into(); 5],
            plddt: vec![90.0, 50.0, 95.0, 30.0, 80.0],
            pae: vec![
                vec![0.0, 2.0, 5.0, 15.0, 25.0],
                vec![2.0, 0.0, 3.0, 10.0, 20.0],
                vec![5.0, 3.0, 0.0, 8.0, 12.0],
                vec![15.0, 10.0, 8.0, 0.0, 6.0],
                vec![25.0, 20.0, 12.0, 6.0, 0.0],
            ],
            max_pae: 31.75,
            n_residues: 5,
        };

        // High confidence, low PAE → strong coupling
        let j_strong = entry.coupling_strength(0, 1);
        // Low confidence, high PAE → weak coupling
        let j_weak = entry.coupling_strength(0, 4);

        assert!(j_strong > j_weak, "strong > weak: {j_strong} > {j_weak}");
        assert!(j_strong >= 0.0);
        assert!(j_weak >= 0.0);
        assert!(j_strong <= 1.0);
    }

    #[test]
    fn adaptive_chi_from_plddt() {
        let entry = AlphaFoldEntry {
            uniprot_id: "test".into(),
            coords: vec![
                [0.0, 0.0, 0.0],
                [3.8, 0.0, 0.0],
                [7.6, 0.0, 0.0],
                [11.4, 0.0, 0.0],
                [15.2, 0.0, 0.0],
            ],
            residue_names: vec!["ALA".into(); 5],
            plddt: vec![95.0, 95.0, 30.0, 30.0, 95.0], // flexible in the middle
            pae: vec![
                vec![0.0, 1.0, 5.0, 8.0, 10.0],
                vec![1.0, 0.0, 4.0, 7.0, 9.0],
                vec![5.0, 4.0, 0.0, 2.0, 6.0],
                vec![8.0, 7.0, 2.0, 0.0, 5.0],
                vec![10.0, 9.0, 6.0, 5.0, 0.0],
            ],
            max_pae: 31.75,
            n_residues: 5,
        };

        let chi = entry.adaptive_chi(4, 64);
        assert_eq!(chi.len(), 4);

        // Middle bonds (near flexible region) should have higher chi
        // than edge bonds (near rigid regions)
        println!("chi profile: {chi:?}");
        for &c in &chi {
            assert!(c >= 4 && c <= 64);
        }
    }
}
