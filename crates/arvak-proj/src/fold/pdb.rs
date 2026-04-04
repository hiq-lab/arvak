use crate::error::{ProjError, Result};

pub struct Residue {
    pub index: usize,
    pub resid: i32,
    pub name: String,
    pub chain: char,
    pub coords: [f64; 3],
}

pub struct ProteinChain {
    pub residues: Vec<Residue>,
    pub name: String,
}

impl ProteinChain {
    pub fn from_pdb(path: &str, chain: Option<char>) -> Result<Self> {
        // Parse ATOM records, filter CA only, first MODEL only
        // If chain is Some, filter to that chain; else take first chain found
        // PDB ATOM format: columns 1-6=record, 13-16=atom name, 17=altloc,
        //   18-20=resname, 22=chain, 23-26=resseq, 31-38=x, 39-46=y, 47-54=z
        let content = std::fs::read_to_string(path)
            .map_err(|e| ProjError::FrequencyExtraction(format!("cannot read PDB: {e}")))?;

        let mut residues = Vec::new();
        let mut found_chain = None;
        let mut in_first_model = true;

        for line in content.lines() {
            if line.starts_with("ENDMDL") {
                in_first_model = false;
            }
            if !in_first_model {
                continue;
            }
            if !line.starts_with("ATOM") || line.len() < 54 {
                continue;
            }

            let atom_name = line[12..16].trim();
            if atom_name != "CA" {
                continue;
            }

            // Skip alternate conformations (take 'A' or ' ')
            let altloc = line.as_bytes().get(16).copied().unwrap_or(b' ');
            if altloc != b' ' && altloc != b'A' {
                continue;
            }

            let ch = line.as_bytes().get(21).map_or('A', |&b| b as char);

            if let Some(target) = chain {
                if ch != target {
                    continue;
                }
            } else {
                if found_chain.is_none() {
                    found_chain = Some(ch);
                }
                if Some(ch) != found_chain {
                    continue;
                }
            }

            let resname = line[17..20].trim().to_string();
            let resid: i32 = line[22..26].trim().parse().unwrap_or(0);
            let x: f64 = line[30..38].trim().parse().unwrap_or(0.0);
            let y: f64 = line[38..46].trim().parse().unwrap_or(0.0);
            let z: f64 = line[46..54].trim().parse().unwrap_or(0.0);

            residues.push(Residue {
                index: residues.len(),
                resid,
                name: resname,
                chain: ch,
                coords: [x, y, z],
            });
        }

        if residues.is_empty() {
            return Err(ProjError::FrequencyExtraction(
                "no CA atoms found in PDB".into(),
            ));
        }

        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        Ok(ProteinChain { residues, name })
    }

    pub fn len(&self) -> usize {
        self.residues.len()
    }

    pub fn is_empty(&self) -> bool {
        self.residues.is_empty()
    }

    pub fn distance(&self, i: usize, j: usize) -> f64 {
        let a = &self.residues[i].coords;
        let b = &self.residues[j].coords;
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }

    #[allow(clippy::needless_range_loop)]
    pub fn distance_matrix(&self) -> Vec<Vec<f64>> {
        let n = self.len();
        let mut dm = vec![vec![0.0; n]; n];
        for i in 0..n {
            for j in i + 1..n {
                let d = self.distance(i, j);
                dm[i][j] = d;
                dm[j][i] = d;
            }
        }
        dm
    }
}
