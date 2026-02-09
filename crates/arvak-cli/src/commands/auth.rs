//! Auth command implementation.
//!
//! Manage authentication for HPC quantum computing providers.

use anyhow::Result;
use console::style;

use arvak_hal::auth::{OidcAuth, OidcConfig};

/// Execute the auth login subcommand.
pub async fn execute_login(provider: &str, project: Option<&str>) -> Result<()> {
    let project_id = project.ok_or_else(|| {
        anyhow::anyhow!("--project is required for login. Example: --project project_462000xxx")
    })?;

    let config = match provider.to_lowercase().as_str() {
        "csc" | "lumi" => {
            println!(
                "{} Authenticating with {} (project {})",
                style("→").cyan().bold(),
                style("CSC / LUMI").yellow(),
                style(project_id).green()
            );
            OidcConfig::lumi(project_id)
        }
        "lrz" => {
            println!(
                "{} Authenticating with {} (project {})",
                style("→").cyan().bold(),
                style("LRZ").yellow(),
                style(project_id).green()
            );
            OidcConfig::lrz(project_id)
        }
        other => {
            anyhow::bail!("Unknown provider: '{other}'. Available: csc (LUMI), lrz");
        }
    };

    let auth =
        OidcAuth::new(config).map_err(|e| anyhow::anyhow!("Failed to initialize auth: {e}"))?;

    println!("  Starting device code flow...");
    println!("  A browser window will open for authentication.\n");

    let token = auth
        .device_code_flow()
        .await
        .map_err(|e| anyhow::anyhow!("Authentication failed: {e}"))?;

    println!("\n{} Authentication successful!", style("✓").green().bold());
    println!(
        "  Token expires: {}",
        style(
            chrono::DateTime::from_timestamp(token.expires_at as i64, 0).map_or_else(|| "unknown".to_string(), |dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        )
        .yellow()
    );

    Ok(())
}

/// Execute the auth status subcommand.
pub async fn execute_status(provider: Option<&str>) -> Result<()> {
    // Check each known provider for cached tokens
    let providers: Vec<(&str, fn(&str) -> OidcConfig)> = vec![
        ("CSC / LUMI", OidcConfig::lumi as fn(&str) -> OidcConfig),
        ("LRZ", OidcConfig::lrz as fn(&str) -> OidcConfig),
    ];

    println!("{} Authentication status:\n", style("→").cyan().bold());

    let mut found_any = false;

    for (name, config_fn) in &providers {
        if let Some(p) = provider {
            let matches = match p.to_lowercase().as_str() {
                "csc" | "lumi" => *name == "CSC / LUMI",
                "lrz" => *name == "LRZ",
                _ => false,
            };
            if !matches {
                continue;
            }
        }

        // Try to create auth with a placeholder project to check token cache
        let config = config_fn("_check");
        if let Ok(auth) = OidcAuth::new(config) {
            let valid = auth.has_valid_token();
            let status = if valid {
                style("authenticated").green()
            } else {
                style("not authenticated").red()
            };
            println!("  {}: {}", style(name).bold(), status);
            found_any = true;
        }
    }

    if !found_any {
        println!("  No authentication tokens found.");
        println!(
            "  Run {} to authenticate.",
            style("arvak auth login --provider <provider> --project <id>").dim()
        );
    }

    Ok(())
}

/// Execute the auth logout subcommand.
pub async fn execute_logout(provider: Option<&str>) -> Result<()> {
    let providers: Vec<(&str, fn(&str) -> OidcConfig)> = vec![
        ("CSC / LUMI", OidcConfig::lumi as fn(&str) -> OidcConfig),
        ("LRZ", OidcConfig::lrz as fn(&str) -> OidcConfig),
    ];

    println!("{} Logging out...", style("→").cyan().bold());

    for (name, config_fn) in &providers {
        if let Some(p) = provider {
            let matches = match p.to_lowercase().as_str() {
                "csc" | "lumi" => *name == "CSC / LUMI",
                "lrz" => *name == "LRZ",
                _ => false,
            };
            if !matches {
                continue;
            }
        }

        let config = config_fn("_logout");
        if let Ok(auth) = OidcAuth::new(config) {
            match auth.logout() {
                Ok(()) => {
                    println!("  {}: {}", style(name).bold(), style("logged out").green());
                }
                Err(e) => {
                    println!(
                        "  {}: {} ({})",
                        style(name).bold(),
                        style("failed").red(),
                        e
                    );
                }
            }
        }
    }

    println!("{} Done.", style("✓").green().bold());

    Ok(())
}
