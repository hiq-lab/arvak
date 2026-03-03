//! Perceval bridge — subprocess interface to the Quandela Cloud API.
//!
//! Calls `perceval_bridge.py` via `python3` (or the interpreter given by
//! `QUANDELA_PYTHON`) and returns its JSON output.
//!
//! # Environment variables
//!
//! | Variable              | Description                                          |
//! |-----------------------|------------------------------------------------------|
//! | `PCVL_CLOUD_TOKEN`    | Quandela cloud token (required for cloud calls)      |
//! | `QUANDELA_PYTHON`     | Python interpreter path (default: `python3`)         |
//! | `QUANDELA_BRIDGE_PATH`| Override path to `perceval_bridge.py`                |

use std::path::{Path, PathBuf};

use crate::error::{QuandelaError, QuandelaResult};

/// Maximum number of cached circuit metadata entries (per CLAUDE.md rules).
pub const MAX_CACHED_JOBS: usize = 10_000;

/// Call the Perceval bridge script with the given arguments.
///
/// If `token` is `Some`, it is passed to the subprocess as the
/// `PCVL_CLOUD_TOKEN` environment variable (overriding any inherited value).
///
/// The bridge always writes a JSON object to stdout. If the object contains a
/// non-null `"error"` key, this function returns `QuandelaError::BridgeError`.
pub async fn call_bridge(
    python: &str,
    script: &Path,
    args: &[&str],
    token: Option<&str>,
) -> QuandelaResult<serde_json::Value> {
    let python = python.to_string();
    let script = script.to_path_buf();
    let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    let token: Option<String> = token.map(std::string::ToString::to_string);

    tokio::task::spawn_blocking(move || {
        let mut cmd = std::process::Command::new(&python);
        cmd.arg(&script).args(&args);
        if let Some(ref t) = token {
            cmd.env("PCVL_CLOUD_TOKEN", t);
        }
        let output = cmd
            .output()
            .map_err(|e| QuandelaError::BridgeError(format!("failed to spawn bridge: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        // Bridge always writes JSON to stdout; non-zero exit without JSON means
        // a hard failure (import error, missing Python, etc.).
        if stdout.trim().is_empty() {
            return Err(QuandelaError::BridgeError(format!(
                "bridge produced no output (exit {}): {stderr}",
                output.status
            )));
        }

        let value: serde_json::Value = serde_json::from_str(stdout.trim()).map_err(|e| {
            QuandelaError::BridgeError(format!("bridge JSON parse error: {e}; stdout: {stdout:?}"))
        })?;

        if let Some(err) = value.get("error").and_then(|e| e.as_str()) {
            if !err.is_empty() {
                return Err(QuandelaError::BridgeError(format!("bridge error: {err}")));
            }
        }

        Ok(value)
    })
    .await
    .map_err(|e| QuandelaError::BridgeError(format!("spawn_blocking join error: {e}")))?
}

/// Return the Python interpreter to use.
///
/// Checks `QUANDELA_PYTHON` first, then defaults to `"python3"`.
pub fn find_python() -> String {
    std::env::var("QUANDELA_PYTHON").unwrap_or_else(|_| "python3".to_string())
}

/// Return the path to `perceval_bridge.py`.
///
/// Resolution order:
/// 1. `QUANDELA_BRIDGE_PATH` env var
/// 2. Next to the running executable
/// 3. `CARGO_MANIFEST_DIR/perceval_bridge.py` (dev / test fallback)
pub fn find_bridge_script() -> PathBuf {
    if let Ok(p) = std::env::var("QUANDELA_BRIDGE_PATH") {
        return PathBuf::from(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe
            .parent()
            .unwrap_or(Path::new("."))
            .join("perceval_bridge.py");
        if candidate.exists() {
            return candidate;
        }
    }
    // Dev fallback: next to Cargo.toml of this crate.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("perceval_bridge.py")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_python_non_empty() {
        // The result is whatever python3 or QUANDELA_PYTHON resolves to.
        assert!(!find_python().is_empty());
    }

    #[test]
    fn test_find_bridge_fallback_exists() {
        // In the dev tree the CARGO_MANIFEST_DIR fallback must exist.
        // (env-var override tests are omitted: set_var is unsafe and racy
        // in parallel test runs.)
        let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("perceval_bridge.py");
        assert!(fallback.exists(), "bridge script not found at {fallback:?}");
    }
}
