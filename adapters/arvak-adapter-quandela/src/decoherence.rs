//! `DecoherenceMonitor` implementation for Quandela Altair.
//!
//! Quandela Altair is a photonic QPU. T1/T2* and the compressor-based fingerprint
//! methods are not applicable. All superconducting methods return `None`.
//!
//! HOM visibility methods also return `None` (inherit the default impl): the
//! photonic path goes through `ingest_alsvid_enrollment` / `ingest_alsvid_schedule`
//! on `QuandelaBackend`, which ingests pre-computed alsvid-lab output rather than
//! submitting live measurement circuits. DEBT-Q5: would override here if a Rust
//! Perceval client were available.

use arvak_hal::capability::DecoherenceMonitor;

use crate::backend::QuandelaBackend;

impl DecoherenceMonitor for QuandelaBackend {
    /// Not applicable to photonic backends. Returns `None`.
    // DEBT-Q1: not applicable to photonic QPU; use measure_hom_visibility instead
    fn measure_t1(&self, _qubit_indices: &[u32], _shots: u32) -> Option<f64> {
        None
    }

    /// Not applicable to photonic backends. Returns `None`.
    // DEBT-Q1: not applicable to photonic QPU; use measure_hom_visibility instead
    fn measure_t2_star(&self, _qubit_indices: &[u32], _shots: u32) -> Option<f64> {
        None
    }

    /// Not applicable to photonic backends. Returns `None`.
    // Use ingest_alsvid_enrollment instead for HOM-based PUF enrollment
    fn compute_fingerprint(&self, _sample_count: u32, _shots_per_sample: u32) -> Option<String> {
        None
    }

    // measure_hom_visibility and compute_hom_fingerprint inherit default None impls.
    // DEBT-Q5: override these once a Rust Perceval client is available.
}

#[cfg(test)]
mod tests {
    use arvak_hal::capability::DecoherenceMonitor;

    use crate::backend::QuandelaBackend;

    fn make_backend() -> QuandelaBackend {
        QuandelaBackend::with_key("test-key").unwrap()
    }

    #[test]
    fn test_measure_t1_returns_none() {
        let b = make_backend();
        assert!(b.measure_t1(&[0, 1], 100).is_none());
    }

    #[test]
    fn test_measure_t2_star_returns_none() {
        let b = make_backend();
        assert!(b.measure_t2_star(&[0], 100).is_none());
    }

    #[test]
    fn test_compute_fingerprint_returns_none() {
        let b = make_backend();
        assert!(b.compute_fingerprint(8, 1000).is_none());
    }

    #[test]
    fn test_measure_hom_visibility_returns_none() {
        let b = make_backend();
        assert!(b.measure_hom_visibility(100).is_none());
    }

    #[test]
    fn test_compute_hom_fingerprint_returns_none() {
        let b = make_backend();
        assert!(b.compute_hom_fingerprint(8, 100).is_none());
    }
}
