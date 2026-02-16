//! Resource management and quota enforcement.
//!
//! This module provides resource tracking and enforcement of limits:
//! - Maximum concurrent jobs
//! - Maximum queued jobs
//! - Job timeouts
//! - Result size limits
//! - Rate limiting per client

use crate::config::ResourceLimits;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Resource manager for tracking and enforcing limits.
#[derive(Clone)]
pub struct ResourceManager {
    limits: ResourceLimits,
    state: Arc<RwLock<ResourceState>>,
}

/// Internal resource state tracking.
struct ResourceState {
    /// Number of currently running jobs
    running_jobs: usize,

    /// Number of queued jobs
    queued_jobs: usize,

    /// Rate limiting state per client IP
    rate_limits: HashMap<String, RateLimitState>,
}

/// Rate limiting state for a single client (fixed-window counter, O(1) memory).
struct RateLimitState {
    /// Number of requests in the current window
    count: u32,

    /// Start of the current 1-second window
    window_start: Instant,
}

impl ResourceManager {
    /// Create a new resource manager with the given limits.
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            limits,
            state: Arc::new(RwLock::new(ResourceState {
                running_jobs: 0,
                queued_jobs: 0,
                rate_limits: HashMap::new(),
            })),
        }
    }

    /// Check if a new job can be accepted (queued).
    ///
    /// Returns Ok(()) if the job can be accepted, or Err with a reason if rejected.
    pub async fn check_can_submit(&self, client_ip: Option<&str>) -> Result<(), ResourceError> {
        let state = self.state.read().await;

        // Check concurrent job limit
        if state.running_jobs >= self.limits.max_concurrent_jobs {
            return Err(ResourceError::ConcurrencyLimitReached {
                current: state.running_jobs,
                limit: self.limits.max_concurrent_jobs,
            });
        }

        // Check queue limit
        if state.queued_jobs >= self.limits.max_queued_jobs {
            return Err(ResourceError::QueueFull {
                current: state.queued_jobs,
                limit: self.limits.max_queued_jobs,
            });
        }

        // Check rate limit if client IP is provided
        if let Some(ip) = client_ip {
            if let Some(rate_state) = state.rate_limits.get(ip) {
                let now = Instant::now();
                let window = Duration::from_secs(1);

                // If within current window, check count
                if now.duration_since(rate_state.window_start) < window
                    && rate_state.count >= self.limits.rate_limit_rps
                {
                    return Err(ResourceError::RateLimitExceeded {
                        current_rps: rate_state.count,
                        limit_rps: self.limits.rate_limit_rps,
                    });
                }
            }
        }

        Ok(())
    }

    /// Mark a job as submitted (queued).
    pub async fn job_submitted(&self, client_ip: Option<&str>) {
        let mut state = self.state.write().await;
        state.queued_jobs += 1;

        // Update rate limit tracking
        if let Some(ip) = client_ip {
            let now = Instant::now();
            let window = Duration::from_secs(1);
            let rate_state =
                state
                    .rate_limits
                    .entry(ip.to_string())
                    .or_insert_with(|| RateLimitState {
                        count: 0,
                        window_start: now,
                    });

            // If window expired, reset
            if now.duration_since(rate_state.window_start) >= window {
                rate_state.count = 1;
                rate_state.window_start = now;
            } else {
                rate_state.count += 1;
            }
        }
    }

    /// Mark a job as started (moved from queued to running).
    pub async fn job_started(&self) {
        let mut state = self.state.write().await;
        if state.queued_jobs > 0 {
            state.queued_jobs -= 1;
        }
        state.running_jobs += 1;
    }

    /// Mark a job as completed (no longer running).
    pub async fn job_completed(&self) {
        let mut state = self.state.write().await;
        if state.running_jobs > 0 {
            state.running_jobs -= 1;
        }
    }

    /// Mark a job as cancelled before it started.
    pub async fn job_cancelled_queued(&self) {
        let mut state = self.state.write().await;
        if state.queued_jobs > 0 {
            state.queued_jobs -= 1;
        }
    }

    /// Check if the result size is within limits.
    pub fn check_result_size(&self, size_bytes: usize) -> Result<(), ResourceError> {
        if size_bytes > self.limits.max_result_size_bytes {
            Err(ResourceError::ResultTooLarge {
                size_bytes,
                limit_bytes: self.limits.max_result_size_bytes,
            })
        } else {
            Ok(())
        }
    }

    /// Get the job timeout duration.
    pub fn job_timeout(&self) -> Duration {
        Duration::from_secs(self.limits.job_timeout_seconds)
    }

    /// Get current resource usage statistics.
    pub async fn stats(&self) -> ResourceStats {
        let state = self.state.read().await;
        ResourceStats {
            running_jobs: state.running_jobs,
            queued_jobs: state.queued_jobs,
            max_concurrent_jobs: self.limits.max_concurrent_jobs,
            max_queued_jobs: self.limits.max_queued_jobs,
        }
    }

    /// Clean up old rate limit state (call periodically).
    pub async fn cleanup_rate_limits(&self) {
        let mut state = self.state.write().await;
        let now = Instant::now();
        let cleanup_window = Duration::from_secs(60);

        // Remove entries whose window started more than 60s ago
        state
            .rate_limits
            .retain(|_, rate_state| now.duration_since(rate_state.window_start) < cleanup_window);
    }
}

/// Resource usage statistics.
#[derive(Debug, Clone)]
pub struct ResourceStats {
    pub running_jobs: usize,
    pub queued_jobs: usize,
    pub max_concurrent_jobs: usize,
    pub max_queued_jobs: usize,
}

/// Resource limit errors.
#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("Concurrency limit reached: {current} jobs running (limit: {limit})")]
    ConcurrencyLimitReached { current: usize, limit: usize },

    #[error("Queue full: {current} jobs queued (limit: {limit})")]
    QueueFull { current: usize, limit: usize },

    #[error("Rate limit exceeded: {current_rps} req/s (limit: {limit_rps} req/s)")]
    RateLimitExceeded { current_rps: u32, limit_rps: u32 },

    #[error("Result too large: {size_bytes} bytes (limit: {limit_bytes} bytes)")]
    ResultTooLarge {
        size_bytes: usize,
        limit_bytes: usize,
    },

    #[error("Job timeout exceeded")]
    Timeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_limits() -> ResourceLimits {
        ResourceLimits {
            max_concurrent_jobs: 10,
            max_queued_jobs: 100,
            job_timeout_seconds: 60,
            max_result_size_bytes: 1024,
            rate_limit_rps: 10,
        }
    }

    #[tokio::test]
    async fn test_job_lifecycle() {
        let manager = ResourceManager::new(test_limits());

        // Submit job
        assert!(manager.check_can_submit(None).await.is_ok());
        manager.job_submitted(None).await;

        let stats = manager.stats().await;
        assert_eq!(stats.queued_jobs, 1);
        assert_eq!(stats.running_jobs, 0);

        // Start job
        manager.job_started().await;
        let stats = manager.stats().await;
        assert_eq!(stats.queued_jobs, 0);
        assert_eq!(stats.running_jobs, 1);

        // Complete job
        manager.job_completed().await;
        let stats = manager.stats().await;
        assert_eq!(stats.queued_jobs, 0);
        assert_eq!(stats.running_jobs, 0);
    }

    #[tokio::test]
    async fn test_queue_limit() {
        let manager = ResourceManager::new(test_limits());

        // Fill the queue
        for _ in 0..100 {
            manager.job_submitted(None).await;
        }

        // Next submission should fail
        assert!(manager.check_can_submit(None).await.is_err());
    }

    #[tokio::test]
    async fn test_rate_limit() {
        let mut limits = test_limits();
        limits.rate_limit_rps = 2; // 2 requests per second
        let manager = ResourceManager::new(limits);

        let client_ip = "127.0.0.1";

        // First 2 requests should succeed
        assert!(manager.check_can_submit(Some(client_ip)).await.is_ok());
        manager.job_submitted(Some(client_ip)).await;

        assert!(manager.check_can_submit(Some(client_ip)).await.is_ok());
        manager.job_submitted(Some(client_ip)).await;

        // Third request should fail
        assert!(manager.check_can_submit(Some(client_ip)).await.is_err());

        // After 1 second, should work again
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(manager.check_can_submit(Some(client_ip)).await.is_ok());
    }

    #[tokio::test]
    async fn test_result_size_limit() {
        let manager = ResourceManager::new(test_limits());

        // Within limit
        assert!(manager.check_result_size(512).is_ok());

        // Exceeds limit
        assert!(manager.check_result_size(2048).is_err());
    }

    #[test]
    fn test_job_timeout() {
        let manager = ResourceManager::new(test_limits());
        assert_eq!(manager.job_timeout(), Duration::from_secs(60));
    }
}
