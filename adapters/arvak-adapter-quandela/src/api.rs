//! Quandela REST API client (stub).
//!
//! DEBT-Q5: The Quandela cloud API endpoint for direct circuit submission is not
//! yet publicly documented. This client currently implements only an availability
//! ping (non-empty API key check) until the submission endpoint is confirmed.

use std::time::Duration;

use reqwest::Client;

use crate::error::{QuandelaError, QuandelaResult};

/// Quandela REST API client.
///
/// Authenticates via an API key read from `QUANDELA_API_KEY`.
/// DEBT-Q5: Real API endpoint and submission format TBD.
pub struct QuandelaClient {
    /// HTTP client with timeouts configured.
    #[allow(dead_code)]
    client: Client,
    /// API key for authentication.
    api_key: String,
}

impl std::fmt::Debug for QuandelaClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuandelaClient")
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

impl QuandelaClient {
    /// Create a new client from an API key.
    pub fn new(api_key: impl Into<String>) -> QuandelaResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(QuandelaError::Http)?;

        Ok(Self {
            client,
            api_key: api_key.into(),
        })
    }

    /// Ping the Quandela service.
    ///
    /// Returns `Ok(())` if the API key is non-empty (stub implementation).
    /// DEBT-Q5: Replace with a real availability endpoint once documented.
    pub async fn ping(&self) -> QuandelaResult<()> {
        if self.api_key.is_empty() {
            Err(QuandelaError::MissingApiKey)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ping_non_empty_key() {
        let client = QuandelaClient::new("test-key").unwrap();
        assert!(client.ping().await.is_ok());
    }

    #[tokio::test]
    async fn test_ping_empty_key() {
        let client = QuandelaClient::new("").unwrap();
        let err = client.ping().await.unwrap_err();
        assert!(matches!(err, QuandelaError::MissingApiKey));
    }
}
