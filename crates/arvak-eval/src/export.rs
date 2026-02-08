//! Export Module: JSON output with schema and metadata.
//!
//! Handles serialization of evaluation reports to structured JSON.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::{EvalError, EvalResult};
use crate::report::EvalReport;

/// Export configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    /// Whether to pretty-print JSON output.
    pub pretty: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self { pretty: true }
    }
}

/// Export an evaluation report to JSON string.
pub fn to_json(report: &EvalReport, config: &ExportConfig) -> EvalResult<String> {
    if config.pretty {
        serde_json::to_string_pretty(report).map_err(EvalError::from)
    } else {
        serde_json::to_string(report).map_err(EvalError::from)
    }
}

/// Export an evaluation report to a JSON file.
pub fn to_file(report: &EvalReport, path: &Path, config: &ExportConfig) -> EvalResult<()> {
    let json = to_json(report, config)?;
    std::fs::write(path, json).map_err(|e| {
        EvalError::Io(format!("Failed to write {}: {}", path.display(), e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_config_default() {
        let config = ExportConfig::default();
        assert!(config.pretty);
    }
}
