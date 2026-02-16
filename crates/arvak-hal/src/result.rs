//! Execution result types.
//!
//! # HAL Contract v2
//!
//! Bitstring ordering: the rightmost bit corresponds to the
//! lowest-indexed qubit (OpenQASM 3 convention). For example,
//! the string `"01"` means qubit 0 measured `1` and qubit 1
//! measured `0`.

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Measurement counts from circuit execution.
///
/// Maps bitstrings to occurrence counts. Bitstring ordering follows
/// the OpenQASM 3 convention (rightmost bit = lowest qubit index).
// Implementation note: uses FxHashMap for performance.
// Spec defines HashMap<String, u64> — semantically equivalent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Counts {
    /// Map from bitstring to count.
    counts: FxHashMap<String, u64>,
}

impl Counts {
    /// Create empty counts.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create counts from an iterator of (bitstring, count) pairs.
    /// Duplicate bitstrings are accumulated (summed), consistent with `insert()`.
    pub fn from_pairs(iter: impl IntoIterator<Item = (impl Into<String>, u64)>) -> Self {
        let mut counts = Self::new();
        for (k, v) in iter {
            counts.insert(k, v);
        }
        counts
    }

    /// Insert a count for a bitstring.
    pub fn insert(&mut self, bitstring: impl Into<String>, count: u64) {
        let key = bitstring.into();
        *self.counts.entry(key).or_default() += count;
    }

    /// Get the count for a bitstring.
    pub fn get(&self, bitstring: &str) -> u64 {
        self.counts.get(bitstring).copied().unwrap_or(0)
    }

    /// Iterate over (bitstring, count) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &u64)> {
        self.counts.iter()
    }

    /// Get the total number of shots.
    pub fn total_shots(&self) -> u64 {
        self.counts.values().sum()
    }

    /// Get the most frequent bitstring.
    pub fn most_frequent(&self) -> Option<(&String, &u64)> {
        self.counts.iter().max_by_key(|&(_, count)| count)
    }

    /// Get probabilities for each bitstring.
    #[allow(clippy::cast_precision_loss)]
    pub fn probabilities(&self) -> FxHashMap<String, f64> {
        let total = self.total_shots() as f64;
        if total == 0.0 {
            return FxHashMap::default();
        }
        self.counts
            .iter()
            .map(|(k, &v)| (k.clone(), v as f64 / total))
            .collect()
    }

    /// Get the number of unique bitstrings.
    pub fn len(&self) -> usize {
        self.counts.len()
    }

    /// Check if counts are empty.
    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// Arvak extension — not part of HAL Contract v2 spec.
    /// Get sorted counts (by count, descending).
    pub fn sorted(&self) -> Vec<(&String, &u64)> {
        let mut items: Vec<_> = self.counts.iter().collect();
        items.sort_by(|a, b| b.1.cmp(a.1));
        items
    }
}

impl FromIterator<(String, u64)> for Counts {
    fn from_iter<I: IntoIterator<Item = (String, u64)>>(iter: I) -> Self {
        let mut counts = Self::new();
        for (key, value) in iter {
            counts.insert(key, value);
        }
        counts
    }
}

/// Result of circuit execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Measurement counts.
    pub counts: Counts,
    /// Number of shots executed.
    pub shots: u32,
    /// Execution time in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time_ms: Option<u64>,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl ExecutionResult {
    /// Create a new execution result.
    pub fn new(counts: Counts, shots: u32) -> Self {
        Self {
            counts,
            shots,
            execution_time_ms: None,
            metadata: serde_json::Value::Null,
        }
    }

    /// Set the execution time.
    pub fn with_execution_time(mut self, time_ms: u64) -> Self {
        self.execution_time_ms = Some(time_ms);
        self
    }

    /// Set metadata.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    /// Get probabilities for each bitstring.
    pub fn probabilities(&self) -> FxHashMap<String, f64> {
        self.counts.probabilities()
    }

    /// Get the most frequent measurement result.
    #[allow(clippy::cast_precision_loss)]
    pub fn most_frequent(&self) -> Option<(&String, f64)> {
        let total = self.counts.total_shots() as f64;
        if total == 0.0 {
            return None;
        }
        self.counts
            .most_frequent()
            .map(|(s, &c)| (s, c as f64 / total))
    }
}

impl Default for ExecutionResult {
    fn default() -> Self {
        Self::new(Counts::new(), 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counts_basic() {
        let mut counts = Counts::new();
        counts.insert("00", 500);
        counts.insert("11", 500);

        assert_eq!(counts.get("00"), 500);
        assert_eq!(counts.get("11"), 500);
        assert_eq!(counts.get("01"), 0);
        assert_eq!(counts.total_shots(), 1000);
    }

    #[test]
    fn test_counts_probabilities() {
        let counts = Counts::from_pairs([
            ("00".to_string(), 300),
            ("01".to_string(), 200),
            ("10".to_string(), 300),
            ("11".to_string(), 200),
        ]);

        let probs = counts.probabilities();
        assert!((probs["00"] - 0.3).abs() < 1e-10);
        assert!((probs["01"] - 0.2).abs() < 1e-10);
    }

    #[test]
    fn test_counts_most_frequent() {
        let counts = Counts::from_pairs([("00".to_string(), 100), ("11".to_string(), 900)]);

        let (most, count) = counts.most_frequent().unwrap();
        assert_eq!(most, "11");
        assert_eq!(*count, 900);
    }

    #[test]
    fn test_execution_result() {
        let counts = Counts::from_pairs([("00".to_string(), 500), ("11".to_string(), 500)]);

        let result = ExecutionResult::new(counts, 1000).with_execution_time(42);

        assert_eq!(result.shots, 1000);
        assert_eq!(result.execution_time_ms, Some(42));

        let (_most, prob) = result.most_frequent().unwrap();
        assert!((prob - 0.5).abs() < 1e-10);
    }
}
