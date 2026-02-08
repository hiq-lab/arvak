#!/usr/bin/env python3
"""Example: Advanced result aggregation and analysis."""

from arvak_grpc import (
    ArvakClient,
    ResultAggregator,
    ResultComparator,
    ConvergenceAnalyzer,
    ResultTransformer,
    batch_compare,
    group_by_similarity,
)

BELL_STATE = """
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
"""

GHZ_STATE = """
OPENQASM 3.0;
qubit[3] q;
h q[0];
cx q[0], q[1];
cx q[1], q[2];
"""


def example_result_aggregation():
    """Aggregate multiple results."""
    print("=" * 60)
    print("Example 1: Result Aggregation")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Run same circuit multiple times
    print("\nRunning Bell state 5 times...")
    job_ids = []
    for i in range(5):
        job_id = client.submit_qasm(BELL_STATE, "simulator", shots=500)
        job_ids.append(job_id)

    results = [client.wait_for_job(job_id) for job_id in job_ids]
    print(f"Got {len(results)} results")

    # Combine all results
    print("\nCombining results...")
    combined = ResultAggregator.combine(results)
    print(f"  Total shots: {combined.total_shots}")
    print(f"  Unique states: {len(combined.counts)}")
    print(f"  Combined counts: {combined.counts}")

    # Average results
    print("\nAveraging results...")
    averaged = ResultAggregator.average(results)
    print(f"  Averaged counts: {averaged.counts}")

    # Filter low-probability states
    print("\nFiltering states (min_probability=0.01)...")
    filtered = ResultAggregator.filter_by_threshold(
        averaged,
        min_probability=0.01
    )
    print(f"  Filtered to {len(filtered.counts)} states")

    # Top-k states
    print("\nTop 2 states:")
    top2 = ResultAggregator.top_k_states(averaged, k=2)
    for bitstring, count in sorted(top2.counts.items(),
                                   key=lambda x: x[1], reverse=True):
        prob = count / top2.shots
        print(f"  {bitstring}: {count} ({prob:.4f})")

    client.close()


def example_result_comparison():
    """Compare different measurement distributions."""
    print("\n" + "=" * 60)
    print("Example 2: Result Comparison")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Run two different circuits
    print("\nSubmitting Bell state...")
    bell_id = client.submit_qasm(BELL_STATE, "simulator", shots=2000)
    bell_result = client.wait_for_job(bell_id)

    print("Submitting GHZ state...")
    ghz_id = client.submit_qasm(GHZ_STATE, "simulator", shots=2000)
    ghz_result = client.wait_for_job(ghz_id)

    # Compare distributions
    print("\nComparing distributions...")
    comparison = ResultComparator.compare(bell_result, ghz_result)

    print(f"  Total Variation Distance: {comparison.tvd:.6f}")
    print(f"  KL Divergence: {comparison.kl_divergence:.6f}")
    print(f"  JS Divergence: {comparison.js_divergence:.6f}")
    print(f"  Hellinger Distance: {comparison.hellinger_distance:.6f}")
    print(f"  Overlap: {comparison.overlap:.6f}")
    print(f"  Correlation: {comparison.correlation:.6f}")

    # Compare two runs of same circuit
    print("\nComparing two Bell state runs...")
    bell_id2 = client.submit_qasm(BELL_STATE, "simulator", shots=2000)
    bell_result2 = client.wait_for_job(bell_id2)

    comparison2 = ResultComparator.compare(bell_result, bell_result2)
    print(f"  TVD (should be small): {comparison2.tvd:.6f}")
    print(f"  Overlap (should be ~1): {comparison2.overlap:.6f}")

    client.close()


def example_convergence_analysis():
    """Analyze convergence with increasing shots."""
    print("\n" + "=" * 60)
    print("Example 3: Convergence Analysis")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Run with increasing shot counts
    shot_counts = [100, 200, 500, 1000, 2000, 5000]
    print(f"\nRunning Bell state with shots: {shot_counts}")

    results = []
    for shots in shot_counts:
        job_id = client.submit_qasm(BELL_STATE, "simulator", shots=shots)
        result = client.wait_for_job(job_id)
        results.append(result)
        print(f"  {shots} shots: {len(result.counts)} unique states")

    # Analyze convergence
    print("\nConvergence analysis:")
    ideal = {"00": 0.5, "11": 0.5}
    analysis = ConvergenceAnalyzer.analyze_convergence(
        results,
        target_state=ideal,
        threshold=0.01,
    )

    print(f"  Converged: {analysis.converged}")
    print("\n  Shot count | Entropy | Purity | Fidelity | States")
    print("  " + "-" * 55)
    for i in range(len(analysis.shot_counts)):
        shots = analysis.shot_counts[i]
        entropy = analysis.entropies[i]
        purity = analysis.purities[i]
        fidelity = analysis.fidelities[i] if analysis.fidelities else 0.0
        states = analysis.num_unique_states[i]
        print(f"  {shots:5d}      | {entropy:.4f}  | {purity:.4f} | {fidelity:.6f} | {states:3d}")

    # Estimate required shots
    print("\nEstimating required shots...")
    pilot_results = results[:3]  # Use first 3 as pilot
    required = ConvergenceAnalyzer.estimate_required_shots(
        pilot_results,
        target_precision=0.01,
    )
    print(f"  Estimated shots for 0.01 precision: {required}")

    client.close()


def example_result_transformations():
    """Transform results in various ways."""
    print("\n" + "=" * 60)
    print("Example 4: Result Transformations")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Get a result
    print("\nSubmitting job...")
    job_id = client.submit_qasm(BELL_STATE, "simulator", shots=1000)
    result = client.wait_for_job(job_id)

    print(f"Original result:")
    print(f"  Shots: {result.shots}")
    print(f"  States: {len(result.counts)}")
    print(f"  Counts: {result.counts}")

    # Downsample
    print("\nDownsampling to 100 shots...")
    downsampled = ResultTransformer.downsample(result, target_shots=100)
    print(f"  Downsampled counts: {downsampled.counts}")
    print(f"  Total: {sum(downsampled.counts.values())}")

    # Apply noise
    print("\nApplying 5% noise...")
    noisy = ResultTransformer.apply_noise(result, error_rate=0.05, seed=42)
    print(f"  Noisy states: {len(noisy.counts)}")
    print(f"  Noisy counts: {noisy.counts}")

    # Compare noisy vs clean
    comparison = ResultComparator.compare(result, noisy)
    print(f"  TVD (clean vs noisy): {comparison.tvd:.6f}")

    # Normalize (if counts don't sum to shots)
    print("\nNormalizing result...")
    normalized = ResultTransformer.normalize(result)
    print(f"  Normalized total: {sum(normalized.counts.values())}")

    client.close()


def example_batch_comparison():
    """Pairwise comparison of multiple results."""
    print("\n" + "=" * 60)
    print("Example 5: Batch Comparison")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Submit multiple jobs
    print("\nSubmitting 5 Bell state jobs...")
    job_ids = []
    for i in range(5):
        job_id = client.submit_qasm(BELL_STATE, "simulator", shots=1000)
        job_ids.append(job_id)

    results = [client.wait_for_job(job_id) for job_id in job_ids]

    # Pairwise comparison
    print("\nPairwise comparisons:")
    comparisons = batch_compare(results)

    for (i, j), comp in comparisons.items():
        print(f"  Job {i} vs Job {j}:")
        print(f"    TVD: {comp.tvd:.6f}")
        print(f"    Overlap: {comp.overlap:.6f}")

    # Average TVD
    avg_tvd = sum(comp.tvd for comp in comparisons.values()) / len(comparisons)
    print(f"\nAverage TVD: {avg_tvd:.6f} (lower = more consistent)")

    client.close()


def example_similarity_grouping():
    """Group results by similarity."""
    print("\n" + "=" * 60)
    print("Example 6: Similarity Grouping")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Run multiple circuits (mix of Bell and GHZ)
    print("\nSubmitting mixed jobs...")
    results = []

    # 3 Bell states
    for _ in range(3):
        job_id = client.submit_qasm(BELL_STATE, "simulator", shots=1000)
        result = client.wait_for_job(job_id)
        results.append(result)

    # 2 GHZ states
    for _ in range(2):
        job_id = client.submit_qasm(GHZ_STATE, "simulator", shots=1000)
        result = client.wait_for_job(job_id)
        results.append(result)

    print(f"Got {len(results)} total results")

    # Group by similarity
    print("\nGrouping by similarity (threshold=0.1)...")
    groups = group_by_similarity(results, threshold=0.1)

    print(f"Found {len(groups)} groups:")
    for i, group in enumerate(groups, 1):
        print(f"  Group {i}: {len(group)} results")
        print(f"    Job IDs: {[r.job_id[:12] + '...' for r in group]}")

    client.close()


def example_statistical_workflow():
    """Complete statistical analysis workflow."""
    print("\n" + "=" * 60)
    print("Example 7: Complete Statistical Workflow")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # 1. Run multiple experiments
    print("\nStep 1: Running 10 experiments...")
    job_ids = []
    for i in range(10):
        job_id = client.submit_qasm(BELL_STATE, "simulator", shots=1000)
        job_ids.append(job_id)

    results = [client.wait_for_job(job_id) for job_id in job_ids]

    # 2. Aggregate
    print("\nStep 2: Aggregating results...")
    combined = ResultAggregator.combine(results)
    print(f"  Total shots: {combined.total_shots}")

    # 3. Compute statistics
    print("\nStep 3: Computing statistics...")
    from arvak_grpc.dataframe_integration import StatisticalAnalyzer

    entropy = StatisticalAnalyzer.entropy(
        ResultAggregator.average(results)
    )
    purity = StatisticalAnalyzer.purity(
        ResultAggregator.average(results)
    )
    print(f"  Entropy: {entropy:.4f} bits")
    print(f"  Purity: {purity:.6f}")

    # 4. Check consistency
    print("\nStep 4: Checking consistency...")
    comparisons = batch_compare(results)
    tvds = [comp.tvd for comp in comparisons.values()]
    avg_tvd = sum(tvds) / len(tvds)
    max_tvd = max(tvds)
    print(f"  Average TVD: {avg_tvd:.6f}")
    print(f"  Maximum TVD: {max_tvd:.6f}")
    print(f"  Consistency: {'Good' if max_tvd < 0.05 else 'Poor'}")

    # 5. Identify outliers
    print("\nStep 5: Identifying outliers...")
    averaged = ResultAggregator.average(results)
    for i, result in enumerate(results):
        comp = ResultComparator.compare(result, averaged)
        if comp.tvd > avg_tvd * 2:
            print(f"  Outlier: Job {i} (TVD={comp.tvd:.6f})")

    client.close()


if __name__ == "__main__":
    print("Arvak Result Analysis Examples")
    print()

    example_result_aggregation()
    example_result_comparison()
    example_convergence_analysis()
    example_result_transformations()
    example_batch_comparison()
    example_similarity_grouping()
    example_statistical_workflow()

    print("\n" + "=" * 60)
    print("All analysis examples completed!")
    print("=" * 60)
