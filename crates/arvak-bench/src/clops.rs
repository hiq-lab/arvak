//! CLOPS (Circuit Layer Operations Per Second) benchmark.
//!
//! Measures end-to-end throughput of a quantum system including
//! circuit parameterization, compilation, submission, and result retrieval.
//!
//! CLOPS = (`num_templates` * `num_updates` * `num_qubits` * depth) / `total_time`

use std::f64::consts::PI;
use std::time::Instant;

use arvak_compile::{PassManager, PropertySet};
use arvak_ir::{Circuit, QubitId};
use rand::{Rng, SeedableRng};

use crate::BenchmarkResult;

/// Configuration for a CLOPS benchmark.
#[derive(Debug, Clone)]
pub struct ClopsConfig {
    /// Number of qubits in the circuit.
    pub num_qubits: u32,
    /// Depth (number of layers) of the circuit.
    pub depth: u32,
    /// Number of circuit templates.
    pub num_templates: u32,
    /// Number of parameter updates per template.
    pub num_updates: u32,
}

impl Default for ClopsConfig {
    fn default() -> Self {
        Self {
            num_qubits: 5,
            depth: 10,
            num_templates: 10,
            num_updates: 10,
        }
    }
}

/// Generate a parameterized CLOPS circuit template.
///
/// Creates alternating layers of single-qubit rotations and CX gates.
pub fn generate_clops_circuit(num_qubits: u32, depth: u32, seed: u64) -> Circuit {
    let mut rng = rand::rngs::SmallRng::seed_from_u64(seed);
    let mut circuit = Circuit::with_size(format!("clops_{num_qubits}x{depth}"), num_qubits, 0);

    for layer in 0..depth {
        // Layer of random single-qubit rotations
        for q in 0..num_qubits {
            let angle = rng.gen_range(0.0..2.0 * PI);
            let _ = circuit.rz(angle, QubitId(q));
            let _ = circuit.ry(angle * 0.5, QubitId(q));
        }

        // Layer of CX gates (linear connectivity)
        let offset = layer % 2;
        let mut q = offset;
        while q + 1 < num_qubits {
            let _ = circuit.cx(QubitId(q), QubitId(q + 1));
            q += 2;
        }
    }

    circuit
}

/// Rebuild a fresh `PropertySet` from the target fields of an existing one.
///
/// This avoids needing `Clone` on `PropertySet` (which contains type-erased custom data).
/// Only copies the target configuration fields needed for compilation.
fn rebuild_props(props: &PropertySet) -> PropertySet {
    let mut new = PropertySet::new();
    if let Some(ref cm) = props.coupling_map {
        new.coupling_map = Some(cm.clone());
    }
    if let Some(ref bg) = props.basis_gates {
        new.basis_gates = Some(bg.clone());
    }
    // Layout is intentionally NOT copied — each compilation starts fresh
    new
}

/// Run a CLOPS-style compilation throughput measurement.
///
/// Returns the number of circuit layer operations processed per second
/// during compilation (not including backend execution).
pub fn measure_compilation_clops(
    config: &ClopsConfig,
    pm: &PassManager,
    props: &PropertySet,
) -> BenchmarkResult {
    let start = Instant::now();
    let mut total_layers = 0u64;

    for template_id in 0..config.num_templates {
        for update_id in 0..config.num_updates {
            let seed = u64::from(template_id) * 1000 + u64::from(update_id);
            let circuit = generate_clops_circuit(config.num_qubits, config.depth, seed);

            let mut dag = circuit.into_dag();
            let mut local_props = rebuild_props(props);
            let _ = pm.run(&mut dag, &mut local_props);

            total_layers += u64::from(config.depth) * u64::from(config.num_qubits);
        }
    }

    let elapsed = start.elapsed();
    let clops = total_layers as f64 / elapsed.as_secs_f64();

    BenchmarkResult::new("clops_compilation", clops, "layer_ops/sec")
        .with_duration(elapsed)
        .with_metric("total_layers", total_layers)
        .with_metric("num_templates", u64::from(config.num_templates))
        .with_metric("num_updates", u64::from(config.num_updates))
        .with_metric("num_qubits", u64::from(config.num_qubits))
        .with_metric("depth", u64::from(config.depth))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_clops_circuit() {
        let circuit = generate_clops_circuit(5, 10, 42);
        assert_eq!(circuit.num_qubits(), 5);
        assert!(circuit.depth() > 0);
    }

    #[test]
    fn test_clops_deterministic() {
        let c1 = generate_clops_circuit(3, 5, 99);
        let c2 = generate_clops_circuit(3, 5, 99);
        assert_eq!(c1.depth(), c2.depth());
    }

    #[test]
    fn test_measure_compilation_clops() {
        let config = ClopsConfig {
            num_qubits: 3,
            depth: 5,
            num_templates: 2,
            num_updates: 2,
        };

        let pm = PassManager::new(); // no passes = raw throughput
        let props = PropertySet::new();

        let result = measure_compilation_clops(&config, &pm, &props);
        assert!(result.value > 0.0);
        assert_eq!(result.unit, "layer_ops/sec");
    }
}
