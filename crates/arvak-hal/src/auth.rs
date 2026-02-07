//! Authentication module for HPC quantum backends.
//!
//! This module provides OIDC (OpenID Connect) authentication support for
//! accessing quantum computers at HPC sites like LUMI (Finland) and LRZ (Germany).
//!
//! # Example
//!
//! ```ignore
//! use arvak_hal::auth::{OidcAuth, OidcConfig};
//!
//! // Create OIDC config for LUMI
//! let config = OidcConfig::lumi("project_462000123");
//!
//! // Authenticate
//! let auth = OidcAuth::new(config)?;
//! let token = auth.get_token().await?;
//!
//! // Use token with IQM backend
//! let backend = IqmBackend::with_credentials(
//!     "https://qpu.lumi.csc.fi",
//!     token,
//!     "helmi",
//! )?;
//! ```

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::{HalError, HalResult};

/// OIDC provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    /// Provider name (e.g., "csc", "lrz").
    pub provider: String,

    /// Authorization endpoint URL.
    pub auth_endpoint: String,

    /// Token endpoint URL.
    pub token_endpoint: String,

    /// Client ID for the OIDC application.
    pub client_id: String,

    /// Client secret (optional for public clients).
    pub client_secret: Option<String>,

    /// Redirect URI for OAuth flow.
    pub redirect_uri: String,

    /// Scopes to request.
    pub scopes: Vec<String>,

    /// Project/account identifier (site-specific).
    pub project_id: Option<String>,

    /// Path to cache tokens.
    pub token_cache_path: Option<PathBuf>,

    /// Token refresh buffer (refresh before expiry).
    pub refresh_buffer_secs: u64,
}

impl OidcConfig {
    /// Create configuration for LUMI (CSC, Finland).
    ///
    /// LUMI hosts IQM's Helmi quantum computer.
    pub fn lumi(project_id: &str) -> Self {
        Self {
            provider: "csc".to_string(),
            auth_endpoint: "https://auth.csc.fi/oauth2/authorize".to_string(),
            token_endpoint: "https://auth.csc.fi/oauth2/token".to_string(),
            client_id: "hiq-quantum-client".to_string(),
            client_secret: None,
            redirect_uri: "http://localhost:8080/callback".to_string(),
            scopes: vec!["openid".into(), "profile".into(), "quantum".into()],
            project_id: Some(project_id.to_string()),
            token_cache_path: Some(
                dirs::cache_dir()
                    .unwrap_or_default()
                    .join("arvak/lumi_token.json"),
            ),
            refresh_buffer_secs: 300, // 5 minutes
        }
    }

    /// Create configuration for LRZ (Germany).
    ///
    /// LRZ hosts IQM quantum systems.
    pub fn lrz(project_id: &str) -> Self {
        Self {
            provider: "lrz".to_string(),
            auth_endpoint: "https://auth.lrz.de/oauth2/authorize".to_string(),
            token_endpoint: "https://auth.lrz.de/oauth2/token".to_string(),
            client_id: "hiq-quantum-client".to_string(),
            client_secret: None,
            redirect_uri: "http://localhost:8080/callback".to_string(),
            scopes: vec!["openid".into(), "profile".into()],
            project_id: Some(project_id.to_string()),
            token_cache_path: Some(
                dirs::cache_dir()
                    .unwrap_or_default()
                    .join("arvak/lrz_token.json"),
            ),
            refresh_buffer_secs: 300,
        }
    }

    /// Create a custom OIDC configuration.
    pub fn custom(
        provider: impl Into<String>,
        auth_endpoint: impl Into<String>,
        token_endpoint: impl Into<String>,
        client_id: impl Into<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            auth_endpoint: auth_endpoint.into(),
            token_endpoint: token_endpoint.into(),
            client_id: client_id.into(),
            client_secret: None,
            redirect_uri: "http://localhost:8080/callback".to_string(),
            scopes: vec!["openid".into()],
            project_id: None,
            token_cache_path: None,
            refresh_buffer_secs: 300,
        }
    }

    /// Set the client secret.
    pub fn with_client_secret(mut self, secret: impl Into<String>) -> Self {
        self.client_secret = Some(secret.into());
        self
    }

    /// Set the redirect URI.
    pub fn with_redirect_uri(mut self, uri: impl Into<String>) -> Self {
        self.redirect_uri = uri.into();
        self
    }

    /// Set the scopes.
    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }

    /// Set the project ID.
    pub fn with_project_id(mut self, project_id: impl Into<String>) -> Self {
        self.project_id = Some(project_id.into());
        self
    }

    /// Set the token cache path.
    pub fn with_token_cache(mut self, path: impl Into<PathBuf>) -> Self {
        self.token_cache_path = Some(path.into());
        self
    }
}

/// Cached token with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedToken {
    /// Access token.
    pub access_token: String,

    /// Refresh token (if available).
    pub refresh_token: Option<String>,

    /// Token type (usually "Bearer").
    pub token_type: String,

    /// Expiration time (Unix timestamp).
    pub expires_at: u64,

    /// ID token (for OIDC).
    pub id_token: Option<String>,

    /// Scopes granted.
    pub scope: Option<String>,
}

impl CachedToken {
    /// Check if the token is expired.
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now >= self.expires_at
    }

    /// Check if the token will expire soon (within buffer).
    pub fn expires_soon(&self, buffer_secs: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now + buffer_secs >= self.expires_at
    }
}

/// OIDC token response from the provider.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
    refresh_token: Option<String>,
    id_token: Option<String>,
    scope: Option<String>,
}

/// Device authorization response.
#[derive(Debug, Deserialize)]
struct DeviceAuthResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    expires_in: u64,
    interval: u64,
}

/// OIDC authentication handler.
pub struct OidcAuth {
    config: OidcConfig,
    client: reqwest::Client,
    cached_token: Arc<RwLock<Option<CachedToken>>>,
}

impl OidcAuth {
    /// Create a new OIDC authentication handler.
    pub fn new(config: OidcConfig) -> HalResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| HalError::Auth(format!("Failed to create HTTP client: {}", e)))?;

        let auth = Self {
            config,
            client,
            cached_token: Arc::new(RwLock::new(None)),
        };

        // Try to load cached token
        if let Some(token) = auth.load_cached_token() {
            let mut cached = auth
                .cached_token
                .write()
                .expect("token cache lock poisoned");
            *cached = Some(token);
        }

        Ok(auth)
    }

    /// Get a valid access token, refreshing if necessary.
    pub async fn get_token(&self) -> HalResult<String> {
        // Check cached token
        {
            let cached = self
                .cached_token
                .read()
                .expect("token cache lock poisoned");
            if let Some(ref token) = *cached {
                if !token.expires_soon(self.config.refresh_buffer_secs) {
                    return Ok(token.access_token.clone());
                }
            }
        }

        // Try to refresh
        if let Some(refresh_token) = self.get_refresh_token() {
            match self.refresh_token(&refresh_token).await {
                Ok(token) => {
                    self.save_token(&token)?;
                    return Ok(token.access_token);
                }
                Err(e) => {
                    tracing::warn!("Token refresh failed: {}, need re-authentication", e);
                }
            }
        }

        // Need fresh authentication
        Err(HalError::Auth(
            "No valid token available. Run 'arvak auth login' to authenticate.".to_string(),
        ))
    }

    /// Check if we have a valid (non-expired) token.
    pub fn has_valid_token(&self) -> bool {
        let cached = self
            .cached_token
            .read()
            .expect("token cache lock poisoned");
        if let Some(ref token) = *cached {
            !token.is_expired()
        } else {
            false
        }
    }

    /// Perform device code flow authentication.
    ///
    /// This is the recommended flow for CLI applications as it doesn't
    /// require a browser redirect to localhost.
    pub async fn device_code_flow(&self) -> HalResult<CachedToken> {
        // Request device code
        let device_auth_endpoint = self.config.auth_endpoint.replace("/authorize", "/device");

        let params = [
            ("client_id", self.config.client_id.as_str()),
            ("scope", &self.config.scopes.join(" ")),
        ];

        let response = self
            .client
            .post(&device_auth_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|e| HalError::Auth(format!("Device authorization request failed: {}", e)))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(HalError::Auth(format!(
                "Device authorization failed: {}",
                error
            )));
        }

        let device_auth: DeviceAuthResponse = response
            .json()
            .await
            .map_err(|e| HalError::Auth(format!("Failed to parse device auth response: {}", e)))?;

        // Display instructions to user
        println!("\n╔════════════════════════════════════════════════════════════╗");
        println!("║              OIDC Authentication Required                   ║");
        println!("╠════════════════════════════════════════════════════════════╣");
        println!("║                                                            ║");
        println!("║  Please visit the following URL to authenticate:           ║");
        println!("║                                                            ║");
        if let Some(ref complete_uri) = device_auth.verification_uri_complete {
            println!("║  {}  ║", complete_uri);
        } else {
            println!("║  {}  ║", device_auth.verification_uri);
            println!("║                                                            ║");
            println!(
                "║  Enter this code: {}                             ║",
                device_auth.user_code
            );
        }
        println!("║                                                            ║");
        println!("╚════════════════════════════════════════════════════════════╝\n");
        println!(
            "Waiting for authentication (expires in {} seconds)...",
            device_auth.expires_in
        );

        // Poll for token
        let start = Instant::now();
        let timeout = Duration::from_secs(device_auth.expires_in);
        let interval = Duration::from_secs(device_auth.interval.max(5));

        loop {
            if start.elapsed() > timeout {
                return Err(HalError::Auth(
                    "Device code authentication timed out".to_string(),
                ));
            }

            tokio::time::sleep(interval).await;

            match self.poll_device_token(&device_auth.device_code).await {
                Ok(token) => {
                    println!("\n✓ Authentication successful!");
                    self.save_token(&token)?;
                    return Ok(token);
                }
                Err(PollError::Pending) => {
                    print!(".");
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                    continue;
                }
                Err(PollError::SlowDown) => {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
                Err(PollError::Error(msg)) => {
                    return Err(HalError::Auth(msg));
                }
            }
        }
    }

    /// Refresh an access token using a refresh token.
    async fn refresh_token(&self, refresh_token: &str) -> HalResult<CachedToken> {
        let mut params = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.config.client_id),
        ];

        if let Some(ref secret) = self.config.client_secret {
            params.push(("client_secret", secret));
        }

        let response = self
            .client
            .post(&self.config.token_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|e| HalError::Auth(format!("Token refresh request failed: {}", e)))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(HalError::Auth(format!("Token refresh failed: {}", error)));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| HalError::Auth(format!("Failed to parse token response: {}", e)))?;

        self.token_response_to_cached(token_response)
    }

    /// Poll for device token.
    async fn poll_device_token(&self, device_code: &str) -> Result<CachedToken, PollError> {
        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", device_code),
            ("client_id", &self.config.client_id),
        ];

        let response = self
            .client
            .post(&self.config.token_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|e| PollError::Error(format!("Token poll failed: {}", e)))?;

        if response.status().is_success() {
            let token_response: TokenResponse = response
                .json()
                .await
                .map_err(|e| PollError::Error(format!("Failed to parse token: {}", e)))?;

            return self
                .token_response_to_cached(token_response)
                .map_err(|e| PollError::Error(e.to_string()));
        }

        // Check error type
        let error_body = response.text().await.unwrap_or_default();
        if error_body.contains("authorization_pending") {
            Err(PollError::Pending)
        } else if error_body.contains("slow_down") {
            Err(PollError::SlowDown)
        } else {
            Err(PollError::Error(format!(
                "Token poll failed: {}",
                error_body
            )))
        }
    }

    /// Convert token response to cached token.
    fn token_response_to_cached(&self, response: TokenResponse) -> HalResult<CachedToken> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(CachedToken {
            access_token: response.access_token,
            refresh_token: response.refresh_token,
            token_type: response.token_type,
            expires_at: now + response.expires_in,
            id_token: response.id_token,
            scope: response.scope,
        })
    }

    /// Get the refresh token from cache.
    fn get_refresh_token(&self) -> Option<String> {
        let cached = self
            .cached_token
            .read()
            .expect("token cache lock poisoned");
        cached.as_ref().and_then(|t| t.refresh_token.clone())
    }

    /// Save token to cache (memory and file).
    fn save_token(&self, token: &CachedToken) -> HalResult<()> {
        // Save to memory
        {
            let mut cached = self
                .cached_token
                .write()
                .expect("token cache lock poisoned");
            *cached = Some(token.clone());
        }

        // Save to file if configured
        if let Some(ref path) = self.config.token_cache_path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    HalError::Auth(format!("Failed to create cache directory: {}", e))
                })?;
            }

            let json = serde_json::to_string_pretty(token)
                .map_err(|e| HalError::Auth(format!("Failed to serialize token: {}", e)))?;

            std::fs::write(path, json)
                .map_err(|e| HalError::Auth(format!("Failed to write token cache: {}", e)))?;

            // Set restrictive permissions on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(path)
                    .map_err(|e| HalError::Auth(format!("Failed to get file metadata: {}", e)))?
                    .permissions();
                perms.set_mode(0o600);
                std::fs::set_permissions(path, perms)
                    .map_err(|e| HalError::Auth(format!("Failed to set permissions: {}", e)))?;
            }
        }

        Ok(())
    }

    /// Load cached token from file.
    fn load_cached_token(&self) -> Option<CachedToken> {
        let path = self.config.token_cache_path.as_ref()?;
        let content = std::fs::read_to_string(path).ok()?;
        let token: CachedToken = serde_json::from_str(&content).ok()?;

        // Don't return expired tokens
        if token.is_expired() {
            // But keep it if we have a refresh token
            if token.refresh_token.is_some() {
                return Some(token);
            }
            return None;
        }

        Some(token)
    }

    /// Clear cached tokens.
    pub fn logout(&self) -> HalResult<()> {
        // Clear memory cache
        {
            let mut cached = self
                .cached_token
                .write()
                .expect("token cache lock poisoned");
            *cached = None;
        }

        // Delete file cache
        if let Some(ref path) = self.config.token_cache_path {
            if path.exists() {
                std::fs::remove_file(path)
                    .map_err(|e| HalError::Auth(format!("Failed to remove token cache: {}", e)))?;
            }
        }

        Ok(())
    }
}

/// Poll error types for device code flow.
enum PollError {
    Pending,
    SlowDown,
    Error(String),
}

/// Token provider trait for dependency injection.
#[async_trait::async_trait]
pub trait TokenProvider: Send + Sync {
    /// Get a valid access token.
    async fn get_token(&self) -> HalResult<String>;

    /// Check if authentication is available.
    fn has_valid_token(&self) -> bool;
}

#[async_trait::async_trait]
impl TokenProvider for OidcAuth {
    async fn get_token(&self) -> HalResult<String> {
        OidcAuth::get_token(self).await
    }

    fn has_valid_token(&self) -> bool {
        OidcAuth::has_valid_token(self)
    }
}

/// Environment variable token provider.
///
/// Simple provider that reads token from an environment variable.
pub struct EnvTokenProvider {
    env_var: String,
}

impl EnvTokenProvider {
    /// Create a new environment variable token provider.
    pub fn new(env_var: impl Into<String>) -> Self {
        Self {
            env_var: env_var.into(),
        }
    }

    /// Create provider for IQM_TOKEN.
    pub fn iqm() -> Self {
        Self::new("IQM_TOKEN")
    }

    /// Create provider for IBM_QUANTUM_TOKEN.
    pub fn ibm() -> Self {
        Self::new("IBM_QUANTUM_TOKEN")
    }
}

#[async_trait::async_trait]
impl TokenProvider for EnvTokenProvider {
    async fn get_token(&self) -> HalResult<String> {
        std::env::var(&self.env_var)
            .map_err(|_| HalError::Auth(format!("Environment variable {} not set", self.env_var)))
    }

    fn has_valid_token(&self) -> bool {
        std::env::var(&self.env_var).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lumi_config() {
        let config = OidcConfig::lumi("project_123");
        assert_eq!(config.provider, "csc");
        assert!(config.auth_endpoint.contains("auth.csc.fi"));
        assert_eq!(config.project_id, Some("project_123".to_string()));
    }

    #[test]
    fn test_lrz_config() {
        let config = OidcConfig::lrz("project_456");
        assert_eq!(config.provider, "lrz");
        assert!(config.auth_endpoint.contains("auth.lrz.de"));
        assert_eq!(config.project_id, Some("project_456".to_string()));
    }

    #[test]
    fn test_custom_config() {
        let config = OidcConfig::custom(
            "custom-provider",
            "https://auth.example.com/authorize",
            "https://auth.example.com/token",
            "my-client-id",
        )
        .with_client_secret("secret")
        .with_project_id("project");

        assert_eq!(config.provider, "custom-provider");
        assert_eq!(config.client_secret, Some("secret".to_string()));
        assert_eq!(config.project_id, Some("project".to_string()));
    }

    #[test]
    fn test_cached_token_expiry() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let expired_token = CachedToken {
            access_token: "token".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_at: now - 100, // Expired 100 seconds ago
            id_token: None,
            scope: None,
        };
        assert!(expired_token.is_expired());

        let valid_token = CachedToken {
            access_token: "token".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_at: now + 3600, // Expires in 1 hour
            id_token: None,
            scope: None,
        };
        assert!(!valid_token.is_expired());
        assert!(!valid_token.expires_soon(300)); // Not expiring in 5 minutes

        let expiring_soon = CachedToken {
            access_token: "token".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_at: now + 60, // Expires in 1 minute
            id_token: None,
            scope: None,
        };
        assert!(!expiring_soon.is_expired());
        assert!(expiring_soon.expires_soon(300)); // Will expire within 5 minutes
    }

    #[test]
    fn test_env_token_provider() {
        // SAFETY: This test runs in isolation and only modifies a test-specific variable
        unsafe {
            std::env::set_var("HIQ_TEST_TOKEN_VAR_12345", "test-token-value");
        }
        let provider = EnvTokenProvider::new("HIQ_TEST_TOKEN_VAR_12345");
        assert!(provider.has_valid_token());

        let missing_provider = EnvTokenProvider::new("NONEXISTENT_VAR_12345");
        assert!(!missing_provider.has_valid_token());

        // SAFETY: Cleaning up test variable
        unsafe {
            std::env::remove_var("HIQ_TEST_TOKEN_VAR_12345");
        }
    }
}
