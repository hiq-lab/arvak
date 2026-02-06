"""DataFrame integration for JobResult - Pandas and Polars support."""

from typing import List, Optional, Union, TYPE_CHECKING
import math

if TYPE_CHECKING:
    import pandas as pd
    import polars as pl

from .types import JobResult


class DataFrameConverter:
    """Convert JobResult to pandas or polars DataFrames.

    Provides convenient methods for data analysis and visualization.
    """

    @staticmethod
    def to_pandas(
        result: JobResult,
        include_metadata: bool = False
    ) -> 'pd.DataFrame':
        """Convert JobResult to pandas DataFrame.

        Args:
            result: JobResult to convert
            include_metadata: Include job_id and shots columns

        Returns:
            DataFrame with columns: bitstring, count, probability
            (optionally: job_id, shots)

        Raises:
            ImportError: If pandas is not installed
        """
        try:
            import pandas as pd
        except ImportError:
            raise ImportError(
                "pandas is required for DataFrame conversion. "
                "Install with: pip install pandas"
            )

        total_shots = sum(result.counts.values())

        data = {
            'bitstring': [],
            'count': [],
            'probability': [],
        }

        if include_metadata:
            data['job_id'] = []
            data['shots'] = []
            data['execution_time_ms'] = []

        for bitstring, count in sorted(result.counts.items()):
            data['bitstring'].append(bitstring)
            data['count'].append(count)
            data['probability'].append(count / total_shots if total_shots > 0 else 0.0)

            if include_metadata:
                data['job_id'].append(result.job_id)
                data['shots'].append(result.shots)
                data['execution_time_ms'].append(result.execution_time_ms or 0)

        return pd.DataFrame(data)

    @staticmethod
    def to_polars(
        result: JobResult,
        include_metadata: bool = False
    ) -> 'pl.DataFrame':
        """Convert JobResult to polars DataFrame.

        Args:
            result: JobResult to convert
            include_metadata: Include job_id and shots columns

        Returns:
            DataFrame with columns: bitstring, count, probability
            (optionally: job_id, shots)

        Raises:
            ImportError: If polars is not installed
        """
        try:
            import polars as pl
        except ImportError:
            raise ImportError(
                "polars is required for DataFrame conversion. "
                "Install with: pip install polars"
            )

        total_shots = sum(result.counts.values())

        data = {
            'bitstring': [],
            'count': [],
            'probability': [],
        }

        if include_metadata:
            data['job_id'] = []
            data['shots'] = []
            data['execution_time_ms'] = []

        for bitstring, count in sorted(result.counts.items()):
            data['bitstring'].append(bitstring)
            data['count'].append(count)
            data['probability'].append(count / total_shots if total_shots > 0 else 0.0)

            if include_metadata:
                data['job_id'].append(result.job_id)
                data['shots'].append(result.shots)
                data['execution_time_ms'].append(result.execution_time_ms or 0)

        return pl.DataFrame(data)

    @staticmethod
    def batch_to_pandas(
        results: List[JobResult],
        include_metadata: bool = True
    ) -> 'pd.DataFrame':
        """Convert multiple JobResults to a single pandas DataFrame.

        Args:
            results: List of JobResults
            include_metadata: Include job_id and shots columns

        Returns:
            Combined DataFrame with all results
        """
        try:
            import pandas as pd
        except ImportError:
            raise ImportError("pandas is required")

        dfs = [DataFrameConverter.to_pandas(r, include_metadata) for r in results]
        return pd.concat(dfs, ignore_index=True)

    @staticmethod
    def batch_to_polars(
        results: List[JobResult],
        include_metadata: bool = True
    ) -> 'pl.DataFrame':
        """Convert multiple JobResults to a single polars DataFrame.

        Args:
            results: List of JobResults
            include_metadata: Include job_id and shots columns

        Returns:
            Combined DataFrame with all results
        """
        try:
            import polars as pl
        except ImportError:
            raise ImportError("polars is required")

        dfs = [DataFrameConverter.to_polars(r, include_metadata) for r in results]
        return pl.concat(dfs)


class StatisticalAnalyzer:
    """Statistical analysis tools for quantum measurement results."""

    @staticmethod
    def entropy(result: JobResult) -> float:
        """Calculate Shannon entropy of measurement distribution.

        H = -Σ p(x) log₂(p(x))

        Args:
            result: JobResult to analyze

        Returns:
            Shannon entropy in bits (0 = deterministic, log₂(n) = uniform)
        """
        total_shots = sum(result.counts.values())
        if total_shots == 0:
            return 0.0

        entropy = 0.0
        for count in result.counts.values():
            if count > 0:
                p = count / total_shots
                entropy -= p * math.log2(p)

        return entropy

    @staticmethod
    def max_entropy(num_qubits: int) -> float:
        """Maximum possible entropy for n qubits.

        Args:
            num_qubits: Number of qubits

        Returns:
            Maximum entropy = log₂(2^n) = n bits
        """
        return float(num_qubits)

    @staticmethod
    def purity(result: JobResult) -> float:
        """Calculate purity of measurement distribution.

        P = Σ p(x)²

        Args:
            result: JobResult to analyze

        Returns:
            Purity (1/2^n = maximally mixed, 1 = pure state)
        """
        total_shots = sum(result.counts.values())
        if total_shots == 0:
            return 0.0

        purity_val = 0.0
        for count in result.counts.values():
            p = count / total_shots
            purity_val += p * p

        return purity_val

    @staticmethod
    def fidelity_estimate(result: JobResult, target_state: dict) -> float:
        """Estimate fidelity with target state from measurement counts.

        For diagonal (classical) states, computes:
        F = (Σ √(p_target · p_measured))²

        Args:
            result: Measured JobResult
            target_state: Dict mapping bitstrings to probabilities

        Returns:
            Estimated fidelity (0 to 1)
        """
        total_shots = sum(result.counts.values())
        if total_shots == 0:
            return 0.0

        # Get all bitstrings from both distributions
        all_bitstrings = set(result.counts.keys()) | set(target_state.keys())

        fidelity_sum = 0.0
        for bitstring in all_bitstrings:
            p_measured = result.counts.get(bitstring, 0) / total_shots
            p_target = target_state.get(bitstring, 0.0)
            fidelity_sum += math.sqrt(p_target * p_measured)

        # Square the sum for Bhattacharyya coefficient (classical fidelity)
        return fidelity_sum ** 2

    @staticmethod
    def total_variation_distance(result1: JobResult, result2: JobResult) -> float:
        """Calculate total variation distance between two distributions.

        TVD = 0.5 * Σ |p₁(x) - p₂(x)|

        Args:
            result1: First JobResult
            result2: Second JobResult

        Returns:
            Total variation distance (0 = identical, 1 = orthogonal)
        """
        total1 = sum(result1.counts.values())
        total2 = sum(result2.counts.values())

        if total1 == 0 or total2 == 0:
            return 1.0

        # Get all unique bitstrings
        all_bitstrings = set(result1.counts.keys()) | set(result2.counts.keys())

        tvd = 0.0
        for bitstring in all_bitstrings:
            p1 = result1.counts.get(bitstring, 0) / total1
            p2 = result2.counts.get(bitstring, 0) / total2
            tvd += abs(p1 - p2)

        return tvd / 2.0

    @staticmethod
    def summary_statistics(result: JobResult) -> dict:
        """Compute summary statistics for a JobResult.

        Args:
            result: JobResult to analyze

        Returns:
            Dictionary with various statistical measures
        """
        total_shots = sum(result.counts.values())
        num_unique = len(result.counts)

        # Infer number of qubits from bitstring length
        num_qubits = len(next(iter(result.counts.keys()))) if result.counts else 0

        return {
            'total_shots': total_shots,
            'unique_states': num_unique,
            'num_qubits': num_qubits,
            'entropy': StatisticalAnalyzer.entropy(result),
            'max_entropy': StatisticalAnalyzer.max_entropy(num_qubits),
            'purity': StatisticalAnalyzer.purity(result),
            'most_common_state': max(result.counts, key=result.counts.get) if result.counts else None,
            'most_common_count': max(result.counts.values()) if result.counts else 0,
        }


class Visualizer:
    """Visualization helpers for quantum measurement results."""

    @staticmethod
    def plot_distribution(
        result: JobResult,
        max_states: int = 20,
        figsize: tuple = (12, 6),
        title: Optional[str] = None
    ):
        """Plot measurement distribution as bar chart.

        Args:
            result: JobResult to plot
            max_states: Maximum number of states to show (most common)
            figsize: Figure size (width, height)
            title: Custom title for plot

        Returns:
            matplotlib figure and axes

        Raises:
            ImportError: If matplotlib is not installed
        """
        try:
            import matplotlib.pyplot as plt
        except ImportError:
            raise ImportError(
                "matplotlib is required for visualization. "
                "Install with: pip install matplotlib"
            )

        # Get top N states by count
        sorted_counts = sorted(result.counts.items(), key=lambda x: x[1], reverse=True)
        top_states = sorted_counts[:max_states]

        bitstrings = [bs for bs, _ in top_states]
        counts = [c for _, c in top_states]
        total = sum(result.counts.values())
        probabilities = [c / total for c in counts]

        fig, (ax1, ax2) = plt.subplots(1, 2, figsize=figsize)

        # Plot counts
        ax1.bar(range(len(bitstrings)), counts)
        ax1.set_xticks(range(len(bitstrings)))
        ax1.set_xticklabels(bitstrings, rotation=90)
        ax1.set_xlabel('Bitstring')
        ax1.set_ylabel('Count')
        ax1.set_title(f'Measurement Counts (Job: {result.job_id[:8]}...)')
        ax1.grid(axis='y', alpha=0.3)

        # Plot probabilities
        ax2.bar(range(len(bitstrings)), probabilities)
        ax2.set_xticks(range(len(bitstrings)))
        ax2.set_xticklabels(bitstrings, rotation=90)
        ax2.set_xlabel('Bitstring')
        ax2.set_ylabel('Probability')
        ax2.set_title('Measurement Probabilities')
        ax2.grid(axis='y', alpha=0.3)

        if title:
            fig.suptitle(title, fontsize=14, y=1.02)

        plt.tight_layout()
        return fig, (ax1, ax2)

    @staticmethod
    def plot_comparison(
        results: List[JobResult],
        labels: Optional[List[str]] = None,
        figsize: tuple = (14, 6),
        title: Optional[str] = None
    ):
        """Compare multiple measurement distributions.

        Args:
            results: List of JobResults to compare
            labels: Labels for each result (default: job IDs)
            figsize: Figure size (width, height)
            title: Custom title for plot

        Returns:
            matplotlib figure and axes
        """
        try:
            import matplotlib.pyplot as plt
            import numpy as np
        except ImportError:
            raise ImportError("matplotlib and numpy required")

        if labels is None:
            labels = [r.job_id[:8] for r in results]

        # Get all unique bitstrings across all results
        all_bitstrings = set()
        for result in results:
            all_bitstrings.update(result.counts.keys())
        all_bitstrings = sorted(all_bitstrings)

        # Compute probabilities for each result
        prob_matrix = []
        for result in results:
            total = sum(result.counts.values())
            probs = [result.counts.get(bs, 0) / total for bs in all_bitstrings]
            prob_matrix.append(probs)

        # Create grouped bar chart
        fig, ax = plt.subplots(figsize=figsize)

        x = np.arange(len(all_bitstrings))
        width = 0.8 / len(results)

        for i, (probs, label) in enumerate(zip(prob_matrix, labels)):
            offset = (i - len(results)/2 + 0.5) * width
            ax.bar(x + offset, probs, width, label=label)

        ax.set_xlabel('Bitstring')
        ax.set_ylabel('Probability')
        ax.set_title(title or 'Distribution Comparison')
        ax.set_xticks(x)
        ax.set_xticklabels(all_bitstrings, rotation=90)
        ax.legend()
        ax.grid(axis='y', alpha=0.3)

        plt.tight_layout()
        return fig, ax

    @staticmethod
    def plot_statistics_table(result: JobResult, figsize: tuple = (8, 4)):
        """Display summary statistics as a table plot.

        Args:
            result: JobResult to analyze
            figsize: Figure size

        Returns:
            matplotlib figure and axes
        """
        try:
            import matplotlib.pyplot as plt
        except ImportError:
            raise ImportError("matplotlib required")

        stats = StatisticalAnalyzer.summary_statistics(result)

        fig, ax = plt.subplots(figsize=figsize)
        ax.axis('off')

        # Format statistics for display
        table_data = [
            ['Metric', 'Value'],
            ['Total Shots', f"{stats['total_shots']:,}"],
            ['Unique States', f"{stats['unique_states']:,}"],
            ['Number of Qubits', str(stats['num_qubits'])],
            ['Shannon Entropy', f"{stats['entropy']:.4f} bits"],
            ['Max Entropy', f"{stats['max_entropy']:.4f} bits"],
            ['Purity', f"{stats['purity']:.6f}"],
            ['Most Common State', stats['most_common_state'] or 'N/A'],
            ['Most Common Count', f"{stats['most_common_count']:,}"],
        ]

        table = ax.table(cellText=table_data, cellLoc='left', loc='center',
                        colWidths=[0.5, 0.5])
        table.auto_set_font_size(False)
        table.set_fontsize(10)
        table.scale(1, 2)

        # Style header row
        for i in range(2):
            table[(0, i)].set_facecolor('#4CAF50')
            table[(0, i)].set_text_props(weight='bold', color='white')

        plt.title(f'Summary Statistics - Job {result.job_id[:12]}...',
                 fontsize=12, pad=20)

        return fig, ax


# Convenience functions that can be imported directly
def to_pandas(result: JobResult, include_metadata: bool = False) -> 'pd.DataFrame':
    """Convert JobResult to pandas DataFrame."""
    return DataFrameConverter.to_pandas(result, include_metadata)


def to_polars(result: JobResult, include_metadata: bool = False) -> 'pl.DataFrame':
    """Convert JobResult to polars DataFrame."""
    return DataFrameConverter.to_polars(result, include_metadata)


def batch_to_pandas(results: List[JobResult], include_metadata: bool = True) -> 'pd.DataFrame':
    """Convert multiple JobResults to pandas DataFrame."""
    return DataFrameConverter.batch_to_pandas(results, include_metadata)


def batch_to_polars(results: List[JobResult], include_metadata: bool = True) -> 'pl.DataFrame':
    """Convert multiple JobResults to polars DataFrame."""
    return DataFrameConverter.batch_to_polars(results, include_metadata)
