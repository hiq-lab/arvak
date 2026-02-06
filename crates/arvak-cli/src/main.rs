//! Arvak Command-Line Interface
//!
//! The main entry point for the Arvak CLI tool.
//!
//! ```text
//! ↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙→↗↗↑↑↑↗↑↑↑↑↗↗↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙
//! ↙↗↗↗↗↗↗↗↗↗↗↗↗↗↗→↗↘↓↗↑↑↑↑↖↖          ←↑↑↑↑↑↗↘↙↘→→↗↗↗↗↗↗↗↗↗↗↗↗↗↙
//! ↙↗↗↗↗↗↗↗↗↗↗↗↗→→↗↑↑↑↑                      ↑↑↑↑↗↓↓→→→→↗↗↗↗↗↗↗↗↙
//! ↙↗↗↗↗↗↗→→→↗→→↑↑↑↑                 ↑↑↑↑↑       ↑↑↗↓↘↗→→→↗↗↗↗↗↗↙
//! ↙↗↗↗↗↗→↗↗→↘↗↑↑                   ↑↑↗↑↑↑↑↑↑↑←    ↑↑↑↗→↗↗→→↗↗↗↗↙
//! ↙↗↗↗↗↗↗↗→↑↑↑                 ↖↙↓ ↑↑↑↑↑→→↑↑→↑↑↑     ↑↑↗↓→→↗↗↗↗↙
//! ↙↗↗↗↗→→↘↗↑          ↙↘↑↑↑↗→→→→→→→→→→→→→→→↗↗→↑→↑↑     ↑↑↘→→↗↗↗↙
//! ↙↗→→→→↓↗↑    ←↑→→→→→→→→→→→→→→→↑↑→→→→→→→→↑↑↑↗→↑→→↑     ↑↑→→↗↗↗↙
//! ↙↗↗↗↗→↑↑    ↗→↑↑→→→→→→→→→→→→→→→→→→→→→→→→→→↑↑→→↑→↑↑     ↑↑↓→↗↗↙
//! ↙↗↗→↓↑↑    ↙→→↑→→→→→→→→→→→→→→→→→→→→→→→→→→→→→↑→↑→→↑↑     ↑↑↓↗↗↙
//! ↙↗→↓↑↑    ↙↑→→→→→→→→→→→→→→→→→→→→→→→→→→→→→→→→→↑↑↗→→→↑     ↑→→↗↙
//! ↙↗→↘↑             ↑→→→→→→→→→→→→→→→→→→→→→→→→→→→↑↑→→↑↑     ↑↑↓↗↙
//! ↙→↓↑↑       ↑↑↑↑↑↑↑→→→→→→→→→→→→→→→→→→→→→→→→→→→↑↑→→↑↑↑     ↑↙→↙
//! ↙→→↑         ↑↑→→→→↑↑→↗↑↑↑→→→→→→→→→↑→→→→→→→→→→↑↑→→→↑     ↑↗↘↙
//! ↙→↑↑             ↖        ↑↑↗→→→↑↑↑↗→→→→→→→→→→→↑→→↑→↑     ↗↑↓↙
//! ↙↙↑                          ↑↑↑→→→→→→→→→→→→→→→↑↑→↗↑↑↑     ↑↙↙
//! ↙↙↑                          ↑↑→→→→→→→→→→→→→→→→↑↑→→↑→↑     ↑↙↙
//! ↙↙↑                          ↑→→→→→→→→→→→→→→→→→↑↑↗→→↑↑     ↑↙↙
//! ↙↓↑                         ←↑→→→→→→→→→→→→→→→→→↑↑↑→→↑↑    ↑↑↙↙
//! ↙↙↑                         ↑→→→→→→→→→→→→→→→→→→↗↑↑↑→→↑    ↑↗↘↙
//! ↙↙↑                        ↑→→→→→→→→→→→→→→→→→→→→↑→↑↑↑     ↑↙→↙
//! ↙↘↗↑                      ↑→→→→→→→→→→→→→→→→→→→→↑↑        ↑↑→→↙
//! ↙→↙↑↑                    ↑→→→→→→→→→→→→→→→→→→↑↑           ↑↙↗→↙
//! ↙↗→↓↑                   ↑↑→→→→→→→→→→→→→→→→↑↙            ↑↑→↗→↙
//! ↙→→↓↑↑                  ↑→→→→→→→→→→→→→→↑↑              ↑↑↙→→↗↙
//! ↙↗↗→↓↑↑                ↑↑→→→→→→→→→→→↑↑↖               ↑↑↓→↗↗↗↙
//! ↙↗↗↗↗↘↗↑↓              ↑↑→→→→→→→→→↑↙                ↑↑↗↘→→→↗↗↙
//! ↙↗↗→↗→↓→↑↑             ↑↑→→→→→→↑↑                  ↑↑↘↓→→↗↗↗↗↙
//! ↙↗↗↗↗↗↗→↓↗↑↑            ↑→→→↗↑                   ↑↑→↓↓→↗↗↗↗↗↗↙
//! ↙↗↗↗↗↗→→↗↘↘↑↑↖          ↑→↑↑                   ↑↑↗↙→↗↗↗↗↗↗↗↗↗↙
//! ↙↗↗↗↗↗↗↗↗↗→↓↘↑↑↑        ↑↑                   ↑↑↗↓↓→→→→↗↗↗↗↗↗↗↙
//! ↙↗↗↗↗↗↗→→→↗→↘↓↓↗↑↑↑←                     ↑↑↑↑↘↙↓→↗↗↗↗↗↗↗↗↗↗↗↗↙
//! ↙↗↗↗↗↗↗↗↗↗↗↗↗↗→↘↙↓↑↑↑↑↑↗           ↖↑↑↑↑↗↓↓↓→↗↗↗↗↗↗↗↗↗↗↗↗↗↗↗↙
//! ↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙→↗↑↑↑↑↑↑↑↑↑↑→↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙↙
//!
//!                    A R V A K
//!          Rust-Native Quantum Compilation
//!              for HPC Environments
//!
//!            "Swift as the eternal steeds"
//! ```

use clap::{Parser, Subcommand};
use console::style;
use tracing_subscriber::EnvFilter;

mod commands;

use commands::{backends, compile, run, version};

/// Arvak - Rust-native quantum compilation and orchestration for HPC
#[derive(Parser)]
#[command(name = "arvak")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a quantum circuit for a target backend
    Compile {
        /// Input file (QASM3 or JSON)
        #[arg(short, long)]
        input: String,

        /// Output file
        #[arg(short, long)]
        output: Option<String>,

        /// Target backend (iqm, ibm, simulator)
        #[arg(short, long, default_value = "iqm")]
        target: String,

        /// Optimization level (0-3)
        #[arg(long, default_value = "1")]
        optimization_level: u8,
    },

    /// Run a circuit on a backend
    Run {
        /// Input file (QASM3 or JSON)
        #[arg(short, long)]
        input: String,

        /// Number of shots
        #[arg(short, long, default_value = "1024")]
        shots: u32,

        /// Backend to use
        #[arg(short, long, default_value = "simulator")]
        backend: String,

        /// Compile before running
        #[arg(long)]
        compile: bool,

        /// Target for compilation
        #[arg(long)]
        target: Option<String>,
    },

    /// List available backends
    Backends,

    /// Show version information
    Version,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Setup logging
    let filter = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_target(false)
        .init();

    // Execute command
    let result = match cli.command {
        Commands::Compile {
            input,
            output,
            target,
            optimization_level,
        } => compile::execute(&input, output.as_deref(), &target, optimization_level).await,

        Commands::Run {
            input,
            shots,
            backend,
            compile: do_compile,
            target,
        } => run::execute(&input, shots, &backend, do_compile, target.as_deref()).await,

        Commands::Backends => backends::execute().await,

        Commands::Version => {
            version::execute();
            Ok(())
        }
    };

    // Handle errors
    if let Err(e) = result {
        eprintln!("{} {}", style("Error:").red().bold(), e);
        std::process::exit(1);
    }

    Ok(())
}
