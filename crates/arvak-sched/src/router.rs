//! Job router for directing quantum jobs to appropriate backends.
//!
//! The router examines job properties (qubit count, shots, priority, topology
//! preference) and routes them to the best execution target: cloud backend,
//! HPC scheduler, or local simulator.

use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::broker::subjects;
use crate::job::ScheduledJob;

/// Routing decision for a quantum job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteTarget {
    /// Route to a cloud backend (IQM, IBM, CUDA-Q, etc.).
    Cloud {
        /// Preferred backend name.
        backend: String,
    },
    /// Route to an HPC batch scheduler (SLURM/PBS).
    Hpc,
    /// Route to a local simulator.
    Local,
}

impl RouteTarget {
    /// Get the message broker subject for this route target.
    pub fn subject(&self) -> &str {
        match self {
            RouteTarget::Cloud { .. } => subjects::CLOUD,
            RouteTarget::Hpc => subjects::HPC,
            RouteTarget::Local => subjects::LOCAL,
        }
    }
}

/// Rules for routing jobs to backends.
#[derive(Debug, Clone)]
pub struct RoutingRules {
    /// Maximum qubits for local simulation.
    pub local_qubit_limit: u32,
    /// Maximum qubits for cloud execution.
    pub cloud_qubit_limit: u32,
    /// Preferred cloud backend.
    pub default_cloud_backend: String,
    /// Whether to prefer HPC for large jobs.
    pub prefer_hpc_for_large_jobs: bool,
}

impl Default for RoutingRules {
    fn default() -> Self {
        Self {
            local_qubit_limit: 25,
            cloud_qubit_limit: 100,
            default_cloud_backend: "iqm".into(),
            prefer_hpc_for_large_jobs: true,
        }
    }
}

/// Job router that decides where to send quantum jobs.
pub struct JobRouter {
    rules: RoutingRules,
}

impl JobRouter {
    /// Create a new job router with default rules.
    pub fn new() -> Self {
        Self {
            rules: RoutingRules::default(),
        }
    }

    /// Create a router with custom rules.
    pub fn with_rules(rules: RoutingRules) -> Self {
        Self { rules }
    }

    /// Route a job based on its properties.
    pub fn route(&self, job: &ScheduledJob) -> RouteTarget {
        // Determine max qubit count across all circuits in the job
        let num_qubits = job.max_qubits().unwrap_or(0);

        // Check if a preferred backend was explicitly set
        if let Some(ref backend) = job.matched_backend {
            debug!("Routing to matched backend: {}", backend);
            return RouteTarget::Cloud {
                backend: backend.clone(),
            };
        }

        // Check preferred backends from resource requirements
        if !job.requirements.preferred_backends.is_empty() {
            let backend = job.requirements.preferred_backends[0].clone();
            debug!("Routing to preferred backend: {}", backend);
            return RouteTarget::Cloud { backend };
        }

        // Auto-routing based on circuit size
        if num_qubits <= self.rules.local_qubit_limit {
            debug!(
                "Routing to local (qubits={} <= limit={})",
                num_qubits, self.rules.local_qubit_limit
            );
            return RouteTarget::Local;
        }

        // Large circuits: prefer HPC if enabled
        if self.rules.prefer_hpc_for_large_jobs && num_qubits > self.rules.cloud_qubit_limit {
            debug!(
                "Routing to HPC (qubits={} > cloud_limit={})",
                num_qubits, self.rules.cloud_qubit_limit
            );
            return RouteTarget::Hpc;
        }

        // Default: cloud
        let backend = self.rules.default_cloud_backend.clone();
        debug!("Routing to cloud/{} (default)", backend);
        RouteTarget::Cloud { backend }
    }
}

impl Default for JobRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{CircuitSpec, ScheduledJob};

    fn small_qasm() -> CircuitSpec {
        CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q; h q[0]; cx q[0], q[1];")
    }

    fn large_qasm(n: usize) -> CircuitSpec {
        let mut qasm = format!("OPENQASM 3.0; qubit[{n}] q;");
        for i in 0..n.saturating_sub(1) {
            qasm.push_str(&format!(" cx q[{}], q[{}];", i, i + 1));
        }
        CircuitSpec::from_qasm(qasm)
    }

    #[test]
    fn test_route_small_to_local() {
        let router = JobRouter::new();
        let job = ScheduledJob::new("test", small_qasm()).with_shots(1000);

        assert_eq!(router.route(&job), RouteTarget::Local);
    }

    #[test]
    fn test_route_medium_to_cloud() {
        let router = JobRouter::new();
        let job = ScheduledJob::new("test", large_qasm(30)).with_shots(1000);

        assert_eq!(
            router.route(&job),
            RouteTarget::Cloud {
                backend: "iqm".into()
            }
        );
    }

    #[test]
    fn test_route_large_to_hpc() {
        let router = JobRouter::new();
        let job = ScheduledJob::new("test", large_qasm(150)).with_shots(1000);

        assert_eq!(router.route(&job), RouteTarget::Hpc);
    }

    #[test]
    fn test_custom_rules() {
        let rules = RoutingRules {
            local_qubit_limit: 5,
            cloud_qubit_limit: 50,
            default_cloud_backend: "cudaq".into(),
            prefer_hpc_for_large_jobs: true,
        };
        let router = JobRouter::with_rules(rules);

        let job = ScheduledJob::new("test", large_qasm(10)).with_shots(1000);
        assert_eq!(
            router.route(&job),
            RouteTarget::Cloud {
                backend: "cudaq".into()
            }
        );
    }

    #[test]
    fn test_route_target_subjects() {
        assert_eq!(RouteTarget::Local.subject(), "jobs.local");
        assert_eq!(RouteTarget::Hpc.subject(), "jobs.hpc");
        assert_eq!(
            RouteTarget::Cloud {
                backend: "iqm".into()
            }
            .subject(),
            "jobs.cloud"
        );
    }
}
