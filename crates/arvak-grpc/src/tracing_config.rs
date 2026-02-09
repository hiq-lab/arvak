//! Distributed tracing configuration with OpenTelemetry.
//!
//! This module provides tracing setup with support for:
//! - Console output (development)
//! - JSON structured logging (production)
//! - OpenTelemetry export via OTLP (distributed tracing)

use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

/// Tracing output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracingFormat {
    /// Human-readable console output (for development).
    Console,
    /// JSON structured logging (for production).
    Json,
}

/// Tracing configuration.
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// Log level filter (e.g., "info", "debug", "trace").
    pub log_level: String,
    /// Output format (console or JSON).
    pub format: TracingFormat,
    /// Service name for tracing.
    pub service_name: String,
    /// OpenTelemetry OTLP endpoint (e.g., "<http://localhost:4317>").
    /// If None, OpenTelemetry export is disabled.
    pub otlp_endpoint: Option<String>,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
            format: TracingFormat::Console,
            service_name: "arvak-grpc".to_string(),
            otlp_endpoint: None,
        }
    }
}

impl TracingConfig {
    /// Create a new tracing config with custom settings.
    pub fn new(
        log_level: String,
        format: TracingFormat,
        service_name: String,
        otlp_endpoint: Option<String>,
    ) -> Self {
        Self {
            log_level,
            format,
            service_name,
            otlp_endpoint,
        }
    }

    /// Create config from environment variables.
    ///
    /// Environment variables:
    /// - `RUST_LOG`: Log level (default: "info")
    /// - `ARVAK_LOG_FORMAT`: "console" or "json" (default: "console")
    /// - `ARVAK_SERVICE_NAME`: Service name (default: "arvak-grpc")
    /// - `OTEL_EXPORTER_OTLP_ENDPOINT`: OTLP endpoint (optional)
    pub fn from_env() -> Self {
        let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

        let format = std::env::var("ARVAK_LOG_FORMAT").map_or(TracingFormat::Console, |f| match f
            .as_str()
        {
            "json" => TracingFormat::Json,
            _ => TracingFormat::Console,
        });

        let service_name =
            std::env::var("ARVAK_SERVICE_NAME").unwrap_or_else(|_| "arvak-grpc".to_string());

        let otlp_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();

        Self {
            log_level,
            format,
            service_name,
            otlp_endpoint,
        }
    }
}

/// Initialize tracing with the given configuration.
///
/// This sets up the global tracing subscriber with:
/// - Environment-based log level filtering
/// - Console or JSON output
/// - Optional OpenTelemetry export
pub fn init_tracing(config: TracingConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Create environment filter
    let env_filter = EnvFilter::try_new(&config.log_level)
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // Create fmt layer based on format
    let fmt_layer = match config.format {
        TracingFormat::Console => fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_file(true)
            .with_line_number(true)
            .pretty()
            .boxed(),
        TracingFormat::Json => fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .json()
            .boxed(),
    };

    // Setup OpenTelemetry if endpoint is configured
    if let Some(endpoint) = config.otlp_endpoint {
        // Create OTLP exporter
        let otlp_exporter = opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(endpoint);

        // Create tracer
        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(otlp_exporter)
            .with_trace_config(
                opentelemetry_sdk::trace::config()
                    .with_sampler(Sampler::AlwaysOn)
                    .with_id_generator(RandomIdGenerator::default())
                    .with_resource(Resource::new(vec![opentelemetry::KeyValue::new(
                        "service.name",
                        config.service_name,
                    )])),
            )
            .install_batch(opentelemetry_sdk::runtime::Tokio)?;

        // Create OpenTelemetry tracing layer
        let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        // Initialize subscriber with all layers
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(telemetry_layer)
            .init();

        tracing::info!("Tracing initialized with OpenTelemetry export");
    } else {
        // Initialize subscriber without OpenTelemetry
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();

        tracing::info!("Tracing initialized (OpenTelemetry disabled)");
    }

    Ok(())
}

/// Initialize tracing with default configuration from environment.
pub fn init_default_tracing() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing(TracingConfig::from_env())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TracingConfig::default();
        assert_eq!(config.log_level, "info");
        assert_eq!(config.format, TracingFormat::Console);
        assert_eq!(config.service_name, "arvak-grpc");
        assert!(config.otlp_endpoint.is_none());
    }

    #[test]
    fn test_custom_config() {
        let config = TracingConfig::new(
            "debug".to_string(),
            TracingFormat::Json,
            "test-service".to_string(),
            Some("http://localhost:4317".to_string()),
        );

        assert_eq!(config.log_level, "debug");
        assert_eq!(config.format, TracingFormat::Json);
        assert_eq!(config.service_name, "test-service");
        assert_eq!(
            config.otlp_endpoint,
            Some("http://localhost:4317".to_string())
        );
    }

    #[test]
    fn test_format_types() {
        assert_eq!(TracingFormat::Console, TracingFormat::Console);
        assert_eq!(TracingFormat::Json, TracingFormat::Json);
        assert_ne!(TracingFormat::Console, TracingFormat::Json);
    }
}
