"""Tests for DataFrame integration functionality."""

import pytest
from arvak_grpc.types import JobResult
from arvak_grpc.dataframe_integration import (
    DataFrameConverter,
    StatisticalAnalyzer,
    to_pandas,
    to_polars,
    batch_to_pandas,
    batch_to_polars,
)


@pytest.fixture
def bell_result():
    """Sample Bell state result."""
    return JobResult(
        job_id="test-bell-123",
        counts={"00": 505, "11": 495},
        shots=1000,
        execution_time_ms=42,
    )


@pytest.fixture
def ghz_result():
    """Sample GHZ state result."""
    return JobResult(
        job_id="test-ghz-456",
        counts={"000": 512, "111": 488},
        shots=1000,
        execution_time_ms=55,
    )


@pytest.fixture
def noisy_result():
    """Sample noisy result with multiple states."""
    return JobResult(
        job_id="test-noisy-789",
        counts={
            "00": 450,
            "01": 50,
            "10": 30,
            "11": 470,
        },
        shots=1000,
        execution_time_ms=38,
    )


class TestPandasConversion:
    """Test pandas DataFrame conversion."""

    def test_basic_conversion(self, bell_result):
        """Test basic conversion to pandas."""
        pytest.importorskip("pandas")

        df = to_pandas(bell_result)

        assert len(df) == 2
        assert list(df.columns) == ['bitstring', 'count', 'probability']
        assert df['count'].sum() == 1000
        assert abs(df['probability'].sum() - 1.0) < 1e-10

    def test_with_metadata(self, bell_result):
        """Test conversion with metadata columns."""
        pytest.importorskip("pandas")

        df = to_pandas(bell_result, include_metadata=True)

        assert len(df) == 2
        assert 'job_id' in df.columns
        assert 'shots' in df.columns
        assert 'execution_time_ms' in df.columns
        assert df['job_id'].iloc[0] == "test-bell-123"
        assert df['shots'].iloc[0] == 1000

    def test_sorted_bitstrings(self, noisy_result):
        """Test that bitstrings are sorted."""
        pytest.importorskip("pandas")

        df = to_pandas(noisy_result)

        bitstrings = df['bitstring'].tolist()
        assert bitstrings == sorted(bitstrings)

    def test_probability_calculation(self, bell_result):
        """Test probability calculation."""
        pytest.importorskip("pandas")

        df = to_pandas(bell_result)

        for _, row in df.iterrows():
            expected_prob = row['count'] / 1000
            assert abs(row['probability'] - expected_prob) < 1e-10


class TestPolarsConversion:
    """Test polars DataFrame conversion."""

    def test_basic_conversion(self, bell_result):
        """Test basic conversion to polars."""
        pytest.importorskip("polars")

        df = to_polars(bell_result)

        assert len(df) == 2
        assert df.columns == ['bitstring', 'count', 'probability']
        assert df['count'].sum() == 1000
        assert abs(df['probability'].sum() - 1.0) < 1e-10

    def test_with_metadata(self, ghz_result):
        """Test conversion with metadata."""
        pytest.importorskip("polars")

        df = to_polars(ghz_result, include_metadata=True)

        assert 'job_id' in df.columns
        assert 'shots' in df.columns
        assert df['job_id'][0] == "test-ghz-456"
        assert df['shots'][0] == 1000

    def test_filtering(self, noisy_result):
        """Test polars filtering operations."""
        pytest.importorskip("polars")

        df = to_polars(noisy_result)
        high_count = df.filter(df['count'] > 400)

        assert len(high_count) == 2  # Only 00 and 11


class TestBatchConversion:
    """Test batch DataFrame conversion."""

    def test_batch_pandas(self, bell_result, ghz_result):
        """Test batch conversion to pandas."""
        pytest.importorskip("pandas")

        results = [bell_result, ghz_result]
        df = batch_to_pandas(results)

        # Should have rows for both results
        assert len(df) == 4  # 2 states each
        assert set(df['job_id'].unique()) == {"test-bell-123", "test-ghz-456"}

    def test_batch_polars(self, bell_result, noisy_result):
        """Test batch conversion to polars."""
        pytest.importorskip("polars")

        results = [bell_result, noisy_result]
        df = batch_to_polars(results)

        # Should have rows for both results
        assert len(df) == 6  # 2 + 4 states
        assert len(df['job_id'].unique()) == 2

    def test_batch_grouping(self, bell_result):
        """Test grouping operations on batch data."""
        pytest.importorskip("pandas")

        # Simulate multiple runs of same circuit
        results = [
            JobResult(f"job-{i}", {"00": 500 + i*10, "11": 500 - i*10}, 1000)
            for i in range(5)
        ]

        df = batch_to_pandas(results)
        grouped = df.groupby('bitstring')['count'].mean()

        assert len(grouped) == 2
        assert '00' in grouped.index
        assert '11' in grouped.index


class TestStatisticalAnalyzer:
    """Test statistical analysis functions."""

    def test_entropy_uniform(self):
        """Test entropy for uniform distribution."""
        # 2-qubit uniform distribution
        result = JobResult(
            "test-uniform",
            {"00": 250, "01": 250, "10": 250, "11": 250},
            1000
        )

        entropy = StatisticalAnalyzer.entropy(result)
        max_entropy = StatisticalAnalyzer.max_entropy(2)

        assert abs(entropy - max_entropy) < 1e-10  # Should be maximal
        assert abs(entropy - 2.0) < 1e-10  # 2 qubits = 2 bits max

    def test_entropy_deterministic(self):
        """Test entropy for deterministic state."""
        result = JobResult("test-det", {"00": 1000}, 1000)

        entropy = StatisticalAnalyzer.entropy(result)

        assert abs(entropy - 0.0) < 1e-10  # Should be zero

    def test_purity_pure_state(self):
        """Test purity for pure state."""
        result = JobResult("test-pure", {"00": 1000}, 1000)

        purity = StatisticalAnalyzer.purity(result)

        assert abs(purity - 1.0) < 1e-10  # Should be 1

    def test_purity_mixed_state(self):
        """Test purity for mixed state."""
        result = JobResult(
            "test-mixed",
            {"00": 250, "01": 250, "10": 250, "11": 250},
            1000
        )

        purity = StatisticalAnalyzer.purity(result)

        assert abs(purity - 0.25) < 1e-10  # 1/4 for 2 qubits

    def test_fidelity_perfect(self, bell_result):
        """Test fidelity with perfect target."""
        ideal = {"00": 0.5, "11": 0.5}

        fidelity = StatisticalAnalyzer.fidelity_estimate(bell_result, ideal)

        # Should be very close to 1.0 (accounting for sampling noise)
        assert fidelity > 0.99

    def test_fidelity_orthogonal(self):
        """Test fidelity with orthogonal state."""
        result = JobResult("test", {"00": 1000}, 1000)
        ideal = {"11": 1.0}  # Orthogonal state

        fidelity = StatisticalAnalyzer.fidelity_estimate(result, ideal)

        assert abs(fidelity - 0.0) < 1e-10

    def test_tvd_identical(self, bell_result):
        """Test TVD between identical distributions."""
        tvd = StatisticalAnalyzer.total_variation_distance(bell_result, bell_result)

        assert abs(tvd - 0.0) < 1e-10

    def test_tvd_orthogonal(self):
        """Test TVD between orthogonal distributions."""
        result1 = JobResult("test1", {"00": 1000}, 1000)
        result2 = JobResult("test2", {"11": 1000}, 1000)

        tvd = StatisticalAnalyzer.total_variation_distance(result1, result2)

        assert abs(tvd - 1.0) < 1e-10

    def test_summary_statistics(self, bell_result):
        """Test summary statistics computation."""
        stats = StatisticalAnalyzer.summary_statistics(bell_result)

        assert stats['total_shots'] == 1000
        assert stats['unique_states'] == 2
        assert stats['num_qubits'] == 2
        assert stats['most_common_state'] == '00'
        assert stats['most_common_count'] == 505
        assert 'entropy' in stats
        assert 'purity' in stats

    def test_empty_result_handling(self):
        """Test handling of empty results."""
        empty_result = JobResult("empty", {}, 0)

        assert StatisticalAnalyzer.entropy(empty_result) == 0.0
        assert StatisticalAnalyzer.purity(empty_result) == 0.0
        stats = StatisticalAnalyzer.summary_statistics(empty_result)
        assert stats['total_shots'] == 0


class TestVisualization:
    """Test visualization functions (basic checks only)."""

    def test_plot_distribution_basic(self, bell_result):
        """Test that plot_distribution doesn't crash."""
        pytest.importorskip("matplotlib")
        from arvak_grpc.dataframe_integration import Visualizer

        fig, axes = Visualizer.plot_distribution(bell_result)

        # Basic check: should return figure and 2 axes
        assert fig is not None
        assert len(axes) == 2

        # Cleanup
        import matplotlib.pyplot as plt
        plt.close(fig)

    def test_plot_comparison_basic(self, bell_result, ghz_result):
        """Test that plot_comparison doesn't crash."""
        pytest.importorskip("matplotlib")
        from arvak_grpc.dataframe_integration import Visualizer

        fig, ax = Visualizer.plot_comparison([bell_result, ghz_result])

        assert fig is not None
        assert ax is not None

        import matplotlib.pyplot as plt
        plt.close(fig)

    def test_plot_statistics_table_basic(self, bell_result):
        """Test that plot_statistics_table doesn't crash."""
        pytest.importorskip("matplotlib")
        from arvak_grpc.dataframe_integration import Visualizer

        fig, ax = Visualizer.plot_statistics_table(bell_result)

        assert fig is not None
        assert ax is not None

        import matplotlib.pyplot as plt
        plt.close(fig)


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
