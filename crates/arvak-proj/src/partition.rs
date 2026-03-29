//! Partitioning: classify bonds as stable (KAM tori) or volatile (chaos).

use crate::channel::ChannelMap;

/// Classification of an MPS bond.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondClass {
    /// Stable: low entanglement flow, χ ≈ 1-4 sufficient.
    Stable,
    /// Volatile: significant entanglement, needs full simulation.
    Volatile,
}

/// Result of partitioning: per-bond classification + recommended χ.
#[derive(Debug, Clone)]
pub struct Partition {
    pub bond_classes: Vec<BondClass>,
    pub recommended_chi: Vec<usize>,
    pub n_volatile_bonds: usize,
    pub n_volatile_qubits: usize,
}

/// Partition bonds based on adaptive χ from sin(C/2) channel weights.
///
/// Bonds where the recommended adaptive χ ≤ `stable_chi_cutoff` are
/// classified as stable (KAM tori). This is a relative criterion:
/// it compares each bond's recommended χ against its peers, not
/// against an absolute threshold.
///
/// Default `stable_chi_cutoff`: bonds that need ≤ 25% of chi_max
/// are considered stable.
#[must_use]
pub fn partition_adaptive(
    channels: &ChannelMap,
    chi_max: usize,
    stable_fraction: f64,
) -> Partition {
    let n = channels.n_qubits();
    if n < 2 {
        return Partition {
            bond_classes: vec![],
            recommended_chi: vec![],
            n_volatile_bonds: 0,
            n_volatile_qubits: 0,
        };
    }

    let adaptive = channels.adaptive_bond_dims(chi_max);
    let stable_chi_cutoff = ((chi_max as f64) * stable_fraction).max(2.0) as usize;

    let mut classes = Vec::with_capacity(n - 1);
    let mut rec_chi = Vec::with_capacity(n - 1);

    for &chi in &adaptive {
        if chi <= stable_chi_cutoff {
            classes.push(BondClass::Stable);
            rec_chi.push(2);
        } else {
            classes.push(BondClass::Volatile);
            rec_chi.push(chi);
        }
    }

    let n_volatile_bonds = classes
        .iter()
        .filter(|&&c| c == BondClass::Volatile)
        .count();

    let mut volatile_qubit = vec![false; n];
    for (bond, &class) in classes.iter().enumerate() {
        if class == BondClass::Volatile {
            volatile_qubit[bond] = true;
            volatile_qubit[bond + 1] = true;
        }
    }
    let n_volatile_qubits = volatile_qubit.iter().filter(|&&v| v).count();

    Partition {
        bond_classes: classes,
        recommended_chi: rec_chi,
        n_volatile_bonds,
        n_volatile_qubits,
    }
}

/// Legacy: partition by absolute weight threshold.
#[must_use]
pub fn partition_by_threshold(channels: &ChannelMap, chi_max: usize, threshold: f64) -> Partition {
    let n = channels.n_qubits();
    if n < 2 {
        return Partition {
            bond_classes: vec![],
            recommended_chi: vec![],
            n_volatile_bonds: 0,
            n_volatile_qubits: 0,
        };
    }

    let adaptive = channels.adaptive_bond_dims(chi_max);
    let mut classes = Vec::with_capacity(n - 1);
    let mut rec_chi = Vec::with_capacity(n - 1);

    for (bond, &chi) in adaptive.iter().enumerate() {
        let w = channels.bond_weight(bond);
        if w < threshold {
            classes.push(BondClass::Stable);
            rec_chi.push(2);
        } else {
            classes.push(BondClass::Volatile);
            rec_chi.push(chi);
        }
    }

    let n_volatile_bonds = classes
        .iter()
        .filter(|&&c| c == BondClass::Volatile)
        .count();

    let mut volatile_qubit = vec![false; n];
    for (bond, &class) in classes.iter().enumerate() {
        if class == BondClass::Volatile {
            volatile_qubit[bond] = true;
            volatile_qubit[bond + 1] = true;
        }
    }
    let n_volatile_qubits = volatile_qubit.iter().filter(|&&v| v).count();

    Partition {
        bond_classes: classes,
        recommended_chi: rec_chi,
        n_volatile_bonds,
        n_volatile_qubits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commensurate_system_mostly_stable() {
        // All integer frequencies → mostly commensurate
        let cm = ChannelMap::from_frequencies(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 1.0);
        let part = partition_by_threshold(&cm, 32, 0.005);
        // Most bonds should be stable for integer frequencies
        assert!(
            part.n_volatile_bonds <= 3,
            "expected few volatile bonds, got {}",
            part.n_volatile_bonds
        );
    }

    #[test]
    fn mixed_system_has_volatile_region() {
        let freqs: Vec<f64> = vec![
            1.0,
            2.0,
            3.0, // commensurate
            7.0_f64.sqrt(),
            11.0_f64.sqrt(),
            13.0_f64.sqrt(), // irrational
        ];
        let cm = ChannelMap::from_frequencies(&freqs, 1.0);
        let part = partition_by_threshold(&cm, 32, 0.001);
        assert!(
            part.n_volatile_bonds > 0,
            "mixed system should have volatile bonds"
        );
        assert!(part.n_volatile_qubits >= 2, "should have volatile qubits");
    }
}
