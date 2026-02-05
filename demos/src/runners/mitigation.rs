//! Error mitigation techniques for quantum circuit execution.
//!
//! This module provides various error mitigation strategies that can improve
//! the quality of results from noisy quantum hardware:
//!
//! - **Zero Noise Extrapolation (ZNE)**: Amplify noise and extrapolate to zero noise
//! - **Measurement Error Mitigation**: Correct for readout errors
//! - **Pauli Twirling**: Convert coherent errors to stochastic errors

use std::collections::HashMap;

/// Error mitigation configuration.
#[derive(Debug, Clone)]
pub struct MitigationConfig {
    /// Enable Zero Noise Extrapolation.
    pub zne_enabled: bool,
    /// Noise scale factors for ZNE (e.g., [1.0, 2.0, 3.0]).
    pub zne_scale_factors: Vec<f64>,
    /// Enable measurement error mitigation.
    pub measurement_mitigation: bool,
    /// Number of calibration shots for measurement mitigation.
    pub calibration_shots: u32,
    /// Enable Pauli twirling.
    pub pauli_twirling: bool,
    /// Number of twirling samples.
    pub twirling_samples: usize,
}

impl Default for MitigationConfig {
    fn default() -> Self {
        Self {
            zne_enabled: false,
            zne_scale_factors: vec![1.0, 2.0, 3.0],
            measurement_mitigation: false,
            calibration_shots: 1000,
            pauli_twirling: false,
            twirling_samples: 10,
        }
    }
}

impl MitigationConfig {
    /// Create a new configuration with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable Zero Noise Extrapolation with custom scale factors.
    pub fn with_zne(mut self, scale_factors: Vec<f64>) -> Self {
        self.zne_enabled = true;
        self.zne_scale_factors = scale_factors;
        self
    }

    /// Enable measurement error mitigation.
    pub fn with_measurement_mitigation(mut self, calibration_shots: u32) -> Self {
        self.measurement_mitigation = true;
        self.calibration_shots = calibration_shots;
        self
    }

    /// Enable Pauli twirling.
    pub fn with_pauli_twirling(mut self, samples: usize) -> Self {
        self.pauli_twirling = true;
        self.twirling_samples = samples;
        self
    }

    /// Enable all mitigation techniques with default parameters.
    pub fn full_mitigation() -> Self {
        Self {
            zne_enabled: true,
            zne_scale_factors: vec![1.0, 1.5, 2.0, 2.5, 3.0],
            measurement_mitigation: true,
            calibration_shots: 2000,
            pauli_twirling: true,
            twirling_samples: 20,
        }
    }
}

/// Zero Noise Extrapolation result.
#[derive(Debug, Clone)]
pub struct ZneResult {
    /// Expectation values at each noise scale.
    pub scaled_values: Vec<(f64, f64)>,
    /// Extrapolated zero-noise value.
    pub extrapolated_value: f64,
    /// Fit quality (R² value).
    pub fit_quality: f64,
}

/// Perform Zero Noise Extrapolation.
///
/// ZNE works by:
/// 1. Running the circuit at different noise levels (by gate folding)
/// 2. Fitting the results to a noise model
/// 3. Extrapolating to zero noise
///
/// # Arguments
/// * `values` - Expectation values at each noise scale factor
/// * `scale_factors` - The noise scale factors used
///
/// # Returns
/// The extrapolated zero-noise value
pub fn zero_noise_extrapolation(values: &[f64], scale_factors: &[f64]) -> ZneResult {
    assert_eq!(values.len(), scale_factors.len());
    assert!(values.len() >= 2);

    // Richardson extrapolation (linear fit for simplicity)
    // For more accuracy, use polynomial or exponential fits
    let n = values.len() as f64;

    // Linear regression: y = a + b*x, extrapolate to x=0
    let sum_x: f64 = scale_factors.iter().sum();
    let sum_y: f64 = values.iter().sum();
    let sum_xy: f64 = scale_factors
        .iter()
        .zip(values.iter())
        .map(|(x, y)| x * y)
        .sum();
    let sum_x2: f64 = scale_factors.iter().map(|x| x * x).sum();

    let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x * sum_x);
    let intercept = (sum_y - slope * sum_x) / n;

    // Calculate R² for fit quality
    let y_mean = sum_y / n;
    let ss_tot: f64 = values.iter().map(|y| (y - y_mean).powi(2)).sum();
    let ss_res: f64 = scale_factors
        .iter()
        .zip(values.iter())
        .map(|(x, y)| (y - (intercept + slope * x)).powi(2))
        .sum();
    let r_squared = if ss_tot > 0.0 {
        1.0 - ss_res / ss_tot
    } else {
        1.0
    };

    let scaled_values: Vec<(f64, f64)> = scale_factors
        .iter()
        .zip(values.iter())
        .map(|(&s, &v)| (s, v))
        .collect();

    ZneResult {
        scaled_values,
        extrapolated_value: intercept,
        fit_quality: r_squared,
    }
}

/// Measurement error mitigation matrix.
///
/// This stores the calibration data for correcting readout errors.
#[derive(Debug, Clone)]
pub struct MeasurementMitigator {
    /// Number of qubits.
    n_qubits: usize,
    /// Calibration matrix (row = prepared state, col = measured state).
    calibration_matrix: Vec<Vec<f64>>,
    /// Inverse calibration matrix for correction.
    inverse_matrix: Vec<Vec<f64>>,
}

impl MeasurementMitigator {
    /// Create a new mitigator from calibration data.
    ///
    /// # Arguments
    /// * `n_qubits` - Number of qubits
    /// * `calibration_counts` - Map from prepared state to measured counts
    pub fn from_calibration(
        n_qubits: usize,
        calibration_counts: &HashMap<usize, HashMap<usize, u32>>,
    ) -> Self {
        let dim = 1 << n_qubits;
        let mut calibration_matrix = vec![vec![0.0; dim]; dim];

        // Build the calibration matrix
        for (prepared, measured_counts) in calibration_counts {
            let total: u32 = measured_counts.values().sum();
            for (measured, &count) in measured_counts {
                calibration_matrix[*prepared][*measured] = count as f64 / total as f64;
            }
        }

        // Compute pseudo-inverse (simplified for demo - use proper linear algebra in production)
        let inverse_matrix = Self::pseudo_inverse(&calibration_matrix, dim);

        Self {
            n_qubits,
            calibration_matrix,
            inverse_matrix,
        }
    }

    /// Create a mitigator assuming simple depolarizing readout errors.
    ///
    /// # Arguments
    /// * `n_qubits` - Number of qubits
    /// * `error_rate` - Probability of a bit flip during readout
    pub fn from_error_rate(n_qubits: usize, error_rate: f64) -> Self {
        let dim = 1 << n_qubits;
        let mut calibration_matrix = vec![vec![0.0; dim]; dim];

        // Build calibration matrix assuming independent errors
        for i in 0..dim {
            for j in 0..dim {
                let diff = i ^ j; // XOR gives which bits differ
                let num_errors = diff.count_ones() as usize;
                let prob = error_rate.powi(num_errors as i32)
                    * (1.0 - error_rate).powi((n_qubits - num_errors) as i32);
                calibration_matrix[i][j] = prob;
            }
        }

        let inverse_matrix = Self::pseudo_inverse(&calibration_matrix, dim);

        Self {
            n_qubits,
            calibration_matrix,
            inverse_matrix,
        }
    }

    /// Compute pseudo-inverse using iterative method.
    fn pseudo_inverse(matrix: &[Vec<f64>], dim: usize) -> Vec<Vec<f64>> {
        // Simplified pseudo-inverse: transpose for near-identity matrices
        // In production, use proper matrix inversion
        let mut inverse = vec![vec![0.0; dim]; dim];

        // Neumann series approximation: (A)^-1 ≈ I + (I-A) + (I-A)^2 + ...
        // For near-identity A, this converges quickly
        for i in 0..dim {
            inverse[i][i] = 1.0;
        }

        // One iteration of correction
        for i in 0..dim {
            for j in 0..dim {
                let correction = if i == j { 1.0 } else { 0.0 } - matrix[i][j];
                inverse[i][j] += correction;
            }
        }

        // Normalize rows to sum to 1 (probability conservation)
        for row in &mut inverse {
            let sum: f64 = row.iter().sum();
            if sum > 0.0 {
                for val in row {
                    *val /= sum;
                }
            }
        }

        inverse
    }

    /// Mitigate measurement errors in a probability distribution.
    pub fn mitigate(&self, probabilities: &[f64]) -> Vec<f64> {
        let dim = 1 << self.n_qubits;
        assert_eq!(probabilities.len(), dim);

        let mut mitigated = vec![0.0; dim];

        // Apply inverse calibration matrix
        for i in 0..dim {
            for j in 0..dim {
                mitigated[i] += self.inverse_matrix[i][j] * probabilities[j];
            }
        }

        // Clip negative values and renormalize
        for val in &mut mitigated {
            if *val < 0.0 {
                *val = 0.0;
            }
        }

        let sum: f64 = mitigated.iter().sum();
        if sum > 0.0 {
            for val in &mut mitigated {
                *val /= sum;
            }
        }

        mitigated
    }

    /// Get the readout fidelity for a specific state.
    pub fn readout_fidelity(&self, state: usize) -> f64 {
        self.calibration_matrix[state][state]
    }

    /// Get the average readout fidelity.
    pub fn average_fidelity(&self) -> f64 {
        let dim = 1 << self.n_qubits;
        (0..dim).map(|i| self.calibration_matrix[i][i]).sum::<f64>() / dim as f64
    }
}

/// Apply Pauli twirling to convert coherent errors to stochastic.
///
/// Twirling randomly applies Pauli gates before and after each two-qubit gate
/// and averages over many runs. This converts coherent errors (which can
/// accumulate) into stochastic errors (which partially cancel).
///
/// # Arguments
/// * `expectation_values` - Results from multiple twirled circuit executions
///
/// # Returns
/// The averaged expectation value
pub fn pauli_twirling_average(expectation_values: &[f64]) -> f64 {
    if expectation_values.is_empty() {
        return 0.0;
    }
    expectation_values.iter().sum::<f64>() / expectation_values.len() as f64
}

/// Compute variance of twirled results for uncertainty estimation.
pub fn twirling_variance(expectation_values: &[f64]) -> f64 {
    if expectation_values.len() < 2 {
        return 0.0;
    }
    let mean = pauli_twirling_average(expectation_values);
    let variance: f64 = expectation_values
        .iter()
        .map(|v| (v - mean).powi(2))
        .sum::<f64>()
        / (expectation_values.len() - 1) as f64;
    variance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zne_linear() {
        // Test with perfect linear data
        let values = vec![1.0, 0.9, 0.8];
        let scales = vec![1.0, 2.0, 3.0];

        let result = zero_noise_extrapolation(&values, &scales);

        // Should extrapolate to 1.1 at scale 0
        assert!((result.extrapolated_value - 1.1).abs() < 0.01);
        assert!(result.fit_quality > 0.99);
    }

    #[test]
    fn test_measurement_mitigator() {
        // 2-qubit system with 5% readout error
        let mitigator = MeasurementMitigator::from_error_rate(2, 0.05);

        // Check readout fidelity
        let fidelity = mitigator.average_fidelity();
        assert!(fidelity > 0.9);

        // Test mitigation on a noisy distribution
        let noisy_probs = vec![0.85, 0.05, 0.05, 0.05];
        let mitigated = mitigator.mitigate(&noisy_probs);

        // Mitigated should be closer to pure state
        assert!(mitigated[0] > noisy_probs[0]);
    }

    #[test]
    fn test_pauli_twirling() {
        // Simulated twirled measurements with some variance
        let values = vec![0.8, 0.82, 0.78, 0.81, 0.79];

        let avg = pauli_twirling_average(&values);
        let var = twirling_variance(&values);

        assert!((avg - 0.8).abs() < 0.01);
        assert!(var < 0.01);
    }

    #[test]
    fn test_mitigation_config() {
        let config = MitigationConfig::full_mitigation();

        assert!(config.zne_enabled);
        assert!(config.measurement_mitigation);
        assert!(config.pauli_twirling);
        assert_eq!(config.zne_scale_factors.len(), 5);
    }
}
