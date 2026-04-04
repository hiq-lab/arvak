use super::anm::ANMResult;
use super::commensurability::CommensurabilityResult;
use super::contact::ContactMap;
use super::pdb::ProteinChain;

/// A sequence of protein conformations along a folding pathway.
pub struct FoldingTrajectory {
    pub frames: Vec<ProteinChain>,
    pub energies: Vec<f64>, // Energy per frame (can be empty)
}

/// Analysis results for a folding trajectory.
pub struct TrajectoryAnalysis {
    /// Level spacing ratio <r> at each frame.
    /// Poisson ~ 0.386 (integrable/localized), GOE ~ 0.530 (chaotic/delocalized)
    pub r_ratios: Vec<f64>,
    /// Mean contact commensurability at each frame
    pub mean_commensurability: Vec<f64>,
    /// Bond entanglement budget profile at each frame
    pub bond_budgets: Vec<Vec<f64>>,
    /// Indices of frames identified as folding intermediates
    pub intermediate_indices: Vec<usize>,
    /// Number of native contacts at each frame
    pub n_contacts: Vec<usize>,
    /// Relative contact order at each frame
    pub contact_orders: Vec<f64>,
}

impl TrajectoryAnalysis {
    /// Analyze a folding trajectory.
    ///
    /// For each frame:
    /// 1. Build contact map
    /// 2. Compute ANM -> frequencies -> level spacing ratio <r>
    /// 3. Compute commensurability scores for contacts
    /// 4. Compute bond budget profile
    ///
    /// Intermediates identified where <r> shows a local minimum
    /// (transition from GOE-like to Poisson-like = folding barrier).
    pub fn analyze(
        trajectory: &FoldingTrajectory,
        anm_cutoff: f64,
        contact_cutoff: f64,
        contact_min_sep: usize,
    ) -> Self {
        let n_frames = trajectory.frames.len();
        let mut r_ratios = Vec::with_capacity(n_frames);
        let mut mean_comm = Vec::with_capacity(n_frames);
        let mut bond_budgets = Vec::with_capacity(n_frames);
        let mut n_contacts = Vec::with_capacity(n_frames);
        let mut contact_orders = Vec::with_capacity(n_frames);

        for frame in &trajectory.frames {
            // Contact map
            let contacts = ContactMap::from_chain(frame, contact_cutoff, contact_min_sep);
            n_contacts.push(contacts.contacts.len());
            contact_orders.push(contacts.relative_contact_order());

            // ANM
            let anm = ANMResult::compute(frame, anm_cutoff, 1.0, None);
            r_ratios.push(anm.level_spacing_ratio());

            // Commensurability
            let comm = CommensurabilityResult::compute(&anm, &contacts, 8);

            let mean_c = if comm.contact_scores.is_empty() {
                0.0
            } else {
                comm.contact_scores.iter().sum::<f64>() / comm.contact_scores.len() as f64
            };
            mean_comm.push(mean_c);
            bond_budgets.push(comm.bond_budget);
        }

        // Identify intermediates: local minima in <r>
        let intermediate_indices = find_local_minima(&r_ratios, 0.01);

        TrajectoryAnalysis {
            r_ratios,
            mean_commensurability: mean_comm,
            bond_budgets,
            intermediate_indices,
            n_contacts,
            contact_orders,
        }
    }

    /// Identify "druggable" intermediates: frames where <r> dips (folding barrier)
    /// AND commensurability variance is high (structural rearrangement -> new pockets).
    pub fn druggable_intermediates(&self) -> Vec<usize> {
        if self.intermediate_indices.is_empty() {
            return Vec::new();
        }

        // Compute commensurability variance at each frame
        let comm_var: Vec<f64> = self
            .bond_budgets
            .iter()
            .map(|bb| {
                if bb.is_empty() {
                    return 0.0;
                }
                let mean = bb.iter().sum::<f64>() / bb.len() as f64;
                bb.iter().map(|&b| (b - mean).powi(2)).sum::<f64>() / bb.len() as f64
            })
            .collect();

        // Median variance
        let mut sorted_var = comm_var.clone();
        sorted_var.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median_var = sorted_var.get(sorted_var.len() / 2).copied().unwrap_or(0.0);

        // Druggable = intermediate with above-median variance
        self.intermediate_indices
            .iter()
            .filter(|&&idx| comm_var.get(idx).copied().unwrap_or(0.0) > median_var)
            .copied()
            .collect()
    }
}

/// Find local minima in a signal, ignoring fluctuations smaller than `min_depth`.
fn find_local_minima(signal: &[f64], min_depth: f64) -> Vec<usize> {
    if signal.len() < 3 {
        return Vec::new();
    }

    let mut minima = Vec::new();
    for i in 1..signal.len() - 1 {
        if signal[i] < signal[i - 1] && signal[i] < signal[i + 1] {
            // Check depth: must be at least min_depth below both neighbors
            let depth = (signal[i - 1] - signal[i]).min(signal[i + 1] - signal[i]);
            if depth >= min_depth {
                minima.push(i);
            }
        }
    }
    minima
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_local_minima() {
        let signal = vec![1.0, 0.5, 0.8, 0.3, 0.9, 0.7, 0.2, 0.6];
        let minima = find_local_minima(&signal, 0.05);
        assert!(minima.contains(&1)); // 0.5 is local min
        assert!(minima.contains(&3)); // 0.3 is local min
        assert!(minima.contains(&6)); // 0.2 is local min
    }

    #[test]
    fn test_find_local_minima_empty() {
        assert!(find_local_minima(&[], 0.01).is_empty());
        assert!(find_local_minima(&[1.0], 0.01).is_empty());
        assert!(find_local_minima(&[1.0, 2.0], 0.01).is_empty());
    }

    #[test]
    fn test_find_local_minima_flat() {
        let signal = vec![1.0, 1.0, 1.0, 1.0];
        assert!(find_local_minima(&signal, 0.01).is_empty());
    }
}
