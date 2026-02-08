#!/usr/bin/env python3
"""Example: Exporting results to various formats."""

from arvak_grpc import ArvakClient, ResultExporter, BatchExporter, get_parquet_metadata
from pathlib import Path
import tempfile

BELL_STATE = """
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
"""


def example_single_result_export():
    """Export a single result to multiple formats."""
    print("=" * 60)
    print("Example 1: Single Result Export")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Submit and get result
    print("\nSubmitting job...")
    job_id = client.submit_qasm(BELL_STATE, "simulator", shots=1000)
    result = client.wait_for_job(job_id)

    print(f"Got result: {len(result.counts)} measurement states")

    with tempfile.TemporaryDirectory() as tmpdir:
        tmppath = Path(tmpdir)

        # Export to Parquet
        parquet_file = tmppath / "result.parquet"
        print(f"\nExporting to Parquet: {parquet_file.name}")
        ResultExporter.to_parquet(result, parquet_file, compression='snappy')
        print(f"  File size: {parquet_file.stat().st_size} bytes")

        # Get metadata
        metadata = get_parquet_metadata(parquet_file)
        print(f"  Rows: {metadata['num_rows']}")
        print(f"  Columns: {metadata['num_columns']}")

        # Export to CSV
        csv_file = tmppath / "result.csv"
        print(f"\nExporting to CSV: {csv_file.name}")
        ResultExporter.to_csv(result, csv_file, include_probability=True)
        print(f"  File size: {csv_file.stat().st_size} bytes")

        # Export to JSON
        json_file = tmppath / "result.json"
        print(f"\nExporting to JSON: {json_file.name}")
        ResultExporter.to_json(result, json_file, indent=2)
        print(f"  File size: {json_file.stat().st_size} bytes")

        # Load back from Parquet
        print(f"\nLoading from Parquet...")
        loaded_results = ResultExporter.from_parquet(parquet_file)
        print(f"  Loaded {len(loaded_results)} result(s)")
        print(f"  Counts match: {loaded_results[0].counts == result.counts}")

    client.close()


def example_batch_export():
    """Export multiple results in batch."""
    print("\n" + "=" * 60)
    print("Example 2: Batch Export")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Submit batch
    circuits = [(BELL_STATE, 500) for _ in range(5)]
    print(f"\nSubmitting batch of {len(circuits)} jobs...")

    job_ids = client.submit_batch(circuits, "simulator")

    # Wait for all results
    print("Waiting for results...")
    results = []
    for i, job_id in enumerate(job_ids, 1):
        result = client.wait_for_job(job_id)
        results.append(result)
        print(f"  Job {i}/{len(job_ids)} completed")

    with tempfile.TemporaryDirectory() as tmpdir:
        tmppath = Path(tmpdir)

        # Export all results to single Parquet file
        parquet_file = tmppath / "batch_results.parquet"
        print(f"\nExporting {len(results)} results to Parquet...")
        ResultExporter.to_parquet(results, parquet_file, compression='snappy')

        metadata = get_parquet_metadata(parquet_file)
        print(f"  Total rows: {metadata['num_rows']}")
        print(f"  File size: {parquet_file.stat().st_size} bytes")
        print(f"  Compression: snappy")

        # Load back
        print(f"\nLoading from Parquet...")
        loaded = ResultExporter.from_parquet(parquet_file)
        print(f"  Loaded {len(loaded)} results")

    client.close()


def example_batch_exporter():
    """Use BatchExporter for incremental export."""
    print("\n" + "=" * 60)
    print("Example 3: Incremental Batch Export")
    print("=" * 60)

    client = ArvakClient("localhost:50051")
    exporter = BatchExporter()

    # Submit jobs and add results incrementally
    circuits = [(BELL_STATE, 200) for _ in range(10)]
    print(f"\nSubmitting {len(circuits)} jobs...")

    job_ids = client.submit_batch(circuits, "simulator")

    print("Processing results as they complete...")
    for i, job_id in enumerate(job_ids, 1):
        result = client.wait_for_job(job_id)
        exporter.add(result)
        print(f"  Added result {i}/{len(job_ids)} to batch")

    print(f"\nBatch contains {exporter.count()} results")

    with tempfile.TemporaryDirectory() as tmpdir:
        tmppath = Path(tmpdir)

        # Export to multiple formats
        print("\nExporting to multiple formats...")

        parquet_file = tmppath / "batch.parquet"
        exporter.to_parquet(parquet_file, compression='lz4')
        print(f"  Parquet: {parquet_file.stat().st_size} bytes (lz4)")

        csv_file = tmppath / "batch.csv"
        exporter.to_csv(csv_file)
        print(f"  CSV: {csv_file.stat().st_size} bytes")

        json_file = tmppath / "batch.json"
        exporter.to_json(json_file, indent=None)  # Compact JSON
        print(f"  JSON: {json_file.stat().st_size} bytes (compact)")

    client.close()


def example_compression_comparison():
    """Compare different compression codecs."""
    print("\n" + "=" * 60)
    print("Example 4: Compression Comparison")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Get a decent amount of data
    circuits = [(BELL_STATE, 1000) for _ in range(20)]
    print(f"\nSubmitting {len(circuits)} jobs...")

    job_ids = client.submit_batch(circuits, "simulator")

    print("Waiting for all results...")
    results = [client.wait_for_job(job_id) for job_id in job_ids]

    with tempfile.TemporaryDirectory() as tmpdir:
        tmppath = Path(tmpdir)

        print(f"\nComparing compression codecs for {len(results)} results:")

        codecs = ['none', 'snappy', 'gzip', 'lz4', 'zstd']
        for codec in codecs:
            parquet_file = tmppath / f"results_{codec}.parquet"

            try:
                ResultExporter.to_parquet(results, parquet_file, compression=codec)
                size = parquet_file.stat().st_size
                print(f"  {codec:8s}: {size:,} bytes")
            except Exception as e:
                print(f"  {codec:8s}: Not available ({e})")

    client.close()


def example_arrow_table_usage():
    """Work directly with Arrow tables."""
    print("\n" + "=" * 60)
    print("Example 5: Arrow Table Usage")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Get results
    circuits = [(BELL_STATE, 500) for _ in range(3)]
    job_ids = client.submit_batch(circuits, "simulator")
    results = [client.wait_for_job(job_id) for job_id in job_ids]

    print(f"\nConverting {len(results)} results to Arrow table...")

    # Convert to Arrow table
    table = ResultExporter.to_arrow_table(results)

    print(f"\nArrow Table:")
    print(f"  Rows: {table.num_rows}")
    print(f"  Columns: {table.num_columns}")
    print(f"  Schema:")
    for field in table.schema:
        print(f"    {field.name}: {field.type}")

    # Filter table
    print(f"\nFiltering bitstrings with count > 100...")
    filtered = table.filter(table['count'] > 100)
    print(f"  Filtered rows: {filtered.num_rows}")

    # Group by bitstring
    print(f"\nGrouping by bitstring...")
    import pyarrow.compute as pc
    unique_bitstrings = pc.unique(table['bitstring'])
    print(f"  Unique bitstrings: {len(unique_bitstrings)}")

    client.close()


if __name__ == "__main__":
    try:
        import pyarrow
        print(f"PyArrow version: {pyarrow.__version__}")
        print()

        example_single_result_export()
        example_batch_export()
        example_batch_exporter()
        example_compression_comparison()
        example_arrow_table_usage()

        print("\n" + "=" * 60)
        print("All export examples completed!")
        print("=" * 60)

    except ImportError:
        print("ERROR: PyArrow is required for this example")
        print("Install with: pip install pyarrow")
