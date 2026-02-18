//! Arvak Adapter for AWS Braket
//!
//! This crate provides a backend implementation for connecting to quantum
//! hardware and simulators available through the AWS Braket service.
//!
//! # Supported Devices
//!
//! | Device | Qubits | Provider | Native Gates |
//! |--------|--------|----------|--------------|
//! | Rigetti Ankaa-3 | 84 | Rigetti | RX, RZ, CZ |
//! | IonQ Aria | 25 | IonQ | RX, RY, RZ, XX |
//! | IonQ Forte | 36 | IonQ | RX, RY, RZ, XX |
//! | IQM Garnet | 20 | IQM | PRX, CZ |
//! | Amazon SV1 | 34 | Amazon | Universal |
//! | Amazon TN1 | 50 | Amazon | Universal |
//! | Amazon DM1 | 17 | Amazon | Universal |
//!
//! # Authentication
//!
//! AWS credentials are loaded from the standard AWS credential chain:
//! environment variables, shared config, SSO, or IAM role.
//!
//! Required environment variables:
//! - `ARVAK_BRAKET_S3_BUCKET` — S3 bucket for storing task results
//!
//! Optional environment variables:
//! - `ARVAK_BRAKET_S3_PREFIX` — S3 key prefix (default: `"arvak-results"`)
//! - `AWS_REGION` — AWS region (default: `"us-east-1"`)
//!
//! # Example
//!
//! ```ignore
//! use arvak_adapter_braket::BraketBackend;
//! use arvak_hal::Backend;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Connect to the SV1 simulator
//!     let backend = BraketBackend::connect(
//!         "arn:aws:braket:::device/quantum-simulator/amazon/sv1"
//!     ).await?;
//!
//!     let caps = backend.capabilities();
//!     println!("Max qubits: {}", caps.num_qubits);
//!
//!     Ok(())
//! }
//! ```

mod api;
mod backend;
pub mod device;
mod error;

pub use backend::BraketBackend;
pub use error::{BraketError, BraketResult};

// Re-export common types
pub use arvak_hal::Backend;
