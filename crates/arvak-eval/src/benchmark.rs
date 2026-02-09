//! Optional Benchmark Loader: standard quantum benchmark circuits.
//!
//! Provides well-known benchmark circuits (GHZ, Grover, QFT, etc.) as
//! workload inputs for evaluation. These are **non-normative** -- they
//! serve only as convenient workload generators and do NOT replace
//! QDMI evaluation.
//!
//! Reference: MQT Bench style circuits, built using arvak-ir primitives.

use serde::{Deserialize, Serialize};

use arvak_ir::{Circuit, QubitId};

use crate::error::{EvalError, EvalResult};

/// Available benchmark suites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BenchmarkSuite {
    /// GHZ state preparation (N-qubit entanglement).
    Ghz,
    /// Quantum Fourier Transform.
    Qft,
    /// Grover's search (simplified single iteration).
    Grover,
    /// Mixed-gate circuit for broad coverage testing.
    Random,
}

impl BenchmarkSuite {
    /// Parse a suite name from string.
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "ghz" => Some(Self::Ghz),
            "qft" => Some(Self::Qft),
            "grover" => Some(Self::Grover),
            "random" => Some(Self::Random),
            _ => None,
        }
    }

    /// Human-readable name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Ghz => "GHZ State",
            Self::Qft => "Quantum Fourier Transform",
            Self::Grover => "Grover Search",
            Self::Random => "Random Circuit",
        }
    }

    /// All available suites.
    pub fn all() -> Vec<Self> {
        vec![Self::Ghz, Self::Qft, Self::Grover, Self::Random]
    }
}

/// A benchmark circuit with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkCircuit {
    /// Benchmark suite identifier.
    pub suite: String,
    /// Human-readable name.
    pub name: String,
    /// Number of qubits.
    pub num_qubits: usize,
    /// Expected gate count.
    pub expected_gates: usize,
    /// Generated QASM3 source.
    pub qasm3_source: String,
}

/// Benchmark loader and generator.
pub struct BenchmarkLoader;

impl BenchmarkLoader {
    /// Generate a benchmark circuit for the given suite and qubit count.
    pub fn generate(suite: &BenchmarkSuite, num_qubits: usize) -> EvalResult<BenchmarkCircuit> {
        if num_qubits == 0 {
            return Err(EvalError::Parse(
                "Benchmark requires at least 1 qubit".into(),
            ));
        }

        let (circuit, expected_gates) = match suite {
            BenchmarkSuite::Ghz => Self::build_ghz(num_qubits)?,
            BenchmarkSuite::Qft => Self::build_qft(num_qubits)?,
            BenchmarkSuite::Grover => Self::build_grover(num_qubits)?,
            BenchmarkSuite::Random => Self::build_random(num_qubits)?,
        };

        let qasm3_source = arvak_qasm3::emit(&circuit)
            .map_err(|e| EvalError::Parse(format!("Failed to emit benchmark: {e}")))?;

        Ok(BenchmarkCircuit {
            suite: format!("{suite:?}"),
            name: format!("{} ({}q)", suite.display_name(), num_qubits),
            num_qubits,
            expected_gates,
            qasm3_source,
        })
    }

    /// List available benchmarks with descriptions.
    pub fn available() -> Vec<(&'static str, &'static str)> {
        vec![
            ("ghz", "GHZ state preparation - entangles N qubits"),
            ("qft", "Quantum Fourier Transform"),
            ("grover", "Grover's search with simple oracle"),
            ("random", "Mixed-gate circuit for coverage testing"),
        ]
    }

    /// GHZ state: H on q[0], then CX cascade.
    fn build_ghz(n: usize) -> EvalResult<(Circuit, usize)> {
        let mut circuit = Circuit::with_size("ghz", n as u32, 0);
        circuit
            .h(QubitId(0))
            .map_err(|e| EvalError::Compilation(e.to_string()))?;

        for i in 0..n - 1 {
            circuit
                .cx(QubitId(i as u32), QubitId((i + 1) as u32))
                .map_err(|e| EvalError::Compilation(e.to_string()))?;
        }

        let gates = n; // 1 H + (n-1) CX
        Ok((circuit, gates))
    }

    /// Simplified QFT: H + controlled rotations (approximated as CX).
    fn build_qft(n: usize) -> EvalResult<(Circuit, usize)> {
        let mut circuit = Circuit::with_size("qft", n as u32, 0);
        let mut gate_count = 0;

        for i in 0..n {
            circuit
                .h(QubitId(i as u32))
                .map_err(|e| EvalError::Compilation(e.to_string()))?;
            gate_count += 1;

            for j in (i + 1)..n {
                circuit
                    .cx(QubitId(j as u32), QubitId(i as u32))
                    .map_err(|e| EvalError::Compilation(e.to_string()))?;
                gate_count += 1;
            }
        }

        Ok((circuit, gate_count))
    }

    /// Simplified Grover (1 iteration): H layer + oracle + diffusion.
    fn build_grover(n: usize) -> EvalResult<(Circuit, usize)> {
        let mut circuit = Circuit::with_size("grover", n as u32, 0);
        let mut gate_count = 0;

        // Initial H layer
        for i in 0..n {
            circuit
                .h(QubitId(i as u32))
                .map_err(|e| EvalError::Compilation(e.to_string()))?;
            gate_count += 1;
        }

        // Oracle: CX cascade (simplified)
        for i in 0..n.saturating_sub(1) {
            circuit
                .cx(QubitId(i as u32), QubitId((i + 1) as u32))
                .map_err(|e| EvalError::Compilation(e.to_string()))?;
            gate_count += 1;
        }

        // Diffusion operator (simplified): H + X + CX + X + H
        for i in 0..n {
            circuit
                .h(QubitId(i as u32))
                .map_err(|e| EvalError::Compilation(e.to_string()))?;
            gate_count += 1;
        }
        for i in 0..n {
            circuit
                .x(QubitId(i as u32))
                .map_err(|e| EvalError::Compilation(e.to_string()))?;
            gate_count += 1;
        }
        if n >= 2 {
            circuit
                .cx(QubitId(0), QubitId(1))
                .map_err(|e| EvalError::Compilation(e.to_string()))?;
            gate_count += 1;
        }
        for i in 0..n {
            circuit
                .x(QubitId(i as u32))
                .map_err(|e| EvalError::Compilation(e.to_string()))?;
            gate_count += 1;
        }
        for i in 0..n {
            circuit
                .h(QubitId(i as u32))
                .map_err(|e| EvalError::Compilation(e.to_string()))?;
            gate_count += 1;
        }

        Ok((circuit, gate_count))
    }

    /// Mixed-gate circuit with varied gate types for broad coverage.
    fn build_random(n: usize) -> EvalResult<(Circuit, usize)> {
        let mut circuit = Circuit::with_size("random", n as u32, 0);
        let mut gate_count = 0;

        // Layer 1: H on all
        for i in 0..n {
            circuit
                .h(QubitId(i as u32))
                .map_err(|e| EvalError::Compilation(e.to_string()))?;
            gate_count += 1;
        }

        // Layer 2: CX ladder (even pairs)
        for i in (0..n).step_by(2) {
            if i + 1 < n {
                circuit
                    .cx(QubitId(i as u32), QubitId((i + 1) as u32))
                    .map_err(|e| EvalError::Compilation(e.to_string()))?;
                gate_count += 1;
            }
        }

        // Layer 3: S gates
        for i in 0..n {
            circuit
                .s(QubitId(i as u32))
                .map_err(|e| EvalError::Compilation(e.to_string()))?;
            gate_count += 1;
        }

        // Layer 4: CX ladder (reverse, odd pairs)
        for i in (0..n).rev().step_by(2) {
            if i > 0 {
                circuit
                    .cx(QubitId(i as u32), QubitId((i - 1) as u32))
                    .map_err(|e| EvalError::Compilation(e.to_string()))?;
                gate_count += 1;
            }
        }

        // Layer 5: T gates
        for i in 0..n {
            circuit
                .t(QubitId(i as u32))
                .map_err(|e| EvalError::Compilation(e.to_string()))?;
            gate_count += 1;
        }

        Ok((circuit, gate_count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suite_from_name() {
        assert_eq!(BenchmarkSuite::from_name("ghz"), Some(BenchmarkSuite::Ghz));
        assert_eq!(BenchmarkSuite::from_name("qft"), Some(BenchmarkSuite::Qft));
        assert_eq!(
            BenchmarkSuite::from_name("grover"),
            Some(BenchmarkSuite::Grover)
        );
        assert_eq!(
            BenchmarkSuite::from_name("random"),
            Some(BenchmarkSuite::Random)
        );
        assert_eq!(BenchmarkSuite::from_name("unknown"), None);
    }

    #[test]
    fn test_ghz_generation() {
        let bench = BenchmarkLoader::generate(&BenchmarkSuite::Ghz, 4).unwrap();
        assert_eq!(bench.num_qubits, 4);
        assert_eq!(bench.expected_gates, 4); // 1 H + 3 CX
        assert!(bench.qasm3_source.contains("OPENQASM 3.0"));
        assert!(bench.qasm3_source.contains("h q[0]"));
        assert!(bench.qasm3_source.contains("cx"));
    }

    #[test]
    fn test_qft_generation() {
        let bench = BenchmarkLoader::generate(&BenchmarkSuite::Qft, 3).unwrap();
        assert_eq!(bench.num_qubits, 3);
        assert_eq!(bench.expected_gates, 6); // 3 H + 3 CX
        assert!(!bench.qasm3_source.is_empty());
    }

    #[test]
    fn test_grover_generation() {
        let bench = BenchmarkLoader::generate(&BenchmarkSuite::Grover, 3).unwrap();
        assert_eq!(bench.num_qubits, 3);
        assert!(!bench.qasm3_source.is_empty());
        assert!(bench.expected_gates > 0);
    }

    #[test]
    fn test_random_generation() {
        let bench = BenchmarkLoader::generate(&BenchmarkSuite::Random, 4).unwrap();
        assert_eq!(bench.num_qubits, 4);
        assert!(!bench.qasm3_source.is_empty());
        // Should contain diverse gate types
        assert!(bench.qasm3_source.contains('h'));
        assert!(bench.qasm3_source.contains("cx"));
        assert!(bench.qasm3_source.contains('s'));
        assert!(bench.qasm3_source.contains('t'));
    }

    #[test]
    fn test_zero_qubits_error() {
        let result = BenchmarkLoader::generate(&BenchmarkSuite::Ghz, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_single_qubit_ghz() {
        let bench = BenchmarkLoader::generate(&BenchmarkSuite::Ghz, 1).unwrap();
        assert_eq!(bench.num_qubits, 1);
        assert_eq!(bench.expected_gates, 1); // Just H
    }

    #[test]
    fn test_available_benchmarks() {
        let available = BenchmarkLoader::available();
        assert_eq!(available.len(), 4);
    }

    #[test]
    fn test_all_suites() {
        let suites = BenchmarkSuite::all();
        assert_eq!(suites.len(), 4);
    }

    #[test]
    fn test_display_names() {
        assert_eq!(BenchmarkSuite::Ghz.display_name(), "GHZ State");
        assert_eq!(
            BenchmarkSuite::Qft.display_name(),
            "Quantum Fourier Transform"
        );
        assert_eq!(BenchmarkSuite::Grover.display_name(), "Grover Search");
        assert_eq!(BenchmarkSuite::Random.display_name(), "Random Circuit");
    }
}
