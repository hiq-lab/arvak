//! Arvak gRPC Service API
//!
//! This crate provides a production-ready gRPC service for remote quantum circuit
//! submission and execution. It enables language-agnostic access to Arvak backends
//! while maintaining backward compatibility with local execution.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Arvak gRPC Service                        │
//! │                                                               │
//! │  - ArvakServiceImpl (server/service.rs)                      │
//! │  - JobStore (server/job_store.rs)                            │
//! │  - BackendRegistry (server/backend_registry.rs)              │
//! │  - Async job execution                                       │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Features
//!
//! - **simulator**: Enable local simulator backend (default)
//!
//! # Example
//!
//! ```rust,no_run
//! use arvak_grpc::server::{ArvakServiceImpl, JobStore, BackendRegistry};
//! use tonic::transport::Server;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let service = ArvakServiceImpl::new();
//!     let addr = "0.0.0.0:50051".parse()?;
//!
//!     println!("Arvak gRPC server listening on {}", addr);
//!
//!     Server::builder()
//!         .add_service(arvak_grpc::proto::arvak_service_server::ArvakServiceServer::new(service))
//!         .serve(addr)
//!         .await?;
//!
//!     Ok(())
//! }
//! ```

pub mod error;
pub mod metrics;
pub mod proto;
pub mod server;
pub mod storage;

// Re-export commonly used types
pub use error::{Error, Result};
pub use metrics::Metrics;
pub use server::{ArvakServiceImpl, BackendRegistry, JobStore};
pub use storage::{JobStorage, MemoryStorage, StoredJob};
