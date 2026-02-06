#!/usr/bin/env python3
"""Example: DataFrame integration with pandas, polars, and visualization."""

from arvak_grpc import (
    ArvakClient,
    to_pandas,
    to_polars,
    batch_to_pandas,
    batch_to_polars,
    StatisticalAnalyzer,
    Visualizer,
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


def example_pandas_conversion():
    """Convert results to pandas DataFrame."""
    print("=" * 60)
    print("Example 1: Pandas DataFrame Conversion")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Submit job
    print("\nSubmitting Bell state job...")
    job_id = client.submit_qasm(BELL_STATE, "simulator", shots=1000)
    result = client.wait_for_job(job_id)

    print(f"Got result: {len(result.counts)} measurement states")

    # Convert to pandas DataFrame
    print("\nConverting to pandas DataFrame...")
    df = to_pandas(result)
    print(df)

    # With metadata
    print("\nWith metadata columns:")
    df_meta = to_pandas(result, include_metadata=True)
    print(df_meta)

    # Basic analysis with pandas
    print("\nBasic pandas analysis:")
    print(f"  Mean count: {df['count'].mean():.2f}")
    print(f"  Std count: {df['count'].std():.2f}")
    print(f"  Total probability: {df['probability'].sum():.4f}")

    client.close()


def example_polars_conversion():
    """Convert results to polars DataFrame."""
    print("\n" + "=" * 60)
    print("Example 2: Polars DataFrame Conversion")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Submit job
    print("\nSubmitting GHZ state job...")
    job_id = client.submit_qasm(GHZ_STATE, "simulator", shots=2000)
    result = client.wait_for_job(job_id)

    print(f"Got result: {len(result.counts)} measurement states")

    # Convert to polars DataFrame
    print("\nConverting to polars DataFrame...")
    df = to_polars(result)
    print(df)

    # Polars filtering and aggregation
    print("\nFiltering high-probability states (p > 0.1):")
    high_prob = df.filter(df['probability'] > 0.1)
    print(high_prob)

    # Summary statistics with polars
    print("\nPolars summary statistics:")
    print(df.select([
        df['count'].mean().alias('mean_count'),
        df['count'].std().alias('std_count'),
        df['probability'].sum().alias('total_prob'),
    ]))

    client.close()


def example_batch_dataframes():
    """Convert batch results to DataFrame."""
    print("\n" + "=" * 60)
    print("Example 3: Batch DataFrame Conversion")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Submit batch
    circuits = [(BELL_STATE, 500) for _ in range(5)]
    print(f"\nSubmitting batch of {len(circuits)} jobs...")

    job_ids = client.submit_batch(circuits, "simulator")
    results = [client.wait_for_job(job_id) for job_id in job_ids]

    print(f"Got {len(results)} results")

    # Convert all to single DataFrame
    print("\nConverting batch to pandas DataFrame...")
    df = batch_to_pandas(results)
    print(df.head(10))

    # Group by bitstring and compute statistics
    print("\nGrouped statistics by bitstring:")
    grouped = df.groupby('bitstring').agg({
        'count': ['sum', 'mean', 'std'],
        'probability': ['mean', 'std']
    })
    print(grouped)

    # Convert to polars for faster operations
    print("\nSame data in polars:")
    df_polars = batch_to_polars(results)
    print(df_polars.head(10))

    # Group with polars (faster for large datasets)
    print("\nPolars groupby:")
    grouped_polars = df_polars.groupby('bitstring').agg([
        df_polars['count'].sum().alias('total_count'),
        df_polars['count'].mean().alias('mean_count'),
        df_polars['probability'].mean().alias('mean_prob'),
    ])
    print(grouped_polars)

    client.close()


def example_statistical_analysis():
    """Statistical analysis of results."""
    print("\n" + "=" * 60)
    print("Example 4: Statistical Analysis")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Submit two different circuits
    print("\nSubmitting Bell state...")
    bell_id = client.submit_qasm(BELL_STATE, "simulator", shots=2000)
    bell_result = client.wait_for_job(bell_id)

    print("Submitting GHZ state...")
    ghz_id = client.submit_qasm(GHZ_STATE, "simulator", shots=2000)
    ghz_result = client.wait_for_job(ghz_id)

    # Compute entropy
    print("\nShannon Entropy:")
    bell_entropy = StatisticalAnalyzer.entropy(bell_result)
    ghz_entropy = StatisticalAnalyzer.entropy(ghz_result)
    print(f"  Bell state: {bell_entropy:.4f} bits")
    print(f"  GHZ state:  {ghz_entropy:.4f} bits")

    # Compute purity
    print("\nPurity:")
    bell_purity = StatisticalAnalyzer.purity(bell_result)
    ghz_purity = StatisticalAnalyzer.purity(ghz_result)
    print(f"  Bell state: {bell_purity:.6f}")
    print(f"  GHZ state:  {ghz_purity:.6f}")

    # Summary statistics
    print("\nBell State Summary:")
    bell_stats = StatisticalAnalyzer.summary_statistics(bell_result)
    for key, value in bell_stats.items():
        print(f"  {key}: {value}")

    print("\nGHZ State Summary:")
    ghz_stats = StatisticalAnalyzer.summary_statistics(ghz_result)
    for key, value in ghz_stats.items():
        print(f"  {key}: {value}")

    # Fidelity with ideal states
    print("\nFidelity Estimates:")
    ideal_bell = {"00": 0.5, "11": 0.5}
    ideal_ghz = {"000": 0.5, "111": 0.5}

    bell_fidelity = StatisticalAnalyzer.fidelity_estimate(bell_result, ideal_bell)
    ghz_fidelity = StatisticalAnalyzer.fidelity_estimate(ghz_result, ideal_ghz)

    print(f"  Bell with ideal: {bell_fidelity:.6f}")
    print(f"  GHZ with ideal:  {ghz_fidelity:.6f}")

    # Total variation distance
    print("\nTotal Variation Distance:")
    # Submit same circuit twice to compare
    bell_id2 = client.submit_qasm(BELL_STATE, "simulator", shots=2000)
    bell_result2 = client.wait_for_job(bell_id2)

    tvd = StatisticalAnalyzer.total_variation_distance(bell_result, bell_result2)
    print(f"  Between two Bell state runs: {tvd:.6f}")

    client.close()


def example_visualization():
    """Visualize measurement results."""
    print("\n" + "=" * 60)
    print("Example 5: Visualization")
    print("=" * 60)

    try:
        import matplotlib
        matplotlib.use('Agg')  # Non-interactive backend
        import matplotlib.pyplot as plt
    except ImportError:
        print("ERROR: matplotlib is required for this example")
        print("Install with: pip install matplotlib")
        return

    client = ArvakClient("localhost:50051")

    # Submit job
    print("\nSubmitting GHZ state job...")
    job_id = client.submit_qasm(GHZ_STATE, "simulator", shots=3000)
    result = client.wait_for_job(job_id)

    # Plot distribution
    print("Generating distribution plot...")
    fig, axes = Visualizer.plot_distribution(result, max_states=8)
    fig.savefig('distribution.png', dpi=150, bbox_inches='tight')
    print("  Saved to: distribution.png")
    plt.close(fig)

    # Plot statistics table
    print("Generating statistics table...")
    fig, ax = Visualizer.plot_statistics_table(result)
    fig.savefig('statistics.png', dpi=150, bbox_inches='tight')
    print("  Saved to: statistics.png")
    plt.close(fig)

    # Compare multiple runs
    print("\nSubmitting 3 Bell state jobs for comparison...")
    circuits = [(BELL_STATE, 1000) for _ in range(3)]
    job_ids = client.submit_batch(circuits, "simulator")
    results = [client.wait_for_job(job_id) for job_id in job_ids]

    print("Generating comparison plot...")
    fig, ax = Visualizer.plot_comparison(
        results,
        labels=["Run 1", "Run 2", "Run 3"],
        title="Bell State - Three Independent Runs"
    )
    fig.savefig('comparison.png', dpi=150, bbox_inches='tight')
    print("  Saved to: comparison.png")
    plt.close(fig)

    client.close()


def example_advanced_analysis():
    """Advanced analysis combining multiple tools."""
    print("\n" + "=" * 60)
    print("Example 6: Advanced Analysis Workflow")
    print("=" * 60)

    try:
        import pandas as pd
    except ImportError:
        print("ERROR: pandas is required for this example")
        return

    client = ArvakClient("localhost:50051")

    # Run multiple jobs with different shot counts
    shot_counts = [100, 500, 1000, 2000, 5000]
    print(f"\nRunning Bell state with different shot counts: {shot_counts}")

    results = []
    for shots in shot_counts:
        job_id = client.submit_qasm(BELL_STATE, "simulator", shots=shots)
        result = client.wait_for_job(job_id)
        results.append(result)
        print(f"  {shots} shots: {len(result.counts)} unique states")

    # Convert to DataFrame
    df = batch_to_pandas(results)

    # Analyze convergence with shot count
    print("\nAnalyzing statistical convergence:")
    analysis_data = []
    for shots, result in zip(shot_counts, results):
        stats = StatisticalAnalyzer.summary_statistics(result)
        ideal = {"00": 0.5, "11": 0.5}
        fidelity = StatisticalAnalyzer.fidelity_estimate(result, ideal)

        analysis_data.append({
            'shots': shots,
            'entropy': stats['entropy'],
            'purity': stats['purity'],
            'fidelity': fidelity,
            'unique_states': stats['unique_states'],
        })

    analysis_df = pd.DataFrame(analysis_data)
    print(analysis_df.to_string(index=False))

    # Compute convergence metrics
    print("\nConvergence analysis:")
    print(f"  Entropy stabilizes at: {analysis_df['entropy'].iloc[-1]:.4f} bits")
    print(f"  Final fidelity: {analysis_df['fidelity'].iloc[-1]:.6f}")
    print(f"  Fidelity improvement: {analysis_df['fidelity'].iloc[-1] - analysis_df['fidelity'].iloc[0]:.6f}")

    client.close()


if __name__ == "__main__":
    try:
        import pandas
        import polars
        print(f"Pandas version: {pandas.__version__}")
        print(f"Polars version: {polars.__version__}")
        print()

        example_pandas_conversion()
        example_polars_conversion()
        example_batch_dataframes()
        example_statistical_analysis()
        example_visualization()
        example_advanced_analysis()

        print("\n" + "=" * 60)
        print("All dataframe examples completed!")
        print("=" * 60)

    except ImportError as e:
        print(f"ERROR: Missing required package: {e}")
        print("\nInstall all dependencies with:")
        print("  pip install arvak-grpc[all]")
        print("\nOr install specific features:")
        print("  pip install arvak-grpc[export]  # pandas, pyarrow")
        print("  pip install arvak-grpc[polars]  # polars")
        print("  pip install arvak-grpc[viz]     # matplotlib, numpy")
