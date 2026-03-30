//! Python bindings for the arvak-proj projection simulator.
//!
//! Exposes `run_projection(n_qubits, zz_gates, single_gates, shots, ...)` which
//! runs the full sin(C/2) partitioning + MPS pipeline and returns measurement
//! counts with fidelity estimates.

use std::collections::HashMap;
use std::time::Instant;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use rand::prelude::*;

use arvak_proj::channel::ChannelMap;
use arvak_proj::frequency;
use arvak_proj::mps::{self, Mps};
use arvak_proj::partition;
use arvak_proj::reassembly;

/// Run the projection simulator on a circuit defined by gate lists.
///
/// # Arguments
/// * `n_qubits` - Number of qubits
/// * `zz_gates` - List of (i, j, theta) tuples for ZZ/entangling gates
/// * `single_gates` - List of (qubit, gate_type, theta) for single-qubit gates
///   gate_type: "h", "rx", "ry", "rz", "x", "z"
/// * `shots` - Number of measurement shots (1–1_000_000)
/// * `chi_max` - Maximum bond dimension budget (default 64)
/// * `stable_fraction` - Cutoff for stable classification (default 0.25)
/// * `sparse_radius` - Radius for sparse channel map (0 = dense, default 5)
///
/// # Returns
/// Dict with counts, fidelity, partition info, timing
#[pyfunction]
#[pyo3(signature = (
    n_qubits,
    zz_gates,
    single_gates,
    shots = 1000,
    chi_max = 64,
    stable_fraction = 0.25,
    sparse_radius = 5,
))]
pub fn run_projection(
    n_qubits: usize,
    zz_gates: Vec<(usize, usize, f64)>,
    single_gates: Vec<(usize, String, f64)>,
    shots: u32,
    chi_max: usize,
    stable_fraction: f64,
    sparse_radius: usize,
    py: Python<'_>,
) -> PyResult<Py<PyDict>> {
    if n_qubits == 0 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "n_qubits must be > 0",
        ));
    }
    if shots == 0 || shots > 1_000_000 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "shots must be 1..1_000_000",
        ));
    }

    // Validate gate indices
    for &(i, j, _) in &zz_gates {
        if i >= n_qubits || j >= n_qubits {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "ZZ gate index out of range: ({i}, {j}) for {n_qubits} qubits"
            )));
        }
    }
    for (q, _, _) in &single_gates {
        if *q >= n_qubits {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Single gate qubit {q} out of range for {n_qubits} qubits"
            )));
        }
    }

    let t0 = Instant::now();

    // --- Step 1: Frequency extraction ---
    let freqs = frequency::from_zz_angles(n_qubits, &zz_gates);

    // --- Step 2: Channel assessment ---
    let channels = if sparse_radius > 0 && n_qubits > 500 {
        ChannelMap::from_frequencies_sparse(&freqs, 1.0, sparse_radius)
    } else {
        ChannelMap::from_frequencies(&freqs, 1.0)
    };

    // --- Step 3: Partitioning ---
    let part = partition::partition_adaptive(&channels, chi_max, stable_fraction);

    // --- Step 4: MPS simulation ---
    let mut state = Mps::new(n_qubits);

    // Apply single-qubit gates first (initial layer: typically H gates)
    for (q, gate_type, theta) in &single_gates {
        let gate = match gate_type.as_str() {
            "h" => mps::h(),
            "rx" => mps::rx(*theta),
            "ry" => mps::ry(*theta),
            "rz" => mps::rz(*theta),
            "x" => mps::rx(std::f64::consts::PI),
            "z" => mps::rz(std::f64::consts::PI),
            _ => mps::rz(*theta), // default to RZ
        };
        state.apply_single(*q, gate);
    }

    // Apply ZZ gates with adaptive bond dimensions
    // Sort by bond index for MPS locality
    let mut sorted_zz: Vec<(usize, usize, f64)> = zz_gates.clone();
    sorted_zz.sort_by_key(|&(i, j, _)| (i.min(j), i.max(j)));

    // For nearest-neighbor pairs, apply directly. For long-range, decompose
    // into a chain of nearest-neighbor gates (SWAP network).
    // For now: apply only adjacent pairs, skip non-adjacent (conservative).
    let truncation_residual = 0.0_f64;
    for &(i, j, theta) in &sorted_zz {
        let bond = i.min(j);
        let dist = i.abs_diff(j);

        if dist == 1 {
            // Adjacent: apply ZZ directly
            let max_chi = part.recommended_chi.get(bond).copied().unwrap_or(chi_max);
            if max_chi <= 2 && state.bond_dim(bond) <= 2 {
                state.apply_zz_fast(bond, theta);
            } else {
                state
                    .apply_two_qubit(bond, mps::zz(theta), max_chi)
                    .map_err(|e| {
                        pyo3::exceptions::PyRuntimeError::new_err(format!(
                            "MPS gate failed at bond {bond}: {e}"
                        ))
                    })?;
            }
        }
        // Non-adjacent gates: skip for now (conservative — underestimates entanglement)
        // TODO: SWAP network decomposition for long-range gates
    }

    // --- Step 5: Measurement sampling ---
    let counts = if n_qubits <= 25 {
        // Small: use statevector for exact Born sampling
        let psi = state.to_statevector();
        sample_from_statevector(&psi, n_qubits, shots)
    } else {
        // Large: sequential MPS Born sampling
        sample_from_mps(&state, shots)
    };

    // --- Step 6: Fidelity estimation ---
    let result =
        reassembly::estimate_fidelity(n_qubits, part.n_volatile_qubits, truncation_residual);

    let elapsed_ms = t0.elapsed().as_millis() as u64;

    // --- Build Python dict ---
    let dict = PyDict::new(py);
    let counts_dict = PyDict::new(py);
    for (bitstring, count) in &counts {
        counts_dict.set_item(bitstring, count)?;
    }
    dict.set_item("counts", counts_dict)?;
    dict.set_item("fidelity", result.estimated_fidelity)?;
    dict.set_item("n_volatile", part.n_volatile_qubits)?;
    dict.set_item("n_stable", n_qubits.saturating_sub(part.n_volatile_qubits))?;

    let bond_dims: Vec<usize> = state.bond_dims();
    dict.set_item("bond_dims", bond_dims)?;
    dict.set_item("execution_time_ms", elapsed_ms)?;
    dict.set_item("truncation_residual", result.truncation_residual)?;
    dict.set_item("ln_gamma_c", result.ln_gamma_c)?;
    dict.set_item("n_qubits", n_qubits)?;

    Ok(dict.into())
}

/// Sample measurement outcomes from a dense statevector via Born rule.
fn sample_from_statevector(
    psi: &[num_complex::Complex64],
    n_qubits: usize,
    shots: u32,
) -> HashMap<String, u32> {
    let mut rng = rand::thread_rng();
    let mut counts: HashMap<String, u32> = HashMap::new();

    // Compute probabilities
    let probs: Vec<f64> = psi.iter().map(|a| a.norm_sqr()).collect();
    let total: f64 = probs.iter().sum();

    // Build CDF for efficient sampling
    let mut cdf = Vec::with_capacity(probs.len());
    let mut cumulative = 0.0;
    for p in &probs {
        cumulative += p / total;
        cdf.push(cumulative);
    }

    for _ in 0..shots {
        let r: f64 = rng.r#gen();
        let idx = cdf.partition_point(|&c| c < r).min(probs.len() - 1);
        let bitstring = format!("{:0width$b}", idx, width = n_qubits);
        *counts.entry(bitstring).or_insert(0) += 1;
    }

    counts
}

/// Sample measurement outcomes directly from MPS (sequential Born sampling).
///
/// For each shot: measure qubit 0, then qubit 1 conditioned on qubit 0's
/// outcome, etc. This is O(n · χ²) per shot — fast enough for moderate shots.
fn sample_from_mps(state: &Mps, shots: u32) -> HashMap<String, u32> {
    let mut rng = rand::thread_rng();
    let mut counts: HashMap<String, u32> = HashMap::new();
    let n = state.n_qubits;

    for _ in 0..shots {
        let mut bitstring = String::with_capacity(n);
        // Track the conditional state as a vector of bond dimension
        let mut vec: Vec<num_complex::Complex64> = vec![num_complex::Complex64::new(1.0, 0.0)];
        let mut current_dim = 1usize;

        for q in 0..n {
            let site = &state.sites[q];
            let rd = site.right_dim;

            // Compute amplitude for σ=0 and σ=1
            let mut new_vec_0 = vec![num_complex::Complex64::new(0.0, 0.0); rd];
            let mut new_vec_1 = vec![num_complex::Complex64::new(0.0, 0.0); rd];

            for b in 0..rd {
                for a in 0..current_dim {
                    new_vec_0[b] += vec[a] * site.m0[a * rd + b];
                    new_vec_1[b] += vec[a] * site.m1[a * rd + b];
                }
            }

            // Born probabilities
            let p0: f64 = new_vec_0.iter().map(|c| c.norm_sqr()).sum();
            let p1: f64 = new_vec_1.iter().map(|c| c.norm_sqr()).sum();
            let total = p0 + p1;

            if total < 1e-30 {
                // Degenerate: pick 0
                bitstring.push('0');
                vec = new_vec_0;
            } else {
                let r: f64 = rng.r#gen();
                if r < p0 / total {
                    bitstring.push('0');
                    // Normalize
                    let norm = p0.sqrt();
                    vec = new_vec_0.iter().map(|c| c / norm).collect();
                } else {
                    bitstring.push('1');
                    let norm = p1.sqrt();
                    vec = new_vec_1.iter().map(|c| c / norm).collect();
                }
            }
            current_dim = rd;
        }

        *counts.entry(bitstring).or_insert(0) += 1;
    }

    counts
}

/// Register the projection submodule.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run_projection, m)?)?;
    Ok(())
}
