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
//!       "Calm down HAL, Arvak's got Dave's back"
//! ```

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use clap::{Parser, Subcommand};
use console::style;
use tracing_subscriber::EnvFilter;

mod commands;

use commands::{auth, backends, compile, eval, result, run, status, submit, version, wait};

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

    /// Submit a circuit to an HPC batch scheduler
    Submit {
        /// Input file (QASM3 or JSON)
        #[arg(short, long)]
        input: String,

        /// Backend to use (simulator, iqm, ibm)
        #[arg(short, long, default_value = "simulator")]
        backend: String,

        /// Number of shots
        #[arg(short, long, default_value = "1024")]
        shots: u32,

        /// Batch scheduler (slurm, pbs)
        #[arg(long, default_value = "slurm")]
        scheduler: String,

        /// Scheduler partition/queue name
        #[arg(long)]
        partition: Option<String>,

        /// Scheduler account/project
        #[arg(long)]
        account: Option<String>,

        /// Wall time limit (HH:MM:SS)
        #[arg(long)]
        time: Option<String>,

        /// Job priority (low, default, high, critical)
        #[arg(long)]
        priority: Option<String>,

        /// Wait for job to complete
        #[arg(short, long)]
        wait: bool,
    },

    /// Query job status
    Status {
        /// Job ID (UUID)
        job_id: Option<String>,

        /// List all jobs
        #[arg(short, long)]
        all: bool,
    },

    /// Retrieve results for a completed job
    Result {
        /// Job ID (UUID)
        job_id: String,

        /// Output format (table, json)
        #[arg(short, long, default_value = "table")]
        format: String,
    },

    /// Manage authentication for HPC providers
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },

    /// Wait for a job to complete
    Wait {
        /// Job ID (UUID)
        job_id: String,

        /// Timeout in seconds
        #[arg(short, long, default_value = "86400")]
        timeout: u64,
    },

    /// Evaluate a circuit: compilation observability, QDMI contract check, metrics
    Eval {
        /// Input file (QASM3)
        #[arg(short, long)]
        input: String,

        /// Evaluation profile
        #[arg(short, long, default_value = "default")]
        profile: String,

        /// Target backend (iqm, ibm, simulator)
        #[arg(short, long, default_value = "iqm")]
        target: String,

        /// Optimization level (0-3)
        #[arg(long, default_value = "1")]
        optimization_level: u8,

        /// Number of qubits on target device
        #[arg(long, default_value = "20")]
        target_qubits: u32,

        /// Output file for JSON report (stdout if omitted)
        #[arg(short, long)]
        export: Option<String>,

        /// Include orchestration analysis (hybrid DAG, batchability, critical path)
        #[arg(long)]
        orchestration: bool,

        /// HPC scheduler site for constraints (lrz, lumi)
        #[arg(long)]
        scheduler_site: Option<String>,

        /// Emitter compliance target (iqm, ibm, cuda-q)
        #[arg(long)]
        emit: Option<String>,

        /// Optional benchmark workload (ghz, qft, grover, random)
        #[arg(long)]
        benchmark: Option<String>,

        /// Number of qubits for benchmark circuit (defaults to input circuit size)
        #[arg(long)]
        benchmark_qubits: Option<usize>,
    },

    /// List available backends
    Backends,

    /// Show version information
    Version,
}

#[derive(Subcommand)]
enum AuthAction {
    /// Log in to an HPC provider
    Login {
        /// Provider (csc, lumi, lrz)
        #[arg(short, long)]
        provider: String,

        /// Project ID
        #[arg(long)]
        project: Option<String>,
    },

    /// Show authentication status
    Status {
        /// Provider to check (optional, checks all if omitted)
        #[arg(short, long)]
        provider: Option<String>,
    },

    /// Log out and clear cached tokens
    Logout {
        /// Provider to log out from (optional, logs out all if omitted)
        #[arg(short, long)]
        provider: Option<String>,
    },
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

        Commands::Submit {
            input,
            backend,
            shots,
            scheduler,
            partition,
            account,
            time,
            priority,
            wait: do_wait,
        } => {
            submit::execute(
                &input,
                &backend,
                shots,
                &scheduler,
                partition.as_deref(),
                account.as_deref(),
                time.as_deref(),
                priority.as_deref(),
                do_wait,
            )
            .await
        }

        Commands::Status { job_id, all } => status::execute(job_id.as_deref(), all).await,

        Commands::Result { job_id, format } => result::execute(&job_id, &format).await,

        Commands::Auth { action } => match action {
            AuthAction::Login { provider, project } => {
                auth::execute_login(&provider, project.as_deref()).await
            }
            AuthAction::Status { provider } => auth::execute_status(provider.as_deref()).await,
            AuthAction::Logout { provider } => auth::execute_logout(provider.as_deref()).await,
        },

        Commands::Wait { job_id, timeout } => wait::execute(&job_id, timeout).await,

        Commands::Eval {
            input,
            profile,
            target,
            optimization_level,
            target_qubits,
            export,
            orchestration,
            scheduler_site,
            emit,
            benchmark,
            benchmark_qubits,
        } => {
            eval::execute(
                &input,
                &profile,
                &target,
                optimization_level,
                export.as_deref(),
                target_qubits,
                orchestration,
                scheduler_site.as_deref(),
                emit.as_deref(),
                benchmark.as_deref(),
                benchmark_qubits,
            )
            .await
        }

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
