pub struct Contact {
    pub i: usize,
    pub j: usize,
    pub distance: f64,
    pub seq_sep: usize,
}

pub struct ContactMap {
    pub contacts: Vec<Contact>,
    pub n_residues: usize,
}

impl ContactMap {
    pub fn from_chain(chain: &super::pdb::ProteinChain, cutoff: f64, min_sep: usize) -> Self {
        let n = chain.len();
        let mut contacts = Vec::new();
        for i in 0..n {
            for j in i + min_sep..n {
                let d = chain.distance(i, j);
                if d <= cutoff {
                    contacts.push(Contact {
                        i,
                        j,
                        distance: d,
                        seq_sep: j - i,
                    });
                }
            }
        }
        ContactMap {
            contacts,
            n_residues: n,
        }
    }

    pub fn contacts_crossing_bond(&self, k: usize) -> Vec<&Contact> {
        self.contacts
            .iter()
            .filter(|c| c.i <= k && c.j > k)
            .collect()
    }

    pub fn crossing_profile(&self) -> Vec<usize> {
        let mut profile = vec![0usize; self.n_residues.saturating_sub(1)];
        for c in &self.contacts {
            for k in c.i..c.j.min(profile.len()) {
                profile[k] += 1;
            }
        }
        profile
    }

    pub fn relative_contact_order(&self) -> f64 {
        if self.contacts.is_empty() || self.n_residues == 0 {
            return 0.0;
        }
        let total_sep: f64 = self.contacts.iter().map(|c| c.seq_sep as f64).sum();
        total_sep / (self.contacts.len() as f64 * self.n_residues as f64)
    }
}
