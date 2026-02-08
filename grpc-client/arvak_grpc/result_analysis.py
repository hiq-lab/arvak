"""Advanced result aggregation and analysis tools."""

import math
from typing import List, Dict, Optional, Tuple, Callable
from collections import defaultdict
from dataclasses import dataclass

from .types import JobResult


@dataclass
class AggregatedResult:
    """Aggregated result from multiple job runs."""
    job_ids: List[str]
    counts: Dict[str, int]  # Combined counts
    total_shots: int
    num_jobs: int
    mean_execution_time_ms: Optional[float] = None

    @property
    def probabilities(self) -> Dict[str, float]:
        """Compute probability distribution."""
        if self.total_shots == 0:
            return {}
        return {bs: count / self.total_shots for bs, count in self.counts.items()}


@dataclass
class ComparisonResult:
    """Result of comparing two distributions."""
    tvd: float  # Total variation distance
    kl_divergence: float  # KL divergence (if defined)
    js_divergence: float  # Jensen-Shannon divergence
    hellinger_distance: float  # Hellinger distance
    overlap: float  # Probability overlap
    correlation: float  # Pearson correlation of counts


class ResultAggregator:
    """Aggregate multiple JobResults."""

    @staticmethod
    def combine(results: List[JobResult]) -> AggregatedResult:
        """Combine multiple results by summing counts.

        Args:
            results: List of JobResults to combine

        Returns:
            AggregatedResult with combined counts
        """
        if not results:
            return AggregatedResult(
                job_ids=[],
                counts={},
                total_shots=0,
                num_jobs=0,
            )

        combined_counts = defaultdict(int)
        total_shots = 0
        execution_times = []

        for result in results:
            for bitstring, count in result.counts.items():
                combined_counts[bitstring] += count
            total_shots += result.shots

            if result.execution_time_ms is not None:
                execution_times.append(result.execution_time_ms)

        mean_time = sum(execution_times) / len(execution_times) if execution_times else None

        return AggregatedResult(
            job_ids=[r.job_id for r in results],
            counts=dict(combined_counts),
            total_shots=total_shots,
            num_jobs=len(results),
            mean_execution_time_ms=mean_time,
        )

    @staticmethod
    def average(results: List[JobResult]) -> JobResult:
        """Average multiple results (normalized counts).

        Args:
            results: List of JobResults to average

        Returns:
            JobResult with averaged probability distribution
        """
        if not results:
            return JobResult("averaged", {}, 0)

        # Get all unique bitstrings
        all_bitstrings = set()
        for result in results:
            all_bitstrings.update(result.counts.keys())

        # Compute average probabilities
        avg_probs = {}
        for bitstring in all_bitstrings:
            probs = []
            for result in results:
                total = sum(result.counts.values())
                prob = result.counts.get(bitstring, 0) / total if total > 0 else 0.0
                probs.append(prob)
            avg_probs[bitstring] = sum(probs) / len(probs)

        # Convert back to counts (using first result's shot count as reference)
        ref_shots = results[0].shots
        avg_counts = {
            bs: int(round(prob * ref_shots))
            for bs, prob in avg_probs.items()
            if prob > 0
        }

        # Normalize to exactly ref_shots
        total_counts = sum(avg_counts.values())
        if total_counts != ref_shots and total_counts > 0:
            # Distribute rounding error to most common state
            most_common = max(avg_counts.keys(), key=lambda k: avg_counts[k])
            avg_counts[most_common] += (ref_shots - total_counts)

        return JobResult(
            job_id="averaged",
            counts=avg_counts,
            shots=ref_shots,
        )

    @staticmethod
    def filter_by_threshold(
        result: JobResult,
        min_count: Optional[int] = None,
        min_probability: Optional[float] = None,
    ) -> JobResult:
        """Filter out low-probability states.

        Args:
            result: JobResult to filter
            min_count: Minimum count threshold
            min_probability: Minimum probability threshold

        Returns:
            New JobResult with filtered counts
        """
        total = sum(result.counts.values())
        filtered_counts = {}

        for bitstring, count in result.counts.items():
            if min_count is not None and count < min_count:
                continue

            if min_probability is not None:
                prob = count / total if total > 0 else 0.0
                if prob < min_probability:
                    continue

            filtered_counts[bitstring] = count

        return JobResult(
            job_id=result.job_id,
            counts=filtered_counts,
            shots=result.shots,
            execution_time_ms=result.execution_time_ms,
            metadata=result.metadata,
        )

    @staticmethod
    def top_k_states(result: JobResult, k: int) -> JobResult:
        """Keep only top-k most probable states.

        Args:
            result: JobResult to filter
            k: Number of states to keep

        Returns:
            New JobResult with top k states
        """
        sorted_items = sorted(result.counts.items(), key=lambda x: x[1], reverse=True)
        top_counts = dict(sorted_items[:k])

        return JobResult(
            job_id=result.job_id,
            counts=top_counts,
            shots=result.shots,
            execution_time_ms=result.execution_time_ms,
            metadata=result.metadata,
        )


class ResultComparator:
    """Compare two measurement distributions."""

    @staticmethod
    def compare(result1: JobResult, result2: JobResult) -> ComparisonResult:
        """Comprehensive comparison of two distributions.

        Args:
            result1: First result
            result2: Second result

        Returns:
            ComparisonResult with various distance metrics
        """
        # Get probabilities
        total1 = sum(result1.counts.values())
        total2 = sum(result2.counts.values())

        all_bitstrings = set(result1.counts.keys()) | set(result2.counts.keys())

        probs1 = {bs: result1.counts.get(bs, 0) / total1 if total1 > 0 else 0.0
                  for bs in all_bitstrings}
        probs2 = {bs: result2.counts.get(bs, 0) / total2 if total2 > 0 else 0.0
                  for bs in all_bitstrings}

        # Compute metrics
        tvd = ResultComparator._total_variation_distance(probs1, probs2)
        kl_div = ResultComparator._kl_divergence(probs1, probs2)
        js_div = ResultComparator._js_divergence(probs1, probs2)
        hellinger = ResultComparator._hellinger_distance(probs1, probs2)
        overlap = ResultComparator._overlap(probs1, probs2)
        correlation = ResultComparator._correlation(result1, result2, all_bitstrings)

        return ComparisonResult(
            tvd=tvd,
            kl_divergence=kl_div,
            js_divergence=js_div,
            hellinger_distance=hellinger,
            overlap=overlap,
            correlation=correlation,
        )

    @staticmethod
    def _total_variation_distance(probs1: dict, probs2: dict) -> float:
        """Total variation distance."""
        return 0.5 * sum(abs(probs1[bs] - probs2[bs]) for bs in probs1)

    @staticmethod
    def _kl_divergence(probs1: dict, probs2: dict) -> float:
        """KL divergence D(P||Q)."""
        kl = 0.0
        for bs in probs1:
            p = probs1[bs]
            q = probs2[bs]
            if p > 0:
                if q > 0:
                    kl += p * math.log2(p / q)
                else:
                    return float('inf')  # Undefined
        return kl

    @staticmethod
    def _js_divergence(probs1: dict, probs2: dict) -> float:
        """Jensen-Shannon divergence."""
        # M = (P + Q) / 2
        m_probs = {bs: (probs1[bs] + probs2[bs]) / 2 for bs in probs1}

        kl1 = 0.0
        kl2 = 0.0

        for bs in probs1:
            p = probs1[bs]
            q = probs2[bs]
            m = m_probs[bs]

            if p > 0 and m > 0:
                kl1 += p * math.log2(p / m)
            if q > 0 and m > 0:
                kl2 += q * math.log2(q / m)

        return (kl1 + kl2) / 2

    @staticmethod
    def _hellinger_distance(probs1: dict, probs2: dict) -> float:
        """Hellinger distance."""
        sum_sq_diff = sum((math.sqrt(probs1[bs]) - math.sqrt(probs2[bs])) ** 2
                          for bs in probs1)
        return math.sqrt(sum_sq_diff / 2)

    @staticmethod
    def _overlap(probs1: dict, probs2: dict) -> float:
        """Probability overlap (Bhattacharyya coefficient)."""
        return sum(math.sqrt(probs1[bs] * probs2[bs]) for bs in probs1)

    @staticmethod
    def _correlation(result1: JobResult, result2: JobResult, bitstrings: set) -> float:
        """Pearson correlation of counts."""
        counts1 = [result1.counts.get(bs, 0) for bs in bitstrings]
        counts2 = [result2.counts.get(bs, 0) for bs in bitstrings]

        n = len(counts1)
        if n == 0:
            return 0.0

        mean1 = sum(counts1) / n
        mean2 = sum(counts2) / n

        numerator = sum((c1 - mean1) * (c2 - mean2)
                        for c1, c2 in zip(counts1, counts2))

        var1 = sum((c1 - mean1) ** 2 for c1 in counts1)
        var2 = sum((c2 - mean2) ** 2 for c2 in counts2)

        denominator = math.sqrt(var1 * var2)

        return numerator / denominator if denominator > 0 else 0.0


@dataclass
class ConvergenceAnalysis:
    """Analysis of convergence with shot count."""
    shot_counts: List[int]
    entropies: List[float]
    purities: List[float]
    fidelities: List[float]
    num_unique_states: List[int]
    converged: bool
    convergence_threshold: float


class ConvergenceAnalyzer:
    """Analyze convergence of measurement distributions."""

    @staticmethod
    def analyze_convergence(
        results: List[JobResult],
        target_state: Optional[Dict[str, float]] = None,
        threshold: float = 0.01,
    ) -> ConvergenceAnalysis:
        """Analyze how distributions converge with more shots.

        Args:
            results: List of JobResults with increasing shot counts
            target_state: Optional target distribution for fidelity
            threshold: Convergence threshold for standard deviation

        Returns:
            ConvergenceAnalysis with metrics
        """
        from .dataframe_integration import StatisticalAnalyzer

        shot_counts = [r.shots for r in results]
        entropies = [StatisticalAnalyzer.entropy(r) for r in results]
        purities = [StatisticalAnalyzer.purity(r) for r in results]
        num_states = [len(r.counts) for r in results]

        fidelities = []
        if target_state is not None:
            for result in results:
                fid = StatisticalAnalyzer.fidelity_estimate(result, target_state)
                fidelities.append(fid)

        # Check convergence of entropy (last few values should be stable)
        converged = False
        if len(entropies) >= 3:
            recent_entropies = entropies[-3:]
            std_dev = math.sqrt(sum((e - sum(recent_entropies)/3)**2
                                   for e in recent_entropies) / 3)
            converged = std_dev < threshold

        return ConvergenceAnalysis(
            shot_counts=shot_counts,
            entropies=entropies,
            purities=purities,
            fidelities=fidelities,
            num_unique_states=num_states,
            converged=converged,
            convergence_threshold=threshold,
        )

    @staticmethod
    def estimate_required_shots(
        pilot_results: List[JobResult],
        target_precision: float = 0.01,
    ) -> int:
        """Estimate shots needed for target precision.

        Uses variance of probabilities to estimate required shots.

        Args:
            pilot_results: Small pilot runs
            target_precision: Desired precision (std error of probability)

        Returns:
            Estimated number of shots needed
        """
        # Combine pilot results
        combined = ResultAggregator.combine(pilot_results)
        probs = combined.probabilities

        if not probs:
            return 1000  # Default

        # Find maximum variance state
        max_var = max(p * (1 - p) for p in probs.values())

        # Required shots: n = variance / precision^2
        required_shots = int(math.ceil(max_var / (target_precision ** 2)))

        return max(required_shots, 100)  # At least 100 shots


class ResultTransformer:
    """Transform JobResults in various ways."""

    @staticmethod
    def normalize(result: JobResult) -> JobResult:
        """Normalize counts to match shots exactly.

        Args:
            result: JobResult to normalize

        Returns:
            Normalized JobResult
        """
        total = sum(result.counts.values())

        if total == result.shots:
            return result  # Already normalized

        if total == 0:
            return result

        # Scale and round
        scale = result.shots / total
        normalized_counts = {
            bs: int(round(count * scale))
            for bs, count in result.counts.items()
        }

        # Fix rounding errors
        new_total = sum(normalized_counts.values())
        if new_total != result.shots:
            # Add/subtract difference to most common state
            most_common = max(normalized_counts.keys(),
                            key=lambda k: normalized_counts[k])
            normalized_counts[most_common] += (result.shots - new_total)

        return JobResult(
            job_id=result.job_id,
            counts=normalized_counts,
            shots=result.shots,
            execution_time_ms=result.execution_time_ms,
            metadata=result.metadata,
        )

    @staticmethod
    def downsample(result: JobResult, target_shots: int) -> JobResult:
        """Downsample result to fewer shots.

        Args:
            result: JobResult to downsample
            target_shots: Target number of shots

        Returns:
            Downsampled JobResult
        """
        if target_shots >= result.shots:
            return result

        # Scale counts proportionally
        scale = target_shots / result.shots
        downsampled_counts = {
            bs: int(round(count * scale))
            for bs, count in result.counts.items()
            if count * scale >= 0.5  # Drop very small counts
        }

        # Normalize to exact target_shots
        return ResultTransformer.normalize(
            JobResult(
                job_id=result.job_id,
                counts=downsampled_counts,
                shots=target_shots,
                execution_time_ms=result.execution_time_ms,
                metadata=result.metadata,
            )
        )

    @staticmethod
    def apply_noise(
        result: JobResult,
        error_rate: float,
        seed: Optional[int] = None,
    ) -> JobResult:
        """Apply simulated noise to result.

        Args:
            result: JobResult to add noise to
            error_rate: Bit flip error rate (0.0 to 1.0)
            seed: Random seed for reproducibility

        Returns:
            Noisy JobResult
        """
        import random
        if seed is not None:
            random.seed(seed)

        num_qubits = len(next(iter(result.counts.keys()))) if result.counts else 0

        noisy_counts = defaultdict(int)

        for bitstring, count in result.counts.items():
            for _ in range(count):
                # Apply bit flips with error_rate
                noisy_bits = list(bitstring)
                for i in range(num_qubits):
                    if random.random() < error_rate:
                        noisy_bits[i] = '1' if noisy_bits[i] == '0' else '0'

                noisy_bitstring = ''.join(noisy_bits)
                noisy_counts[noisy_bitstring] += 1

        return JobResult(
            job_id=result.job_id + "_noisy",
            counts=dict(noisy_counts),
            shots=result.shots,
            metadata={'noise': error_rate, **( result.metadata or {})},
        )


def batch_compare(results: List[JobResult]) -> Dict[Tuple[int, int], ComparisonResult]:
    """Pairwise comparison of all results.

    Args:
        results: List of JobResults to compare

    Returns:
        Dictionary mapping (i, j) to ComparisonResult
    """
    comparisons = {}

    for i in range(len(results)):
        for j in range(i + 1, len(results)):
            comp = ResultComparator.compare(results[i], results[j])
            comparisons[(i, j)] = comp

    return comparisons


def group_by_similarity(
    results: List[JobResult],
    threshold: float = 0.1,
) -> List[List[JobResult]]:
    """Group results by similarity.

    Args:
        results: List of JobResults to group
        threshold: Maximum TVD for grouping

    Returns:
        List of groups (each group is a list of similar results)
    """
    groups = []

    for result in results:
        # Find matching group
        added = False
        for group in groups:
            # Compare with first result in group
            comp = ResultComparator.compare(result, group[0])
            if comp.tvd < threshold:
                group.append(result)
                added = True
                break

        if not added:
            # Create new group
            groups.append([result])

    return groups
