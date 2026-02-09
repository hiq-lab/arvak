//! Configuration loading example.
//!
//! This example demonstrates how to load configuration from files
//! and environment variables.
//!
//! Run with: cargo run --example `config_example`

use arvak_grpc::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Arvak gRPC Configuration Example ===\n");

    // Example 1: Default configuration
    println!("1. Default Configuration:");
    let default_config = Config::default();
    println!("   gRPC Address: {}", default_config.server.address);
    println!(
        "   HTTP Address: {}",
        default_config.observability.http_server.address
    );
    println!("   Storage Backend: {}", default_config.storage.backend);
    println!(
        "   Log Level: {}",
        default_config.observability.logging.level
    );
    println!(
        "   Max Concurrent Jobs: {}\n",
        default_config.limits.max_concurrent_jobs
    );

    // Example 2: Load from configuration file
    println!("2. Load from Configuration File:");
    println!("   Copy config.example.yaml to config.yaml and run again");
    match Config::from_file("config.yaml") {
        Ok(config) => {
            println!("   ✓ Loaded from config.yaml");
            println!("   gRPC Address: {}", config.server.address);
            println!("   Storage: {}", config.storage.backend);
        }
        Err(e) => {
            println!("   ✗ Could not load config.yaml: {e}");
            println!("   (This is expected if config.yaml doesn't exist)");
        }
    }
    println!();

    // Example 3: Environment variables
    println!("3. Environment Variable Overrides:");
    println!("   Set ARVAK_GRPC_ADDRESS, ARVAK_LOG_LEVEL, etc.");

    // Simulate setting some env vars
    unsafe {
        std::env::set_var("ARVAK_GRPC_ADDRESS", "127.0.0.1:9090");
        std::env::set_var("ARVAK_LOG_LEVEL", "debug");
        std::env::set_var("ARVAK_MAX_CONCURRENT_JOBS", "200");
    }

    let env_config = Config::from_env();
    println!("   gRPC Address: {}", env_config.server.address);
    println!("   Log Level: {}", env_config.observability.logging.level);
    println!(
        "   Max Concurrent Jobs: {}\n",
        env_config.limits.max_concurrent_jobs
    );

    // Example 4: Combined loading (file + env overrides)
    println!("4. Combined Configuration:");
    println!("   Loads from file (if exists), then applies env overrides");

    // Clean up env vars for this example
    unsafe {
        std::env::remove_var("ARVAK_GRPC_ADDRESS");
        std::env::remove_var("ARVAK_LOG_LEVEL");
        std::env::remove_var("ARVAK_MAX_CONCURRENT_JOBS");
    }

    match Config::load(Some("config.yaml")) {
        Ok(config) => {
            println!("   ✓ Combined config loaded");
            println!("   gRPC Address: {}", config.server.address);
            println!("   Validation: OK");
        }
        Err(e) => {
            println!("   ✗ Error: {e}");
            println!("   Using default configuration instead");
            let config = Config::load(None)?;
            println!("   gRPC Address: {}", config.server.address);
        }
    }
    println!();

    // Example 5: Validation
    println!("5. Configuration Validation:");
    let mut invalid_config = Config::default();
    invalid_config.storage.backend = "invalid".to_string();

    match invalid_config.validate() {
        Ok(()) => println!("   ✓ Configuration is valid"),
        Err(e) => println!("   ✗ Validation error: {e}"),
    }
    println!();

    // Example 6: Address parsing
    println!("6. Address Parsing:");
    let config = Config::default();
    let grpc_addr = config.grpc_address()?;
    let http_addr = config.http_address()?;
    println!("   gRPC SocketAddr: {grpc_addr}");
    println!("   HTTP SocketAddr: {http_addr}");
    println!();

    println!("=== Configuration Examples Complete ===");
    Ok(())
}
