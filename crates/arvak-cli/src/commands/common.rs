//! Shared helpers for CLI commands.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};

use arvak_compile::{BasisGates, CouplingMap};
use arvak_ir::Circuit;
use arvak_qasm3::parse;
use arvak_sched::{HpcScheduler, SchedulerConfig, SqliteStore};

/// Load a circuit from a QASM3 or JSON file.
pub fn load_circuit(path: &str) -> Result<Circuit> {
    let path_obj = Path::new(path);

    if !path_obj.exists() {
        anyhow::bail!("File not found: {path}");
    }

    let source =
        fs::read_to_string(path).with_context(|| format!("Failed to read file: {path}"))?;

    let ext = path_obj.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext.to_lowercase().as_str() {
        "qasm" | "qasm3" => parse(&source).map_err(|e| anyhow::anyhow!("Parse error: {e}")),
        "json" => {
            anyhow::bail!("JSON format not yet supported")
        }
        _ => parse(&source).map_err(|e| anyhow::anyhow!("Parse error: {e}")),
    }
}

/// Get target coupling map and basis gates for a named target.
pub fn get_target_properties(target: &str) -> Result<(CouplingMap, BasisGates)> {
    match target.to_lowercase().as_str() {
        "iqm" | "iqm5" => Ok((CouplingMap::star(5), BasisGates::iqm())),
        "iqm20" => Ok((CouplingMap::star(20), BasisGates::iqm())),
        "ibm" | "ibm5" => Ok((CouplingMap::linear(5), BasisGates::ibm())),
        "ibm27" => Ok((CouplingMap::linear(27), BasisGates::ibm())),
        "ibm_torino" | "ibm_fez" | "ibm_marrakesh" => {
            Ok((CouplingMap::linear(133), BasisGates::heron()))
        }
        "simulator" | "sim" => Ok((CouplingMap::full(20), BasisGates::universal())),
        "braket" | "braket-sv1" | "sv1" | "braket-tn1" | "tn1" | "braket-dm1" | "dm1" => {
            Ok((CouplingMap::full(34), BasisGates::universal()))
        }
        "rigetti" | "ankaa" => Ok((
            CouplingMap::linear(84),
            BasisGates::new(["rx", "rz", "cz"].map(String::from)),
        )),
        "ionq" | "aria" => Ok((
            CouplingMap::full(25),
            BasisGates::new(["rx", "ry", "rz", "xx"].map(String::from)),
        )),
        other => {
            anyhow::bail!(
                "Unknown target: '{other}'. Available: iqm, iqm5, iqm20, ibm, ibm5, ibm27, ibm_torino, ibm_fez, ibm_marrakesh, simulator, braket, rigetti, ionq"
            );
        }
    }
}

/// Return the default Arvak state directory (~/.arvak/).
pub fn default_state_dir() -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    let state_dir = home.join(".arvak");
    if !state_dir.exists() {
        fs::create_dir_all(&state_dir).with_context(|| {
            format!("Failed to create state directory: {}", state_dir.display())
        })?;
    }
    Ok(state_dir)
}

/// Create an `HpcScheduler` with mock SLURM adapter backed by local `SQLite` store.
///
/// Used by `status`, `result`, and `wait` commands to query local job state
/// without requiring a real SLURM/PBS installation.
pub fn create_scheduler() -> Result<HpcScheduler> {
    let state_dir = default_state_dir()?;
    let db_path = state_dir.join("jobs.db");
    let store = SqliteStore::new(&db_path)
        .map_err(|e| anyhow::anyhow!("Failed to open job store at {}: {}", db_path.display(), e))?;
    let config = SchedulerConfig::default();
    Ok(HpcScheduler::with_mock_slurm(
        config,
        vec![],
        Arc::new(store),
    ))
}

/// Print execution results in a table format (shared by run, result, wait).
pub fn print_results(result: &arvak_hal::ExecutionResult) {
    use console::style;

    println!(
        "\n{} Results ({} shots):",
        style("✓").green().bold(),
        result.shots
    );

    let sorted = result.counts.sorted();
    let total = result.counts.total_shots() as f64;

    for (bitstring, count) in sorted.iter().take(16) {
        let prob = **count as f64 / total * 100.0;
        let bar_len = (prob / 2.0).round() as usize;
        let bar: String = "█".repeat(bar_len);

        println!(
            "  {}: {:>6} ({:>5.2}%) {}",
            style(bitstring).cyan(),
            count,
            prob,
            style(bar).green()
        );
    }

    if sorted.len() > 16 {
        println!("  ... and {} more outcomes", sorted.len() - 16);
    }

    if let Some(time_ms) = result.execution_time_ms {
        println!("\n  Execution time: {} ms", style(time_ms).yellow());
    }
}
