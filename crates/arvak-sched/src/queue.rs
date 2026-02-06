//! Priority queue for job scheduling.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::job::{Priority, ScheduledJob, ScheduledJobId};

/// Entry in the priority queue.
#[derive(Debug)]
struct QueueEntry {
    /// Job ID.
    job_id: ScheduledJobId,

    /// Job priority.
    priority: Priority,

    /// Insertion order (for FIFO ordering of same-priority jobs).
    insertion_order: u64,
}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.job_id == other.job_id
    }
}

impl Eq for QueueEntry {}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first
        match self.priority.cmp(&other.priority) {
            Ordering::Equal => {
                // For same priority, earlier insertion first (FIFO)
                // Note: BinaryHeap is a max-heap, so we reverse the comparison
                other.insertion_order.cmp(&self.insertion_order)
            }
            other_cmp => other_cmp,
        }
    }
}

/// A priority queue for scheduled jobs.
///
/// Jobs with higher priority are dequeued first. Among jobs with the same
/// priority, jobs are dequeued in FIFO order.
#[derive(Debug)]
pub struct PriorityQueue {
    heap: BinaryHeap<QueueEntry>,
    jobs: rustc_hash::FxHashMap<ScheduledJobId, ScheduledJob>,
    insertion_counter: u64,
}

impl Default for PriorityQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl PriorityQueue {
    /// Create a new empty priority queue.
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
            jobs: rustc_hash::FxHashMap::default(),
            insertion_counter: 0,
        }
    }

    /// Create a priority queue with the given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            heap: BinaryHeap::with_capacity(capacity),
            jobs: rustc_hash::FxHashMap::with_capacity_and_hasher(
                capacity,
                rustc_hash::FxBuildHasher::default(),
            ),
            insertion_counter: 0,
        }
    }

    /// Push a job onto the queue.
    pub fn push(&mut self, job: ScheduledJob) {
        let entry = QueueEntry {
            job_id: job.id.clone(),
            priority: job.priority,
            insertion_order: self.insertion_counter,
        };
        self.insertion_counter += 1;
        self.jobs.insert(job.id.clone(), job);
        self.heap.push(entry);
    }

    /// Pop the highest priority job from the queue.
    pub fn pop(&mut self) -> Option<ScheduledJob> {
        // Skip entries for jobs that have been removed
        while let Some(entry) = self.heap.pop() {
            if let Some(job) = self.jobs.remove(&entry.job_id) {
                return Some(job);
            }
            // Job was removed via remove(), skip this entry
        }
        None
    }

    /// Peek at the highest priority job without removing it.
    pub fn peek(&self) -> Option<&ScheduledJob> {
        // Find the first entry that still has a corresponding job
        for entry in self.heap.iter() {
            if let Some(job) = self.jobs.get(&entry.job_id) {
                return Some(job);
            }
        }
        None
    }

    /// Get a job by ID.
    pub fn get(&self, job_id: &ScheduledJobId) -> Option<&ScheduledJob> {
        self.jobs.get(job_id)
    }

    /// Get a mutable reference to a job by ID.
    pub fn get_mut(&mut self, job_id: &ScheduledJobId) -> Option<&mut ScheduledJob> {
        self.jobs.get_mut(job_id)
    }

    /// Remove a job from the queue by ID.
    ///
    /// Returns the removed job if found.
    pub fn remove(&mut self, job_id: &ScheduledJobId) -> Option<ScheduledJob> {
        // We don't remove from the heap directly (O(n) operation).
        // Instead, we just remove from the jobs map and let pop() skip invalid entries.
        self.jobs.remove(job_id)
    }

    /// Check if a job is in the queue.
    pub fn contains(&self, job_id: &ScheduledJobId) -> bool {
        self.jobs.contains_key(job_id)
    }

    /// Get the number of jobs in the queue.
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Check if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// Clear all jobs from the queue.
    pub fn clear(&mut self) {
        self.heap.clear();
        self.jobs.clear();
    }

    /// Iterate over all jobs in the queue (not in priority order).
    pub fn iter(&self) -> impl Iterator<Item = &ScheduledJob> {
        self.jobs.values()
    }

    /// Get all job IDs in the queue.
    pub fn job_ids(&self) -> impl Iterator<Item = &ScheduledJobId> {
        self.jobs.keys()
    }

    /// Update a job's priority.
    ///
    /// This re-inserts the job with the new priority. The old entry in the heap
    /// will be skipped on pop().
    pub fn update_priority(&mut self, job_id: &ScheduledJobId, new_priority: Priority) -> bool {
        if let Some(job) = self.jobs.get_mut(job_id) {
            job.priority = new_priority;
            // Add a new entry with the updated priority
            let entry = QueueEntry {
                job_id: job_id.clone(),
                priority: new_priority,
                insertion_order: self.insertion_counter,
            };
            self.insertion_counter += 1;
            self.heap.push(entry);
            true
        } else {
            false
        }
    }

    /// Drain all jobs whose dependencies are satisfied.
    ///
    /// Returns jobs in priority order.
    pub fn drain_ready(
        &mut self,
        completed: &rustc_hash::FxHashSet<ScheduledJobId>,
    ) -> Vec<ScheduledJob> {
        let mut ready = Vec::new();
        let mut to_remove = Vec::new();

        // Collect jobs that are ready
        for (job_id, job) in &self.jobs {
            if job.dependencies_satisfied(completed) {
                to_remove.push(job_id.clone());
            }
        }

        // Remove ready jobs and collect them
        for job_id in to_remove {
            if let Some(job) = self.jobs.remove(&job_id) {
                ready.push(job);
            }
        }

        // Sort by priority (highest first), then by insertion order
        ready.sort_by(|a, b| match b.priority.cmp(&a.priority) {
            Ordering::Equal => a.created_at.cmp(&b.created_at),
            other => other,
        });

        ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::CircuitSpec;

    fn make_job(name: &str, priority: Priority) -> ScheduledJob {
        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q;");
        ScheduledJob::new(name, circuit).with_priority(priority)
    }

    #[test]
    fn test_priority_ordering() {
        let mut queue = PriorityQueue::new();

        let low = make_job("low", Priority::low());
        let default = make_job("default", Priority::default());
        let high = make_job("high", Priority::high());

        // Push in random order
        queue.push(default);
        queue.push(low);
        queue.push(high);

        // Should pop in priority order
        assert_eq!(queue.pop().unwrap().name, "high");
        assert_eq!(queue.pop().unwrap().name, "default");
        assert_eq!(queue.pop().unwrap().name, "low");
        assert!(queue.pop().is_none());
    }

    #[test]
    fn test_fifo_same_priority() {
        let mut queue = PriorityQueue::new();

        let job1 = make_job("first", Priority::default());
        let job2 = make_job("second", Priority::default());
        let job3 = make_job("third", Priority::default());

        queue.push(job1);
        queue.push(job2);
        queue.push(job3);

        // Should pop in FIFO order for same priority
        assert_eq!(queue.pop().unwrap().name, "first");
        assert_eq!(queue.pop().unwrap().name, "second");
        assert_eq!(queue.pop().unwrap().name, "third");
    }

    #[test]
    fn test_remove() {
        let mut queue = PriorityQueue::new();

        let job1 = make_job("job1", Priority::default());
        let job2 = make_job("job2", Priority::default());
        let job1_id = job1.id.clone();

        queue.push(job1);
        queue.push(job2);

        // Remove job1
        let removed = queue.remove(&job1_id);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().name, "job1");

        // Should only pop job2
        assert_eq!(queue.pop().unwrap().name, "job2");
        assert!(queue.pop().is_none());
    }

    #[test]
    fn test_update_priority() {
        let mut queue = PriorityQueue::new();

        let low = make_job("was_low", Priority::low());
        let low_id = low.id.clone();
        let high = make_job("high", Priority::high());

        queue.push(low);
        queue.push(high);

        // Initially high should be first
        assert_eq!(queue.peek().unwrap().name, "high");

        // Update low to critical
        queue.update_priority(&low_id, Priority::critical());

        // Now was_low (now critical) should be first
        assert_eq!(queue.pop().unwrap().name, "was_low");
        assert_eq!(queue.pop().unwrap().name, "high");
    }

    #[test]
    fn test_drain_ready() {
        let mut queue = PriorityQueue::new();

        let job1 = make_job("job1", Priority::default());
        let job1_id = job1.id.clone();

        let job2 = make_job("job2", Priority::high()).depends_on(job1_id.clone());
        let job2_id = job2.id.clone();

        let job3 = make_job("job3", Priority::low());

        queue.push(job1);
        queue.push(job2);
        queue.push(job3);

        // With no completions, only jobs without dependencies should be ready
        let mut completed = rustc_hash::FxHashSet::default();
        let ready = queue.drain_ready(&completed);

        assert_eq!(ready.len(), 2);
        // Should be in priority order
        assert_eq!(ready[0].name, "job1");
        assert_eq!(ready[1].name, "job3");

        // job2 should still be in queue
        assert!(queue.contains(&job2_id));

        // Mark job1 as completed
        completed.insert(job1_id);

        // Now job2 should be ready
        let ready = queue.drain_ready(&completed);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].name, "job2");
    }

    #[test]
    fn test_len_and_is_empty() {
        let mut queue = PriorityQueue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);

        queue.push(make_job("job1", Priority::default()));
        assert!(!queue.is_empty());
        assert_eq!(queue.len(), 1);

        queue.push(make_job("job2", Priority::default()));
        assert_eq!(queue.len(), 2);

        queue.pop();
        assert_eq!(queue.len(), 1);

        queue.clear();
        assert!(queue.is_empty());
    }
}
