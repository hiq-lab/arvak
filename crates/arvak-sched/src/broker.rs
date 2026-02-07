//! Message broker abstraction for job routing.
//!
//! Provides a trait-based messaging layer for distributing quantum jobs
//! across different execution backends (cloud, HPC, local).
//!
//! # Implementations
//!
//! - [`InMemoryBroker`]: In-process channel-based broker for testing and
//!   single-node deployments.
//! - NATS: Available with `--features message-broker` (requires `async-nats`).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use crate::error::SchedResult;
use crate::job::ScheduledJobId;

/// A message routed through the broker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobMessage {
    /// Unique job identifier.
    pub job_id: ScheduledJobId,
    /// Routing subject/topic.
    pub subject: String,
    /// Serialized job payload (typically JSON).
    pub payload: Vec<u8>,
    /// Optional reply-to subject for request/response patterns.
    #[serde(default)]
    pub reply_to: Option<String>,
}

impl JobMessage {
    /// Create a new job message.
    pub fn new(
        job_id: ScheduledJobId,
        subject: impl Into<String>,
        payload: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            job_id,
            subject: subject.into(),
            payload: payload.into(),
            reply_to: None,
        }
    }

    /// Set the reply-to subject.
    pub fn with_reply_to(mut self, reply_to: impl Into<String>) -> Self {
        self.reply_to = Some(reply_to.into());
        self
    }
}

/// Message broker trait for job distribution.
#[async_trait]
pub trait MessageBroker: Send + Sync {
    /// Publish a message to a subject.
    async fn publish(&self, message: JobMessage) -> SchedResult<()>;

    /// Subscribe to a subject and receive messages.
    async fn subscribe(&self, subject: &str) -> SchedResult<Box<dyn MessageSubscription>>;

    /// Check if the broker is connected.
    async fn is_connected(&self) -> bool;
}

/// A subscription to a message subject.
#[async_trait]
pub trait MessageSubscription: Send + Sync {
    /// Receive the next message (blocks until available or timeout).
    async fn next(&mut self) -> SchedResult<Option<JobMessage>>;

    /// Unsubscribe from the subject.
    async fn unsubscribe(self: Box<Self>) -> SchedResult<()>;
}

/// In-memory message broker using tokio channels.
///
/// Suitable for testing and single-node deployments where all
/// components run in the same process.
pub struct InMemoryBroker {
    subscriptions: Arc<Mutex<Vec<(String, mpsc::Sender<JobMessage>)>>>,
}

impl InMemoryBroker {
    /// Create a new in-memory broker.
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Default for InMemoryBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MessageBroker for InMemoryBroker {
    async fn publish(&self, message: JobMessage) -> SchedResult<()> {
        let subs = self.subscriptions.lock().await;
        for (pattern, sender) in subs.iter() {
            if subject_matches(pattern, &message.subject) {
                let _ = sender.send(message.clone()).await;
            }
        }
        Ok(())
    }

    async fn subscribe(&self, subject: &str) -> SchedResult<Box<dyn MessageSubscription>> {
        let (tx, rx) = mpsc::channel(256);
        let mut subs = self.subscriptions.lock().await;
        subs.push((subject.to_string(), tx));
        Ok(Box::new(InMemorySubscription { receiver: rx }))
    }

    async fn is_connected(&self) -> bool {
        true
    }
}

/// In-memory subscription backed by a tokio mpsc channel.
struct InMemorySubscription {
    receiver: mpsc::Receiver<JobMessage>,
}

#[async_trait]
impl MessageSubscription for InMemorySubscription {
    async fn next(&mut self) -> SchedResult<Option<JobMessage>> {
        Ok(self.receiver.recv().await)
    }

    async fn unsubscribe(self: Box<Self>) -> SchedResult<()> {
        // Dropping the receiver is sufficient
        Ok(())
    }
}

/// Simple subject matching with wildcard support.
///
/// Supports `*` for single-token wildcards and `>` for multi-token wildcards
/// (NATS-style subjects).
fn subject_matches(pattern: &str, subject: &str) -> bool {
    if pattern == ">" || pattern == subject {
        return true;
    }

    let pattern_parts: Vec<&str> = pattern.split('.').collect();
    let subject_parts: Vec<&str> = subject.split('.').collect();

    let mut pi = 0;
    let mut si = 0;

    while pi < pattern_parts.len() && si < subject_parts.len() {
        match pattern_parts[pi] {
            ">" => return true, // Match rest
            "*" => {
                pi += 1;
                si += 1;
            }
            token => {
                if token != subject_parts[si] {
                    return false;
                }
                pi += 1;
                si += 1;
            }
        }
    }

    pi == pattern_parts.len() && si == subject_parts.len()
}

/// Standard job routing subjects.
pub mod subjects {
    /// Jobs routed to cloud backends.
    pub const CLOUD: &str = "jobs.cloud";
    /// Jobs routed to HPC schedulers.
    pub const HPC: &str = "jobs.hpc";
    /// Jobs routed to local simulator.
    pub const LOCAL: &str = "jobs.local";
    /// Job status updates.
    pub const STATUS: &str = "jobs.status";
    /// Job results.
    pub const RESULTS: &str = "jobs.results";
    /// All job events (wildcard).
    pub const ALL: &str = "jobs.>";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subject_matching() {
        assert!(subject_matches("jobs.cloud", "jobs.cloud"));
        assert!(!subject_matches("jobs.cloud", "jobs.hpc"));
        assert!(subject_matches("jobs.*", "jobs.cloud"));
        assert!(subject_matches("jobs.*", "jobs.hpc"));
        assert!(!subject_matches("jobs.*", "jobs.cloud.iqm"));
        assert!(subject_matches("jobs.>", "jobs.cloud"));
        assert!(subject_matches("jobs.>", "jobs.cloud.iqm"));
        assert!(subject_matches(">", "anything.at.all"));
    }

    #[test]
    fn test_job_message() {
        let msg = JobMessage::new(
            ScheduledJobId::new(),
            "jobs.cloud",
            b"test payload".to_vec(),
        )
        .with_reply_to("reply.123");

        assert_eq!(msg.subject, "jobs.cloud");
        assert_eq!(msg.payload, b"test payload");
        assert_eq!(msg.reply_to, Some("reply.123".to_string()));
    }

    #[tokio::test]
    async fn test_in_memory_broker_pub_sub() {
        let broker = InMemoryBroker::new();
        assert!(broker.is_connected().await);

        let mut sub = broker.subscribe("jobs.cloud").await.unwrap();

        let msg = JobMessage::new(
            ScheduledJobId::new(),
            "jobs.cloud",
            b"hello".to_vec(),
        );
        broker.publish(msg.clone()).await.unwrap();

        let received = sub.next().await.unwrap().unwrap();
        assert_eq!(received.subject, "jobs.cloud");
        assert_eq!(received.payload, b"hello");
    }

    #[tokio::test]
    async fn test_in_memory_broker_wildcard() {
        let broker = InMemoryBroker::new();

        let mut sub = broker.subscribe("jobs.*").await.unwrap();

        // Publish to different subjects
        broker
            .publish(JobMessage::new(
                ScheduledJobId::new(),
                "jobs.cloud",
                b"cloud".to_vec(),
            ))
            .await
            .unwrap();

        broker
            .publish(JobMessage::new(
                ScheduledJobId::new(),
                "jobs.hpc",
                b"hpc".to_vec(),
            ))
            .await
            .unwrap();

        let msg1 = sub.next().await.unwrap().unwrap();
        let msg2 = sub.next().await.unwrap().unwrap();

        assert_eq!(msg1.payload, b"cloud");
        assert_eq!(msg2.payload, b"hpc");
    }

    #[tokio::test]
    async fn test_in_memory_broker_no_match() {
        let broker = InMemoryBroker::new();
        let mut sub = broker.subscribe("jobs.cloud").await.unwrap();

        // Publish to different subject — should not be received
        broker
            .publish(JobMessage::new(
                ScheduledJobId::new(),
                "jobs.hpc",
                b"hpc".to_vec(),
            ))
            .await
            .unwrap();

        // Use a timeout to verify no message received
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            sub.next(),
        )
        .await;

        assert!(result.is_err()); // Timeout = no message
    }
}
