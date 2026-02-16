//! Resource matcher for matching circuits to backends.

use std::sync::Arc;

use arvak_hal::{Backend, Capabilities};
use async_trait::async_trait;

use crate::error::{SchedError, SchedResult};
use crate::job::{ResourceRequirements, TopologyPreference};

/// Result of a resource match.
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// Name of the matched backend.
    pub backend_name: String,

    /// Score of the match (higher is better).
    pub score: f64,

    /// Capabilities of the matched backend.
    pub capabilities: Capabilities,

    /// Reasons for the match score.
    pub score_breakdown: Vec<(String, f64)>,
}

/// Trait for matching circuits to backends.
#[async_trait]
pub trait Matcher: Send + Sync {
    /// Find the best matching backend for the given requirements.
    async fn find_match(&self, requirements: &ResourceRequirements) -> SchedResult<MatchResult>;

    /// Find all matching backends sorted by score.
    async fn find_all_matches(
        &self,
        requirements: &ResourceRequirements,
    ) -> SchedResult<Vec<MatchResult>>;
}

/// Resource matcher that finds suitable backends for circuit execution.
pub struct ResourceMatcher {
    backends: Vec<Arc<dyn Backend>>,
    /// Cache of backend capabilities.
    capabilities_cache: tokio::sync::RwLock<rustc_hash::FxHashMap<String, Capabilities>>,
}

impl ResourceMatcher {
    /// Create a new resource matcher with the given backends.
    pub fn new(backends: Vec<Arc<dyn Backend>>) -> Self {
        Self {
            backends,
            capabilities_cache: tokio::sync::RwLock::new(rustc_hash::FxHashMap::default()),
        }
    }

    /// Add a backend to the matcher.
    pub fn add_backend(&mut self, backend: Arc<dyn Backend>) {
        self.backends.push(backend);
    }

    /// Refresh the capabilities cache for all backends.
    pub async fn refresh_cache(&self) -> SchedResult<()> {
        let mut cache = self.capabilities_cache.write().await;
        cache.clear();

        for backend in &self.backends {
            cache.insert(backend.name().to_string(), backend.capabilities().clone());
        }

        Ok(())
    }

    /// Get capabilities for a backend (sync in HAL Contract v2).
    fn get_capabilities(&self, backend: &dyn Backend) -> Capabilities {
        backend.capabilities().clone()
    }

    /// Calculate a match score for a backend.
    fn calculate_score(
        &self,
        requirements: &ResourceRequirements,
        capabilities: &Capabilities,
    ) -> (f64, Vec<(String, f64)>) {
        let mut score = 0.0;
        let mut breakdown = Vec::new();

        // Base score for being available
        score += 10.0;
        breakdown.push(("Base availability".to_string(), 10.0));

        // Qubit count score (prefer closer match)
        let qubit_diff = (capabilities.num_qubits as i32 - requirements.min_qubits as i32).abs();
        let qubit_score = 20.0 / (1.0 + f64::from(qubit_diff) * 0.1);
        score += qubit_score;
        breakdown.push((format!("Qubit match (diff: {qubit_diff})"), qubit_score));

        // Simulator preference
        if !requirements.allow_simulator && capabilities.is_simulator {
            // Simulator not allowed but backend is simulator
            return (0.0, vec![("Simulator not allowed".to_string(), 0.0)]);
        }

        if !capabilities.is_simulator {
            // Bonus for real hardware
            let hw_score = 15.0;
            score += hw_score;
            breakdown.push(("Real hardware bonus".to_string(), hw_score));
        }

        // Preferred backend bonus
        if requirements.preferred_backends.contains(&capabilities.name) {
            let pref_score = 25.0;
            score += pref_score;
            breakdown.push(("Preferred backend bonus".to_string(), pref_score));
        }

        // Topology matching
        if let Some(ref pref) = requirements.topology_preference {
            let topology_score = self.score_topology(pref, capabilities);
            score += topology_score;
            breakdown.push(("Topology match".to_string(), topology_score));
        }

        // Gate set matching
        let gate_score = self.score_gates(requirements, capabilities);
        score += gate_score;
        breakdown.push(("Gate set match".to_string(), gate_score));

        (score, breakdown)
    }

    /// Score topology match.
    fn score_topology(&self, preference: &TopologyPreference, capabilities: &Capabilities) -> f64 {
        if capabilities.num_qubits == 0 {
            return 0.0;
        }

        match preference {
            TopologyPreference::Linear => {
                // Check if topology is linear-compatible
                // For now, just check if it's not too sparse
                if capabilities.topology.edges.len()
                    >= (capabilities.num_qubits as usize).saturating_sub(1)
                {
                    10.0
                } else {
                    5.0
                }
            }
            TopologyPreference::Grid => {
                // Prefer topologies with higher connectivity
                let avg_degree =
                    capabilities.topology.edges.len() as f64 / f64::from(capabilities.num_qubits);
                if avg_degree >= 2.0 { 10.0 } else { 5.0 }
            }
            TopologyPreference::AllToAll => {
                // Check if fully connected
                let n = capabilities.num_qubits as usize;
                let max_edges = n * n.saturating_sub(1) / 2;
                if capabilities.topology.edges.len() >= max_edges {
                    15.0
                } else {
                    5.0
                }
            }
            TopologyPreference::Specific(_name) => {
                // Would need to match against known topology names
                5.0
            }
        }
    }

    /// Score gate set match.
    fn score_gates(&self, requirements: &ResourceRequirements, capabilities: &Capabilities) -> f64 {
        if requirements.required_gates.is_empty() {
            return 10.0;
        }

        // Collect all supported gates (single-qubit + two-qubit + native)
        let mut supported: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
        for gate in &capabilities.gate_set.single_qubit {
            supported.insert(gate.to_lowercase());
        }
        for gate in &capabilities.gate_set.two_qubit {
            supported.insert(gate.to_lowercase());
        }
        for gate in &capabilities.gate_set.native {
            supported.insert(gate.to_lowercase());
        }

        let mut matched = 0;
        for gate in &requirements.required_gates {
            if supported.contains(&gate.to_lowercase()) {
                matched += 1;
            }
        }

        let ratio = f64::from(matched) / requirements.required_gates.len() as f64;
        if ratio < 1.0 {
            // Some required gates are missing
            ratio * 5.0
        } else {
            10.0
        }
    }
}

#[async_trait]
impl Matcher for ResourceMatcher {
    async fn find_match(&self, requirements: &ResourceRequirements) -> SchedResult<MatchResult> {
        let matches = self.find_all_matches(requirements).await?;

        matches.into_iter().next().ok_or_else(|| {
            SchedError::NoMatchingBackend(format!(
                "No backend found with {} qubits",
                requirements.min_qubits
            ))
        })
    }

    async fn find_all_matches(
        &self,
        requirements: &ResourceRequirements,
    ) -> SchedResult<Vec<MatchResult>> {
        let mut matches = Vec::new();

        for backend in &self.backends {
            // Check availability
            let avail = backend
                .availability()
                .await
                .unwrap_or(arvak_hal::BackendAvailability::unavailable("query failed"));
            if !avail.is_available {
                continue;
            }

            // Get capabilities (sync, infallible in HAL Contract v2)
            let capabilities = self.get_capabilities(backend.as_ref());

            // Check minimum qubit requirement
            if capabilities.num_qubits < requirements.min_qubits {
                continue;
            }

            // Calculate score
            let (score, breakdown) = self.calculate_score(requirements, &capabilities);

            // Skip if score is 0 (failed a hard requirement)
            if score <= 0.0 {
                continue;
            }

            matches.push(MatchResult {
                backend_name: backend.name().to_string(),
                score,
                capabilities,
                score_breakdown: breakdown,
            });
        }

        // Sort by score (highest first)
        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arvak_hal::{
        BackendAvailability, Counts, GateSet, Topology, TopologyKind, ValidationResult,
    };

    /// Mock backend for testing.
    struct MockBackend {
        name: String,
        capabilities: Capabilities,
        available: bool,
    }

    #[async_trait]
    impl Backend for MockBackend {
        fn name(&self) -> &str {
            &self.name
        }

        fn capabilities(&self) -> &Capabilities {
            &self.capabilities
        }

        async fn availability(&self) -> arvak_hal::HalResult<BackendAvailability> {
            if self.available {
                Ok(BackendAvailability::always_available())
            } else {
                Ok(BackendAvailability::unavailable("offline"))
            }
        }

        async fn validate(
            &self,
            _circuit: &arvak_ir::Circuit,
        ) -> arvak_hal::HalResult<ValidationResult> {
            Ok(ValidationResult::Valid)
        }

        async fn submit(
            &self,
            _circuit: &arvak_ir::Circuit,
            _shots: u32,
        ) -> arvak_hal::HalResult<arvak_hal::JobId> {
            Ok(arvak_hal::JobId("mock".to_string()))
        }

        async fn status(
            &self,
            _job_id: &arvak_hal::JobId,
        ) -> arvak_hal::HalResult<arvak_hal::JobStatus> {
            Ok(arvak_hal::JobStatus::Completed)
        }

        async fn result(
            &self,
            _job_id: &arvak_hal::JobId,
        ) -> arvak_hal::HalResult<arvak_hal::ExecutionResult> {
            Ok(arvak_hal::ExecutionResult::new(Counts::new(), 0))
        }

        async fn cancel(&self, _job_id: &arvak_hal::JobId) -> arvak_hal::HalResult<()> {
            Ok(())
        }

        async fn wait(
            &self,
            _job_id: &arvak_hal::JobId,
        ) -> arvak_hal::HalResult<arvak_hal::ExecutionResult> {
            self.result(_job_id).await
        }
    }

    fn make_backend(name: &str, num_qubits: u32, is_simulator: bool) -> Arc<dyn Backend> {
        Arc::new(MockBackend {
            name: name.to_string(),
            capabilities: Capabilities {
                name: name.to_string(),
                num_qubits,
                gate_set: GateSet {
                    single_qubit: vec!["h".to_string(), "x".to_string()],
                    two_qubit: vec!["cx".to_string()],
                    three_qubit: vec![],
                    native: vec![],
                },
                topology: Topology {
                    kind: TopologyKind::Linear,
                    edges: (0..num_qubits.saturating_sub(1))
                        .map(|i| (i, i + 1))
                        .collect(),
                },
                max_shots: 10000,
                is_simulator,
                features: vec![],
                noise_profile: None,
            },
            available: true,
        })
    }

    #[tokio::test]
    async fn test_find_match_basic() {
        let backends = vec![
            make_backend("simulator", 20, true),
            make_backend("real_hw", 5, false),
        ];

        let matcher = ResourceMatcher::new(backends);

        // Should prefer real hardware
        let requirements = ResourceRequirements::new(2);
        let result = matcher.find_match(&requirements).await.unwrap();
        assert_eq!(result.backend_name, "real_hw");
    }

    #[tokio::test]
    async fn test_find_match_qubit_requirement() {
        let backends = vec![
            make_backend("small", 5, true),
            make_backend("large", 20, true),
        ];

        let matcher = ResourceMatcher::new(backends);

        // Need 10 qubits, should match large
        let requirements = ResourceRequirements::new(10);
        let result = matcher.find_match(&requirements).await.unwrap();
        assert_eq!(result.backend_name, "large");
    }

    #[tokio::test]
    async fn test_find_match_no_simulator() {
        let backends = vec![
            make_backend("simulator", 20, true),
            make_backend("real_hw", 5, false),
        ];

        let matcher = ResourceMatcher::new(backends);

        // Require real hardware
        let requirements = ResourceRequirements::new(2).require_real_hardware();
        let result = matcher.find_match(&requirements).await.unwrap();
        assert_eq!(result.backend_name, "real_hw");
    }

    #[tokio::test]
    async fn test_find_match_preferred_backend() {
        let backends = vec![
            make_backend("backend_a", 10, true),
            make_backend("backend_b", 10, true),
        ];

        let matcher = ResourceMatcher::new(backends);

        // Prefer backend_b
        let requirements = ResourceRequirements::new(2).prefer_backend("backend_b");
        let result = matcher.find_match(&requirements).await.unwrap();
        assert_eq!(result.backend_name, "backend_b");
    }

    #[tokio::test]
    async fn test_no_matching_backend() {
        let backends = vec![make_backend("small", 5, true)];

        let matcher = ResourceMatcher::new(backends);

        // Need more qubits than available
        let requirements = ResourceRequirements::new(100);
        let result = matcher.find_match(&requirements).await;
        assert!(matches!(result, Err(SchedError::NoMatchingBackend(_))));
    }
}
