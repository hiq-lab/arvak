pub struct CommensurabilityResult {
    pub contact_scores: Vec<f64>,
    pub bond_budget: Vec<f64>,
    pub adaptive_chi: Vec<usize>,
}

impl CommensurabilityResult {
    pub fn compute(
        anm: &super::anm::ANMResult,
        contacts: &super::contact::ContactMap,
        n_local_modes: usize,
    ) -> Self {
        let mut contact_scores = Vec::with_capacity(contacts.contacts.len());

        for contact in &contacts.contacts {
            let freqs_i = anm.local_frequencies(contact.i, n_local_modes);
            let freqs_j = anm.local_frequencies(contact.j, n_local_modes);
            let parts_i = anm.residue_participation(contact.i);
            let parts_j = anm.residue_participation(contact.j);

            let mut score = 0.0;
            let mut norm = 0.0;

            for (a, &fi) in freqs_i.iter().enumerate() {
                let pi = parts_i.get(a).map_or(0.0, |(_, p)| *p);
                for (b, &fj) in freqs_j.iter().enumerate() {
                    let pj = parts_j.get(b).map_or(0.0, |(_, p)| *p);
                    let w = pi * pj;
                    if fi > 1e-10 && fj > 1e-10 {
                        let ratio = fi / fj;
                        let frac = ratio - ratio.floor();
                        let sin_val = (std::f64::consts::PI * frac).sin().abs();
                        score += w * (1.0 - sin_val);
                    }
                    norm += w;
                }
            }

            let c = if norm > 1e-15 { score / norm } else { 0.0 };
            contact_scores.push(c.clamp(0.0, 1.0));
        }

        // Bond budget: sum of commensurability scores crossing each bond
        let n = contacts.n_residues;
        let mut bond_budget = vec![0.0; n.saturating_sub(1)];
        for (idx, contact) in contacts.contacts.iter().enumerate() {
            for k in contact.i..contact.j.min(bond_budget.len()) {
                bond_budget[k] += contact_scores[idx];
            }
        }

        CommensurabilityResult {
            contact_scores,
            bond_budget,
            adaptive_chi: Vec::new(), // filled by to_adaptive_chi
        }
    }

    pub fn to_adaptive_chi(&mut self, chi_min: usize, chi_max: usize) -> Vec<usize> {
        if self.bond_budget.is_empty() {
            return Vec::new();
        }

        let max_budget = self.bond_budget.iter().copied().fold(0.0_f64, f64::max);
        if max_budget < 1e-15 {
            self.adaptive_chi = vec![chi_min; self.bond_budget.len()];
            return self.adaptive_chi.clone();
        }

        self.adaptive_chi = self
            .bond_budget
            .iter()
            .map(|&b| {
                let frac = (b / max_budget).sqrt();
                let chi = chi_min + ((chi_max - chi_min) as f64 * frac) as usize;
                chi.clamp(chi_min, chi_max)
            })
            .collect();

        self.adaptive_chi.clone()
    }
}
