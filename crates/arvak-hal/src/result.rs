//! Execution result types — re-exported from HAL Contract spec.
//!
//! # HAL Contract v2
//!
//! Bitstring ordering: the rightmost bit corresponds to the
//! lowest-indexed qubit (OpenQASM 3 convention). For example,
//! the string `"01"` means qubit 0 measured `1` and qubit 1
//! measured `0`.

pub use hal_contract::result::{Counts, ExecutionResult};

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
