//! Reproducibility Module: CLI snapshots, versioning, and artifact tracking.
//!
//! Captures all information needed to reproduce an evaluation run.

use serde::{Deserialize, Serialize};

/// Information for reproducing an evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReproducibilityInfo {
    /// Arvak version used.
    pub arvak_version: String,
    /// CLI arguments used for this evaluation.
    pub cli_args: Vec<String>,
    /// Schema version of the output format.
    pub schema_version: String,
    /// Evaluator module version.
    pub eval_version: String,
}

impl ReproducibilityInfo {
    /// Capture current reproducibility context.
    pub fn capture(cli_args: &[String]) -> Self {
        Self {
            arvak_version: env!("CARGO_PKG_VERSION").to_string(),
            cli_args: cli_args.to_vec(),
            schema_version: "0.1.0".into(),
            eval_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reproducibility_capture() {
        let args = vec![
            "arvak".into(),
            "eval".into(),
            "--input".into(),
            "test.qasm3".into(),
        ];
        let info = ReproducibilityInfo::capture(&args);

        assert!(!info.arvak_version.is_empty());
        assert_eq!(info.cli_args.len(), 4);
        assert_eq!(info.schema_version, "0.1.0");
    }
}
