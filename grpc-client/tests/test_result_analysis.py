"""Tests for result analysis functionality."""

import pytest
import math
from arvak_grpc.types import JobResult
from arvak_grpc.result_analysis import (
    ResultAggregator,
    ResultComparator,
    ConvergenceAnalyzer,
    ResultTransformer,
    batch_compare,
    group_by_similarity,
)


@pytest.fixture
def sample_results():
    """Sample JobResults."""
    return [
        JobResult(f"test-{i}", {"00": 45 + i*5, "11": 55 - i*5}, 100)
        for i in range(5)
    ]


@pytest.fixture
def bell_result():
    """Bell state result."""
    return JobResult("bell", {"00": 500, "11": 500}, 1000)


@pytest.fixture
def ghz_result():
    """GHZ state result."""
    return JobResult("ghz", {"000": 512, "111": 488}, 1000)


class TestResultAggregator:
    """Test ResultAggregator functionality."""

    def test_combine_results(self, sample_results):
        """Test combining multiple results."""
        combined = ResultAggregator.combine(sample_results)

        assert combined.num_jobs == 5
        assert combined.total_shots == 500  # 5 * 100
        assert "00" in combined.counts
        assert "11" in combined.counts

        # Total combined counts should equal total shots
        assert sum(combined.counts.values()) == combined.total_shots

    def test_combine_empty(self):
        """Test combining empty list."""
        combined = ResultAggregator.combine([])

        assert combined.num_jobs == 0
        assert combined.total_shots == 0
        assert len(combined.counts) == 0

    def test_average_results(self, sample_results):
        """Test averaging results."""
        averaged = ResultAggregator.average(sample_results)

        assert averaged.shots == sample_results[0].shots
        assert sum(averaged.counts.values()) == averaged.shots

        # Probabilities should be averaged
        probs = {bs: c / averaged.shots for bs, c in averaged.counts.items()}
        assert abs(sum(probs.values()) - 1.0) < 1e-10

    def test_filter_by_count(self):
        """Test filtering by count threshold."""
        result = JobResult("test", {"00": 100, "01": 10, "10": 5, "11": 85}, 200)

        filtered = ResultAggregator.filter_by_threshold(result, min_count=20)

        assert len(filtered.counts) == 2  # Only 00 and 11
        assert "00" in filtered.counts
        assert "11" in filtered.counts
        assert "01" not in filtered.counts

    def test_filter_by_probability(self):
        """Test filtering by probability threshold."""
        result = JobResult("test", {"00": 100, "01": 10, "10": 5, "11": 85}, 200)

        filtered = ResultAggregator.filter_by_threshold(result, min_probability=0.1)

        # Only states with prob >= 0.1
        assert len(filtered.counts) == 2  # 00 (0.5) and 11 (0.425)

    def test_top_k_states(self):
        """Test keeping only top-k states."""
        result = JobResult(
            "test",
            {"00": 100, "01": 50, "10": 30, "11": 20},
            200
        )

        top2 = ResultAggregator.top_k_states(result, k=2)

        assert len(top2.counts) == 2
        assert "00" in top2.counts  # Most common
        assert "01" in top2.counts  # Second most common
        assert "10" not in top2.counts
        assert "11" not in top2.counts


class TestResultComparator:
    """Test ResultComparator functionality."""

    def test_identical_distributions(self, bell_result):
        """Test comparison of identical distributions."""
        comparison = ResultComparator.compare(bell_result, bell_result)

        assert abs(comparison.tvd - 0.0) < 1e-10
        assert abs(comparison.overlap - 1.0) < 1e-10
        assert abs(comparison.hellinger_distance - 0.0) < 1e-10

    def test_orthogonal_distributions(self):
        """Test comparison of orthogonal distributions."""
        result1 = JobResult("test1", {"00": 1000}, 1000)
        result2 = JobResult("test2", {"11": 1000}, 1000)

        comparison = ResultComparator.compare(result1, result2)

        assert abs(comparison.tvd - 1.0) < 1e-10
        assert abs(comparison.overlap - 0.0) < 1e-10

    def test_similar_distributions(self, bell_result):
        """Test comparison of similar distributions."""
        result2 = JobResult("bell2", {"00": 510, "11": 490}, 1000)

        comparison = ResultComparator.compare(bell_result, result2)

        # Should be very similar
        assert comparison.tvd < 0.05
        assert comparison.overlap > 0.99

    def test_kl_divergence(self, bell_result):
        """Test KL divergence calculation."""
        result2 = JobResult("bell2", {"00": 600, "11": 400}, 1000)

        comparison = ResultComparator.compare(bell_result, result2)

        # KL divergence should be defined and positive
        assert comparison.kl_divergence >= 0.0
        assert not math.isinf(comparison.kl_divergence)

    def test_js_divergence(self, bell_result):
        """Test Jensen-Shannon divergence."""
        result2 = JobResult("bell2", {"00": 700, "11": 300}, 1000)

        comparison = ResultComparator.compare(bell_result, result2)

        # JS divergence should be in [0, 1]
        assert 0.0 <= comparison.js_divergence <= 1.0


class TestConvergenceAnalyzer:
    """Test ConvergenceAnalyzer functionality."""

    def test_convergence_analysis(self):
        """Test convergence analysis with increasing shots."""
        # Simulate results with increasing shots
        results = [
            JobResult(f"test-{i}", {"00": 50, "11": 50}, 100 * (2**i))
            for i in range(5)
        ]

        ideal = {"00": 0.5, "11": 0.5}
        analysis = ConvergenceAnalyzer.analyze_convergence(
            results,
            target_state=ideal,
            threshold=0.01,
        )

        assert len(analysis.shot_counts) == 5
        assert len(analysis.entropies) == 5
        assert len(analysis.fidelities) == 5

        # All fidelities should be high
        for fid in analysis.fidelities:
            assert fid > 0.9

    def test_estimate_required_shots(self):
        """Test estimation of required shots."""
        pilot_results = [
            JobResult(f"pilot-{i}", {"00": 50, "11": 50}, 100)
            for i in range(3)
        ]

        required = ConvergenceAnalyzer.estimate_required_shots(
            pilot_results,
            target_precision=0.01,
        )

        # Should recommend reasonable number of shots
        assert required >= 100
        assert required <= 100000  # Sanity check

    def test_convergence_detection(self):
        """Test detection of convergence."""
        # Stable results (converged)
        stable_results = [
            JobResult(f"test-{i}", {"00": 500, "11": 500}, 1000)
            for i in range(5)
        ]

        analysis = ConvergenceAnalyzer.analyze_convergence(
            stable_results,
            threshold=0.01,
        )

        # Should detect convergence
        assert analysis.converged

        # Unstable results (not converged)
        unstable_results = [
            JobResult(f"test-{i}", {"00": 400 + i*100, "11": 600 - i*100}, 1000)
            for i in range(5)
        ]

        analysis2 = ConvergenceAnalyzer.analyze_convergence(
            unstable_results,
            threshold=0.01,
        )

        # Should not detect convergence
        assert not analysis2.converged


class TestResultTransformer:
    """Test ResultTransformer functionality."""

    def test_normalize(self):
        """Test normalization of counts."""
        # Counts don't sum to shots
        result = JobResult("test", {"00": 48, "11": 47}, 100)

        normalized = ResultTransformer.normalize(result)

        # Should now sum exactly to shots
        assert sum(normalized.counts.values()) == normalized.shots

    def test_downsample(self):
        """Test downsampling."""
        result = JobResult("test", {"00": 500, "11": 500}, 1000)

        downsampled = ResultTransformer.downsample(result, target_shots=100)

        assert downsampled.shots == 100
        assert sum(downsampled.counts.values()) == 100

        # Probabilities should be roughly preserved
        orig_prob_00 = 500 / 1000
        down_prob_00 = downsampled.counts.get("00", 0) / 100
        assert abs(orig_prob_00 - down_prob_00) < 0.2  # Allow some variance

    def test_downsample_no_op(self):
        """Test that downsampling with larger target is no-op."""
        result = JobResult("test", {"00": 50, "11": 50}, 100)

        downsampled = ResultTransformer.downsample(result, target_shots=200)

        assert downsampled.shots == result.shots
        assert downsampled.counts == result.counts

    def test_apply_noise(self):
        """Test applying noise."""
        result = JobResult("test", {"00": 1000}, 1000)  # Pure state

        noisy = ResultTransformer.apply_noise(result, error_rate=0.1, seed=42)

        # Should have more states due to noise
        assert len(noisy.counts) > 1
        assert noisy.shots == 1000

        # 00 should still be most common but not all
        assert "00" in noisy.counts
        assert noisy.counts["00"] < 1000

    def test_noise_with_seed(self):
        """Test that noise with seed is reproducible."""
        result = JobResult("test", {"00": 100}, 100)

        noisy1 = ResultTransformer.apply_noise(result, error_rate=0.1, seed=42)
        noisy2 = ResultTransformer.apply_noise(result, error_rate=0.1, seed=42)

        # Should be identical with same seed
        assert noisy1.counts == noisy2.counts


class TestBatchOperations:
    """Test batch comparison and grouping."""

    def test_batch_compare(self, sample_results):
        """Test pairwise comparison."""
        comparisons = batch_compare(sample_results)

        # Should have n*(n-1)/2 comparisons
        expected_count = len(sample_results) * (len(sample_results) - 1) // 2
        assert len(comparisons) == expected_count

        # All comparisons should be valid
        for (i, j), comp in comparisons.items():
            assert i < j
            assert 0.0 <= comp.tvd <= 1.0
            assert 0.0 <= comp.overlap <= 1.0

    def test_group_by_similarity(self):
        """Test grouping by similarity."""
        # Create two distinct groups
        group1_results = [
            JobResult(f"g1-{i}", {"00": 500, "11": 500}, 1000)
            for i in range(3)
        ]

        group2_results = [
            JobResult(f"g2-{i}", {"00": 900, "11": 100}, 1000)
            for i in range(2)
        ]

        all_results = group1_results + group2_results

        groups = group_by_similarity(all_results, threshold=0.1)

        # Should identify 2 groups
        assert len(groups) == 2

        # Sizes should be correct
        group_sizes = sorted([len(g) for g in groups], reverse=True)
        assert group_sizes == [3, 2]

    def test_group_by_similarity_single(self, bell_result):
        """Test grouping with single result."""
        groups = group_by_similarity([bell_result], threshold=0.1)

        assert len(groups) == 1
        assert len(groups[0]) == 1

    def test_group_by_similarity_all_similar(self, sample_results):
        """Test grouping when all results are similar."""
        groups = group_by_similarity(sample_results, threshold=0.5)

        # With high threshold, all should be in one group
        assert len(groups) == 1
        assert len(groups[0]) == len(sample_results)


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
