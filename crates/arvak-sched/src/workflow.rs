//! Workflow DAG for job dependencies.

use chrono::{DateTime, Utc};
use petgraph::Direction;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{SchedError, SchedResult};
use crate::job::{ScheduledJob, ScheduledJobId};

/// Unique identifier for a workflow.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkflowId(pub Uuid);

impl WorkflowId {
    /// Create a new random workflow ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parse a workflow ID from a string.
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl Default for WorkflowId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for WorkflowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Status of a workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowStatus {
    /// Workflow is pending execution.
    Pending,

    /// Workflow is running.
    Running,

    /// Workflow completed successfully.
    Completed,

    /// Workflow failed.
    Failed { reason: String },

    /// Workflow was cancelled.
    Cancelled,
}

impl WorkflowStatus {
    /// Check if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            WorkflowStatus::Completed | WorkflowStatus::Failed { .. } | WorkflowStatus::Cancelled
        )
    }
}

/// A node in the workflow DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    /// The job at this node.
    pub job: ScheduledJob,

    /// Node status (mirrors job status for quick access).
    pub completed: bool,

    /// Whether this node failed.
    pub failed: bool,
}

/// A workflow consisting of jobs with dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    /// Unique workflow identifier.
    pub id: WorkflowId,

    /// Human-readable name.
    pub name: String,

    /// Current status.
    pub status: WorkflowStatus,

    /// Creation timestamp.
    pub created_at: DateTime<Utc>,

    /// Completion timestamp.
    pub completed_at: Option<DateTime<Utc>>,

    /// The DAG of jobs.
    #[serde(skip)]
    dag: DiGraph<WorkflowNode, ()>,

    /// Mapping from job ID to node index.
    #[serde(skip)]
    job_index: rustc_hash::FxHashMap<ScheduledJobId, NodeIndex>,
}

impl Workflow {
    /// Create a new empty workflow.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: WorkflowId::new(),
            name: name.into(),
            status: WorkflowStatus::Pending,
            created_at: Utc::now(),
            completed_at: None,
            dag: DiGraph::new(),
            job_index: rustc_hash::FxHashMap::default(),
        }
    }

    /// Add a job to the workflow.
    pub fn add_job(&mut self, job: ScheduledJob) -> NodeIndex {
        let job_id = job.id.clone();
        let node = WorkflowNode {
            job,
            completed: false,
            failed: false,
        };
        let idx = self.dag.add_node(node);
        self.job_index.insert(job_id, idx);
        idx
    }

    /// Add a dependency edge between two jobs.
    ///
    /// The `from` job must complete before the `to` job can start.
    pub fn add_dependency(
        &mut self,
        from: &ScheduledJobId,
        to: &ScheduledJobId,
    ) -> SchedResult<()> {
        let from_idx = self
            .job_index
            .get(from)
            .ok_or_else(|| SchedError::InvalidDependency(from.to_string()))?;
        let to_idx = self
            .job_index
            .get(to)
            .ok_or_else(|| SchedError::InvalidDependency(to.to_string()))?;

        // Check for cycles
        if petgraph::algo::has_path_connecting(&self.dag, *to_idx, *from_idx, None) {
            return Err(SchedError::DependencyCycle);
        }

        self.dag.add_edge(*from_idx, *to_idx, ());
        Ok(())
    }

    /// Get a job by ID.
    pub fn get_job(&self, job_id: &ScheduledJobId) -> Option<&ScheduledJob> {
        self.job_index
            .get(job_id)
            .and_then(|idx| self.dag.node_weight(*idx))
            .map(|node| &node.job)
    }

    /// Get a mutable reference to a job by ID.
    pub fn get_job_mut(&mut self, job_id: &ScheduledJobId) -> Option<&mut ScheduledJob> {
        self.job_index
            .get(job_id)
            .copied()
            .and_then(|idx| self.dag.node_weight_mut(idx))
            .map(|node| &mut node.job)
    }

    /// Mark a job as completed.
    pub fn mark_completed(&mut self, job_id: &ScheduledJobId) -> SchedResult<()> {
        let idx = self
            .job_index
            .get(job_id)
            .ok_or_else(|| SchedError::JobNotFound(job_id.to_string()))?;

        if let Some(node) = self.dag.node_weight_mut(*idx) {
            node.completed = true;
            Ok(())
        } else {
            Err(SchedError::JobNotFound(job_id.to_string()))
        }
    }

    /// Mark a job as failed.
    pub fn mark_failed(&mut self, job_id: &ScheduledJobId) -> SchedResult<()> {
        let idx = self
            .job_index
            .get(job_id)
            .ok_or_else(|| SchedError::JobNotFound(job_id.to_string()))?;

        if let Some(node) = self.dag.node_weight_mut(*idx) {
            node.failed = true;
            Ok(())
        } else {
            Err(SchedError::JobNotFound(job_id.to_string()))
        }
    }

    /// Get jobs that are ready to run (all dependencies satisfied).
    pub fn ready_jobs(&self) -> Vec<&ScheduledJob> {
        self.dag
            .node_indices()
            .filter_map(|idx| {
                let node = self.dag.node_weight(idx)?;

                // Skip completed or failed jobs
                if node.completed || node.failed {
                    return None;
                }

                // Check if job is already running
                if !node.job.status.is_pending() {
                    return None;
                }

                // Check all dependencies are completed
                let deps_satisfied =
                    self.dag
                        .edges_directed(idx, Direction::Incoming)
                        .all(|edge| {
                            self.dag
                                .node_weight(edge.source())
                                .is_some_and(|n| n.completed)
                        });

                if deps_satisfied {
                    Some(&node.job)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all jobs in topological order.
    pub fn topological_order(&self) -> Vec<&ScheduledJob> {
        match petgraph::algo::toposort(&self.dag, None) {
            Ok(order) => order
                .into_iter()
                .filter_map(|idx| self.dag.node_weight(idx).map(|n| &n.job))
                .collect(),
            Err(_) => {
                // Shouldn't happen if we check for cycles on add
                Vec::new()
            }
        }
    }

    /// Get all jobs in the workflow.
    pub fn all_jobs(&self) -> Vec<&ScheduledJob> {
        self.dag
            .node_indices()
            .filter_map(|idx| self.dag.node_weight(idx).map(|n| &n.job))
            .collect()
    }

    /// Get all job IDs in the workflow.
    pub fn job_ids(&self) -> Vec<&ScheduledJobId> {
        self.job_index.keys().collect()
    }

    /// Get the number of jobs in the workflow.
    pub fn len(&self) -> usize {
        self.dag.node_count()
    }

    /// Check if the workflow is empty.
    pub fn is_empty(&self) -> bool {
        self.dag.node_count() == 0
    }

    /// Get the number of completed jobs.
    pub fn completed_count(&self) -> usize {
        self.dag
            .node_indices()
            .filter(|idx| self.dag.node_weight(*idx).is_some_and(|n| n.completed))
            .count()
    }

    /// Get the number of failed jobs.
    pub fn failed_count(&self) -> usize {
        self.dag
            .node_indices()
            .filter(|idx| self.dag.node_weight(*idx).is_some_and(|n| n.failed))
            .count()
    }

    /// Check if the workflow is complete.
    pub fn is_complete(&self) -> bool {
        self.dag.node_indices().all(|idx| {
            self.dag
                .node_weight(idx)
                .is_none_or(|n| n.completed || n.failed)
        })
    }

    /// Check if any job in the workflow has failed.
    pub fn has_failures(&self) -> bool {
        self.failed_count() > 0
    }

    /// Get dependencies of a job.
    pub fn dependencies(&self, job_id: &ScheduledJobId) -> Vec<&ScheduledJobId> {
        let Some(idx) = self.job_index.get(job_id) else {
            return Vec::new();
        };

        self.dag
            .edges_directed(*idx, Direction::Incoming)
            .filter_map(|edge| self.dag.node_weight(edge.source()).map(|n| &n.job.id))
            .collect()
    }

    /// Get dependents of a job (jobs that depend on this job).
    pub fn dependents(&self, job_id: &ScheduledJobId) -> Vec<&ScheduledJobId> {
        let Some(idx) = self.job_index.get(job_id) else {
            return Vec::new();
        };

        self.dag
            .edges_directed(*idx, Direction::Outgoing)
            .filter_map(|edge| self.dag.node_weight(edge.target()).map(|n| &n.job.id))
            .collect()
    }

    /// Update workflow status based on job states.
    pub fn update_status(&mut self) {
        if self.is_complete() {
            if self.has_failures() {
                self.status = WorkflowStatus::Failed {
                    reason: format!("{} job(s) failed", self.failed_count()),
                };
            } else {
                self.status = WorkflowStatus::Completed;
            }
            self.completed_at = Some(Utc::now());
        } else if self.dag.node_indices().any(|idx| {
            self.dag
                .node_weight(idx)
                .is_some_and(|n| n.job.status.is_running())
        }) {
            self.status = WorkflowStatus::Running;
        }
    }
}

/// Builder for creating workflows with a fluent API.
pub struct WorkflowBuilder {
    workflow: Workflow,
    last_job_id: Option<ScheduledJobId>,
}

impl WorkflowBuilder {
    /// Create a new workflow builder.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            workflow: Workflow::new(name),
            last_job_id: None,
        }
    }

    /// Add a job to the workflow.
    pub fn add_job(mut self, job: ScheduledJob) -> Self {
        self.last_job_id = Some(job.id.clone());
        self.workflow.add_job(job);
        self
    }

    /// Add a job that depends on the previously added job.
    pub fn then(mut self, job: ScheduledJob) -> SchedResult<Self> {
        let Some(ref prev_id) = self.last_job_id else {
            // No previous job, just add it
            self.last_job_id = Some(job.id.clone());
            self.workflow.add_job(job);
            return Ok(self);
        };

        let current_id = job.id.clone();
        self.workflow.add_job(job);
        self.workflow.add_dependency(prev_id, &current_id)?;
        self.last_job_id = Some(current_id);
        Ok(self)
    }

    /// Add a job that depends on a specific job.
    pub fn add_job_after(
        mut self,
        job: ScheduledJob,
        depends_on: &ScheduledJobId,
    ) -> SchedResult<Self> {
        let current_id = job.id.clone();
        self.workflow.add_job(job);
        self.workflow.add_dependency(depends_on, &current_id)?;
        self.last_job_id = Some(current_id);
        Ok(self)
    }

    /// Add a job that depends on multiple jobs.
    pub fn add_job_after_all(
        mut self,
        job: ScheduledJob,
        depends_on: &[ScheduledJobId],
    ) -> SchedResult<Self> {
        let current_id = job.id.clone();
        self.workflow.add_job(job);

        for dep_id in depends_on {
            self.workflow.add_dependency(dep_id, &current_id)?;
        }

        self.last_job_id = Some(current_id);
        Ok(self)
    }

    /// Build the workflow.
    pub fn build(self) -> Workflow {
        self.workflow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::CircuitSpec;

    fn make_job(name: &str) -> ScheduledJob {
        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q;");
        ScheduledJob::new(name, circuit)
    }

    #[test]
    fn test_workflow_basic() {
        let mut workflow = Workflow::new("test_workflow");

        let job1 = make_job("job1");
        let job1_id = job1.id.clone();
        workflow.add_job(job1);

        let job2 = make_job("job2");
        let job2_id = job2.id.clone();
        workflow.add_job(job2);

        assert_eq!(workflow.len(), 2);
        assert!(workflow.get_job(&job1_id).is_some());
        assert!(workflow.get_job(&job2_id).is_some());
    }

    #[test]
    fn test_workflow_dependencies() {
        let mut workflow = Workflow::new("test_workflow");

        let job1 = make_job("job1");
        let job1_id = job1.id.clone();
        workflow.add_job(job1);

        let job2 = make_job("job2");
        let job2_id = job2.id.clone();
        workflow.add_job(job2);

        // job2 depends on job1
        workflow.add_dependency(&job1_id, &job2_id).unwrap();

        // Initially only job1 is ready
        let ready = workflow.ready_jobs();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].name, "job1");

        // Mark job1 completed
        workflow.mark_completed(&job1_id).unwrap();

        // Now job2 should be ready
        let ready = workflow.ready_jobs();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].name, "job2");
    }

    #[test]
    fn test_workflow_cycle_detection() {
        let mut workflow = Workflow::new("test_workflow");

        let job1 = make_job("job1");
        let job1_id = job1.id.clone();
        workflow.add_job(job1);

        let job2 = make_job("job2");
        let job2_id = job2.id.clone();
        workflow.add_job(job2);

        let job3 = make_job("job3");
        let job3_id = job3.id.clone();
        workflow.add_job(job3);

        // Create chain: job1 -> job2 -> job3
        workflow.add_dependency(&job1_id, &job2_id).unwrap();
        workflow.add_dependency(&job2_id, &job3_id).unwrap();

        // Try to create cycle: job3 -> job1 (should fail)
        let result = workflow.add_dependency(&job3_id, &job1_id);
        assert!(matches!(result, Err(SchedError::DependencyCycle)));
    }

    #[test]
    fn test_workflow_topological_order() {
        let mut workflow = Workflow::new("test_workflow");

        let job1 = make_job("job1");
        let job1_id = job1.id.clone();
        workflow.add_job(job1);

        let job2 = make_job("job2");
        let job2_id = job2.id.clone();
        workflow.add_job(job2);

        let job3 = make_job("job3");
        let job3_id = job3.id.clone();
        workflow.add_job(job3);

        // job2 and job3 depend on job1
        workflow.add_dependency(&job1_id, &job2_id).unwrap();
        workflow.add_dependency(&job1_id, &job3_id).unwrap();

        let order = workflow.topological_order();
        assert_eq!(order.len(), 3);
        // job1 should be first
        assert_eq!(order[0].name, "job1");
    }

    #[test]
    fn test_workflow_builder() {
        let job1 = make_job("job1");
        let job1_id = job1.id.clone();

        let job2 = make_job("job2");
        let job3 = make_job("job3");

        let workflow = WorkflowBuilder::new("test")
            .add_job(job1)
            .then(job2)
            .unwrap()
            .then(job3)
            .unwrap()
            .build();

        assert_eq!(workflow.len(), 3);

        // Only first job should be ready
        let ready = workflow.ready_jobs();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, job1_id);
    }

    #[test]
    fn test_workflow_completion() {
        let mut workflow = Workflow::new("test_workflow");

        let job1 = make_job("job1");
        let job1_id = job1.id.clone();
        workflow.add_job(job1);

        let job2 = make_job("job2");
        let job2_id = job2.id.clone();
        workflow.add_job(job2);

        assert!(!workflow.is_complete());

        workflow.mark_completed(&job1_id).unwrap();
        assert!(!workflow.is_complete());

        workflow.mark_completed(&job2_id).unwrap();
        assert!(workflow.is_complete());
        assert!(!workflow.has_failures());
    }

    #[test]
    fn test_workflow_failure() {
        let mut workflow = Workflow::new("test_workflow");

        let job1 = make_job("job1");
        let job1_id = job1.id.clone();
        workflow.add_job(job1);

        let job2 = make_job("job2");
        let job2_id = job2.id.clone();
        workflow.add_job(job2);

        workflow.mark_completed(&job1_id).unwrap();
        workflow.mark_failed(&job2_id).unwrap();

        assert!(workflow.is_complete());
        assert!(workflow.has_failures());
        assert_eq!(workflow.failed_count(), 1);
    }
}
