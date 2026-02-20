//! Configuration management for Arvak gRPC server.
//!
//! Supports loading configuration from:
//! 1. Configuration files (YAML)
//! 2. Environment variables (with ARVAK_ prefix)
//! 3. .env files
//!
//! Configuration precedence (highest to lowest):
//! 1. Environment variables
//! 2. Configuration file
//! 3. Default values

use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::SocketAddr;
use std::path::Path;

/// Complete server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// gRPC server configuration
    pub server: ServerConfig,

    /// Storage backend configuration
    pub storage: StorageConfig,

    /// Metrics and health check configuration
    pub observability: ObservabilityConfig,

    /// Backend configurations
    #[serde(default)]
    pub backends: BackendConfigs,

    /// Resource limits and quotas
    #[serde(default)]
    pub limits: ResourceLimits,
}

/// gRPC server settings.
#[derive(Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server bind address (e.g., "0.0.0.0:50051")
    #[serde(default = "default_grpc_address")]
    pub address: String,

    /// Connection timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    /// Keep-alive interval in seconds
    #[serde(default = "default_keepalive")]
    pub keepalive_seconds: u64,

    /// Maximum concurrent connections
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,

    /// Graceful shutdown timeout in seconds
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout_seconds: u64,

    /// Maximum gRPC message size in bytes (default: 16 MB)
    #[serde(default = "default_max_message_size")]
    pub max_message_size_bytes: usize,

    /// Optional API key for authentication (set via ARVAK_API_KEY)
    #[serde(default)]
    pub api_key: Option<String>,
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerConfig")
            .field("address", &self.address)
            .field("timeout_seconds", &self.timeout_seconds)
            .field("keepalive_seconds", &self.keepalive_seconds)
            .field("max_connections", &self.max_connections)
            .field("shutdown_timeout_seconds", &self.shutdown_timeout_seconds)
            .field("max_message_size_bytes", &self.max_message_size_bytes)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

/// Storage backend configuration.
#[derive(Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Storage backend type: "memory", "sqlite", "postgres"
    #[serde(default = "default_storage_type")]
    pub backend: String,

    /// Connection string for database backends
    pub connection_string: Option<String>,

    /// Maximum number of database connections
    #[serde(default = "default_db_pool_size")]
    pub pool_size: u32,
}

impl fmt::Debug for StorageConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StorageConfig")
            .field("backend", &self.backend)
            .field("connection_string", &"[REDACTED]")
            .field("pool_size", &self.pool_size)
            .finish()
    }
}

/// Observability configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Metrics and health server configuration
    pub http_server: HttpServerConfig,

    /// Logging configuration
    pub logging: LoggingConfig,

    /// OpenTelemetry configuration
    pub tracing: TracingConfig,
}

/// HTTP server for metrics and health endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpServerConfig {
    /// HTTP server bind address (e.g., "0.0.0.0:8080")
    #[serde(default = "default_http_address")]
    pub address: String,

    /// Enable metrics endpoint (/metrics)
    #[serde(default = "default_true")]
    pub metrics_enabled: bool,

    /// Enable health endpoints (/health, /health/ready)
    #[serde(default = "default_true")]
    pub health_enabled: bool,
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level: "trace", "debug", "info", "warn", "error"
    #[serde(default = "default_log_level")]
    pub level: String,

    /// Log format: "console" or "json"
    #[serde(default = "default_log_format")]
    pub format: String,
}

/// Distributed tracing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// Enable OpenTelemetry tracing
    #[serde(default)]
    pub enabled: bool,

    /// OTLP endpoint (e.g., "<http://localhost:4317>")
    pub otlp_endpoint: Option<String>,

    /// Service name for traces
    #[serde(default = "default_service_name")]
    pub service_name: String,
}

/// Backend configurations.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackendConfigs {
    /// Simulator backend enabled
    #[serde(default = "default_true")]
    pub simulator_enabled: bool,

    /// Custom backend configurations
    #[serde(default)]
    pub custom: std::collections::HashMap<String, BackendConfig>,
}

/// Individual backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    /// Backend type/adapter
    pub backend_type: String,

    /// Backend-specific settings as JSON
    #[serde(default)]
    pub settings: serde_json::Value,
}

/// Resource limits and quotas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum concurrent jobs across all backends
    #[serde(default = "default_max_concurrent_jobs")]
    pub max_concurrent_jobs: usize,

    /// Maximum queued jobs
    #[serde(default = "default_max_queued_jobs")]
    pub max_queued_jobs: usize,

    /// Job execution timeout in seconds
    #[serde(default = "default_job_timeout")]
    pub job_timeout_seconds: u64,

    /// Maximum result size in bytes
    #[serde(default = "default_max_result_size")]
    pub max_result_size_bytes: usize,

    /// Rate limit: requests per second per client
    #[serde(default = "default_rate_limit")]
    pub rate_limit_rps: u32,
}

// Default value functions
fn default_grpc_address() -> String {
    "0.0.0.0:50051".to_string()
}

fn default_http_address() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_timeout() -> u64 {
    60
}

fn default_keepalive() -> u64 {
    30
}

fn default_max_connections() -> usize {
    1000
}

fn default_storage_type() -> String {
    "memory".to_string()
}

fn default_db_pool_size() -> u32 {
    10
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "console".to_string()
}

fn default_service_name() -> String {
    "arvak-grpc".to_string()
}

fn default_true() -> bool {
    true
}

fn default_max_concurrent_jobs() -> usize {
    100
}

fn default_max_queued_jobs() -> usize {
    1000
}

fn default_job_timeout() -> u64 {
    3600 // 1 hour
}

fn default_max_result_size() -> usize {
    100 * 1024 * 1024 // 100 MB
}

fn default_rate_limit() -> u32 {
    100
}

fn default_shutdown_timeout() -> u64 {
    30 // 30 seconds
}

fn default_max_message_size() -> usize {
    16 * 1024 * 1024 // 16 MB
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server: ServerConfig {
                address: default_grpc_address(),
                timeout_seconds: default_timeout(),
                keepalive_seconds: default_keepalive(),
                max_connections: default_max_connections(),
                shutdown_timeout_seconds: default_shutdown_timeout(),
                max_message_size_bytes: default_max_message_size(),
                api_key: None,
            },
            storage: StorageConfig {
                backend: default_storage_type(),
                connection_string: None,
                pool_size: default_db_pool_size(),
            },
            observability: ObservabilityConfig {
                http_server: HttpServerConfig {
                    address: default_http_address(),
                    metrics_enabled: true,
                    health_enabled: true,
                },
                logging: LoggingConfig {
                    level: default_log_level(),
                    format: default_log_format(),
                },
                tracing: TracingConfig {
                    enabled: false,
                    otlp_endpoint: None,
                    service_name: default_service_name(),
                },
            },
            backends: BackendConfigs::default(),
            limits: ResourceLimits::default(),
        }
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        ResourceLimits {
            max_concurrent_jobs: default_max_concurrent_jobs(),
            max_queued_jobs: default_max_queued_jobs(),
            job_timeout_seconds: default_job_timeout(),
            max_result_size_bytes: default_max_result_size(),
            rate_limit_rps: default_rate_limit(),
        }
    }
}

impl Config {
    /// Load configuration from a YAML file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path.as_ref())
            .map_err(|e| ConfigError::IoError(e.to_string()))?;

        let config: Config =
            serde_yml::from_str(&contents).map_err(|e| ConfigError::ParseError(e.to_string()))?;

        config.validate()?;
        Ok(config)
    }

    /// Load configuration from environment variables.
    ///
    /// Environment variables override configuration file values.
    pub fn from_env() -> Self {
        let mut config = Config::default();

        // Server configuration
        if let Ok(addr) = std::env::var("ARVAK_GRPC_ADDRESS") {
            config.server.address = addr;
        }
        if let Ok(timeout) = std::env::var("ARVAK_GRPC_TIMEOUT") {
            if let Ok(val) = timeout.parse() {
                config.server.timeout_seconds = val;
            }
        }
        if let Ok(keepalive) = std::env::var("ARVAK_GRPC_KEEPALIVE") {
            if let Ok(val) = keepalive.parse() {
                config.server.keepalive_seconds = val;
            }
        }

        // Message size
        if let Ok(size) = std::env::var("ARVAK_MAX_MESSAGE_SIZE") {
            if let Ok(val) = size.parse() {
                config.server.max_message_size_bytes = val;
            }
        }

        // API key
        if let Ok(key) = std::env::var("ARVAK_API_KEY") {
            config.server.api_key = Some(key);
        }

        // Storage configuration
        if let Ok(backend) = std::env::var("ARVAK_STORAGE_TYPE") {
            config.storage.backend = backend;
        }
        if let Ok(conn) = std::env::var("ARVAK_STORAGE_CONNECTION") {
            config.storage.connection_string = Some(conn);
        }

        // HTTP server configuration
        if let Ok(addr) = std::env::var("ARVAK_HTTP_ADDRESS") {
            config.observability.http_server.address = addr;
        }

        // Logging configuration
        if let Ok(level) = std::env::var("ARVAK_LOG_LEVEL") {
            config.observability.logging.level = level;
        }
        if let Ok(format) = std::env::var("ARVAK_LOG_FORMAT") {
            config.observability.logging.format = format;
        }

        // Tracing configuration
        if let Ok(endpoint) = std::env::var("ARVAK_OTLP_ENDPOINT") {
            config.observability.tracing.enabled = true;
            config.observability.tracing.otlp_endpoint = Some(endpoint);
        }

        // Resource limits
        if let Ok(max) = std::env::var("ARVAK_MAX_CONCURRENT_JOBS") {
            if let Ok(val) = max.parse() {
                config.limits.max_concurrent_jobs = val;
            }
        }
        if let Ok(max) = std::env::var("ARVAK_MAX_QUEUED_JOBS") {
            if let Ok(val) = max.parse() {
                config.limits.max_queued_jobs = val;
            }
        }
        if let Ok(timeout) = std::env::var("ARVAK_JOB_TIMEOUT") {
            if let Ok(val) = timeout.parse() {
                config.limits.job_timeout_seconds = val;
            }
        }

        config
    }

    /// Load configuration with the following precedence:
    /// 1. Load from file if provided
    /// 2. Apply environment variable overrides
    /// 3. Load .env file if it exists
    pub fn load(config_file: Option<&str>) -> Result<Self, ConfigError> {
        // Load .env file if it exists
        dotenvy::dotenv().ok();

        // Start with file or default
        let mut config = if let Some(path) = config_file {
            Self::from_file(path)?
        } else {
            Config::default()
        };

        // Apply environment overrides
        config = config.merge_env();

        config.validate()?;
        Ok(config)
    }

    /// Merge environment variables into this configuration.
    ///
    /// Only variables that are explicitly set in the environment override the
    /// file-loaded (or default) values. Variables that are absent leave the
    /// corresponding fields unchanged.
    fn merge_env(mut self) -> Self {
        // Server
        if let Ok(v) = std::env::var("ARVAK_GRPC_ADDRESS") {
            self.server.address = v;
        }
        if let Ok(v) = std::env::var("ARVAK_GRPC_TIMEOUT") {
            if let Ok(val) = v.parse() {
                self.server.timeout_seconds = val;
            }
        }
        if let Ok(v) = std::env::var("ARVAK_GRPC_KEEPALIVE") {
            if let Ok(val) = v.parse() {
                self.server.keepalive_seconds = val;
            }
        }
        if let Ok(v) = std::env::var("ARVAK_MAX_MESSAGE_SIZE") {
            if let Ok(val) = v.parse() {
                self.server.max_message_size_bytes = val;
            }
        }
        if let Ok(key) = std::env::var("ARVAK_API_KEY") {
            self.server.api_key = Some(key);
        }

        // Storage
        if let Ok(v) = std::env::var("ARVAK_STORAGE_TYPE") {
            self.storage.backend = v;
        }
        if let Ok(v) = std::env::var("ARVAK_STORAGE_CONNECTION") {
            self.storage.connection_string = Some(v);
        }

        // Observability — HTTP server
        if let Ok(v) = std::env::var("ARVAK_HTTP_ADDRESS") {
            self.observability.http_server.address = v;
        }

        // Observability — logging
        if let Ok(v) = std::env::var("ARVAK_LOG_LEVEL") {
            self.observability.logging.level = v;
        }
        if let Ok(v) = std::env::var("ARVAK_LOG_FORMAT") {
            self.observability.logging.format = v;
        }

        // Observability — tracing
        if let Ok(v) = std::env::var("ARVAK_OTLP_ENDPOINT") {
            self.observability.tracing.enabled = true;
            self.observability.tracing.otlp_endpoint = Some(v);
        }

        // Resource limits
        if let Ok(v) = std::env::var("ARVAK_MAX_CONCURRENT_JOBS") {
            if let Ok(val) = v.parse() {
                self.limits.max_concurrent_jobs = val;
            }
        }
        if let Ok(v) = std::env::var("ARVAK_MAX_QUEUED_JOBS") {
            if let Ok(val) = v.parse() {
                self.limits.max_queued_jobs = val;
            }
        }
        if let Ok(v) = std::env::var("ARVAK_JOB_TIMEOUT") {
            if let Ok(val) = v.parse() {
                self.limits.job_timeout_seconds = val;
            }
        }

        self
    }

    /// Validate configuration values.
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate server address
        self.server.address.parse::<SocketAddr>().map_err(|_| {
            ConfigError::ValidationError(format!("Invalid server address: {}", self.server.address))
        })?;

        // Validate HTTP address
        self.observability
            .http_server
            .address
            .parse::<SocketAddr>()
            .map_err(|_| {
                ConfigError::ValidationError(format!(
                    "Invalid HTTP address: {}",
                    self.observability.http_server.address
                ))
            })?;

        // Validate storage backend
        match self.storage.backend.as_str() {
            "memory" | "sqlite" | "postgres" => {}
            other => {
                return Err(ConfigError::ValidationError(format!(
                    "Unknown storage backend: {other}"
                )));
            }
        }

        // Validate log level
        match self.observability.logging.level.as_str() {
            "trace" | "debug" | "info" | "warn" | "error" => {}
            other => {
                return Err(ConfigError::ValidationError(format!(
                    "Invalid log level: {other}"
                )));
            }
        }

        // Validate log format
        match self.observability.logging.format.as_str() {
            "console" | "json" => {}
            other => {
                return Err(ConfigError::ValidationError(format!(
                    "Invalid log format: {other}"
                )));
            }
        }

        // Validate resource limits
        if self.limits.max_concurrent_jobs == 0 {
            return Err(ConfigError::ValidationError(
                "max_concurrent_jobs must be greater than 0".to_string(),
            ));
        }

        // Validate API key is non-empty if provided
        if let Some(ref key) = self.server.api_key {
            if key.is_empty() {
                return Err(ConfigError::ValidationError(
                    "api_key must not be empty; omit the field to disable authentication".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Get the parsed gRPC server address.
    pub fn grpc_address(&self) -> Result<SocketAddr, ConfigError> {
        self.server.address.parse().map_err(|_| {
            ConfigError::ValidationError(format!("Invalid server address: {}", self.server.address))
        })
    }

    /// Get the parsed HTTP server address.
    pub fn http_address(&self) -> Result<SocketAddr, ConfigError> {
        self.observability.http_server.address.parse().map_err(|_| {
            ConfigError::ValidationError(format!(
                "Invalid HTTP address: {}",
                self.observability.http_server.address
            ))
        })
    }
}

/// Configuration errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    IoError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.address, "0.0.0.0:50051");
        assert_eq!(config.storage.backend, "memory");
        assert_eq!(config.observability.logging.level, "info");
    }

    #[test]
    fn test_validate_valid_config() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_storage() {
        let mut config = Config::default();
        config.storage.backend = "invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_log_level() {
        let mut config = Config::default();
        config.observability.logging.level = "invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_grpc_address_parsing() {
        let config = Config::default();
        let addr = config.grpc_address().unwrap();
        assert_eq!(addr.port(), 50051);
    }
}
