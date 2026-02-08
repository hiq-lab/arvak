"""Result export to various formats (Arrow, Parquet, etc.)."""

from pathlib import Path
from typing import List, Optional, Union
import json

try:
    import pyarrow as pa
    import pyarrow.parquet as pq
    ARROW_AVAILABLE = True
except ImportError:
    ARROW_AVAILABLE = False

from .types import JobResult


class ResultExporter:
    """Exporter for quantum circuit execution results.

    Supports multiple formats:
    - Apache Arrow (in-memory columnar)
    - Parquet (compressed columnar storage)
    - CSV (human-readable)
    - JSON (structured data)
    """

    @staticmethod
    def to_arrow_table(results: Union[JobResult, List[JobResult]]) -> 'pa.Table':
        """Convert results to Apache Arrow table.

        Args:
            results: Single result or list of results

        Returns:
            Arrow Table with columns: job_id, bitstring, count, shots, probability

        Raises:
            ImportError: If pyarrow is not installed
        """
        if not ARROW_AVAILABLE:
            raise ImportError(
                "pyarrow is required for Arrow export. "
                "Install with: pip install pyarrow"
            )

        # Normalize to list
        if isinstance(results, JobResult):
            results = [results]

        # Flatten results into rows
        rows = []
        for result in results:
            total_shots = sum(result.counts.values())
            for bitstring, count in result.counts.items():
                probability = count / total_shots if total_shots > 0 else 0.0
                rows.append({
                    'job_id': result.job_id,
                    'bitstring': bitstring,
                    'count': count,
                    'shots': result.shots,
                    'probability': probability,
                    'execution_time_ms': result.execution_time_ms or 0,
                })

        # Convert to Arrow table
        if not rows:
            # Empty table with schema
            schema = pa.schema([
                ('job_id', pa.string()),
                ('bitstring', pa.string()),
                ('count', pa.int64()),
                ('shots', pa.int32()),
                ('probability', pa.float64()),
                ('execution_time_ms', pa.int64()),
            ])
            return pa.Table.from_pylist([], schema=schema)

        return pa.Table.from_pylist(rows)

    @staticmethod
    def to_parquet(
        results: Union[JobResult, List[JobResult]],
        path: Union[str, Path],
        compression: str = 'snappy',
        **kwargs
    ):
        """Export results to Parquet file.

        Args:
            results: Single result or list of results
            path: Output file path
            compression: Compression codec ('snappy', 'gzip', 'lz4', 'zstd', 'none')
            **kwargs: Additional arguments for pyarrow.parquet.write_table

        Raises:
            ImportError: If pyarrow is not installed
        """
        if not ARROW_AVAILABLE:
            raise ImportError(
                "pyarrow is required for Parquet export. "
                "Install with: pip install pyarrow"
            )

        table = ResultExporter.to_arrow_table(results)
        pq.write_table(table, str(path), compression=compression, **kwargs)

    @staticmethod
    def from_parquet(path: Union[str, Path]) -> List[JobResult]:
        """Load results from Parquet file.

        Args:
            path: Input file path

        Returns:
            List of JobResult objects

        Raises:
            ImportError: If pyarrow is not installed
        """
        if not ARROW_AVAILABLE:
            raise ImportError(
                "pyarrow is required for Parquet import. "
                "Install with: pip install pyarrow"
            )

        table = pq.read_table(str(path))
        return ResultExporter.from_arrow_table(table)

    @staticmethod
    def from_arrow_table(table: 'pa.Table') -> List[JobResult]:
        """Convert Arrow table back to JobResult objects.

        Args:
            table: Arrow table with result data

        Returns:
            List of JobResult objects

        Raises:
            ImportError: If pyarrow is not installed
        """
        if not ARROW_AVAILABLE:
            raise ImportError("pyarrow is required")

        # Convert to pandas for easier grouping
        df = table.to_pandas()

        results = []
        for job_id in df['job_id'].unique():
            job_data = df[df['job_id'] == job_id]

            counts = dict(zip(job_data['bitstring'], job_data['count']))
            shots = int(job_data['shots'].iloc[0])
            execution_time_ms = int(job_data['execution_time_ms'].iloc[0])

            result = JobResult(
                job_id=job_id,
                counts=counts,
                shots=shots,
                execution_time_ms=execution_time_ms if execution_time_ms > 0 else None,
            )
            results.append(result)

        return results

    @staticmethod
    def to_csv(
        results: Union[JobResult, List[JobResult]],
        path: Union[str, Path],
        include_probability: bool = True,
    ):
        """Export results to CSV file.

        Args:
            results: Single result or list of results
            path: Output file path
            include_probability: Include probability column
        """
        import csv

        if isinstance(results, JobResult):
            results = [results]

        with open(path, 'w', newline='') as f:
            fieldnames = ['job_id', 'bitstring', 'count', 'shots']
            if include_probability:
                fieldnames.append('probability')

            writer = csv.DictWriter(f, fieldnames=fieldnames)
            writer.writeheader()

            for result in results:
                total_shots = sum(result.counts.values())
                for bitstring, count in result.counts.items():
                    row = {
                        'job_id': result.job_id,
                        'bitstring': bitstring,
                        'count': count,
                        'shots': result.shots,
                    }
                    if include_probability:
                        row['probability'] = count / total_shots if total_shots > 0 else 0.0
                    writer.writerow(row)

    @staticmethod
    def to_json(
        results: Union[JobResult, List[JobResult]],
        path: Union[str, Path],
        indent: Optional[int] = 2,
    ):
        """Export results to JSON file.

        Args:
            results: Single result or list of results
            path: Output file path
            indent: JSON indentation (None for compact)
        """
        if isinstance(results, JobResult):
            results = [results]

        data = []
        for result in results:
            data.append({
                'job_id': result.job_id,
                'counts': result.counts,
                'shots': result.shots,
                'execution_time_ms': result.execution_time_ms,
                'metadata': result.metadata,
            })

        with open(path, 'w') as f:
            json.dump(data, f, indent=indent)

    @staticmethod
    def from_json(path: Union[str, Path]) -> List[JobResult]:
        """Load results from JSON file.

        Args:
            path: Input file path

        Returns:
            List of JobResult objects
        """
        with open(path, 'r') as f:
            data = json.load(f)

        results = []
        for item in data:
            result = JobResult(
                job_id=item['job_id'],
                counts=item['counts'],
                shots=item['shots'],
                execution_time_ms=item.get('execution_time_ms'),
                metadata=item.get('metadata'),
            )
            results.append(result)

        return results


class BatchExporter:
    """Efficient batch export of multiple job results.

    Example:
        >>> exporter = BatchExporter()
        >>> for result in results:
        ...     exporter.add(result)
        >>> exporter.to_parquet("results.parquet")
    """

    def __init__(self):
        """Initialize batch exporter."""
        self.results: List[JobResult] = []

    def add(self, result: JobResult):
        """Add a result to the batch.

        Args:
            result: JobResult to add
        """
        self.results.append(result)

    def add_many(self, results: List[JobResult]):
        """Add multiple results to the batch.

        Args:
            results: List of JobResult objects
        """
        self.results.extend(results)

    def clear(self):
        """Clear all results from the batch."""
        self.results.clear()

    def count(self) -> int:
        """Get number of results in batch."""
        return len(self.results)

    def to_parquet(
        self,
        path: Union[str, Path],
        compression: str = 'snappy',
        **kwargs
    ):
        """Export batch to Parquet file.

        Args:
            path: Output file path
            compression: Compression codec
            **kwargs: Additional arguments for write_table
        """
        ResultExporter.to_parquet(self.results, path, compression, **kwargs)

    def to_arrow_table(self) -> 'pa.Table':
        """Convert batch to Arrow table.

        Returns:
            Arrow Table
        """
        return ResultExporter.to_arrow_table(self.results)

    def to_csv(self, path: Union[str, Path], include_probability: bool = True):
        """Export batch to CSV file.

        Args:
            path: Output file path
            include_probability: Include probability column
        """
        ResultExporter.to_csv(self.results, path, include_probability)

    def to_json(self, path: Union[str, Path], indent: Optional[int] = 2):
        """Export batch to JSON file.

        Args:
            path: Output file path
            indent: JSON indentation
        """
        ResultExporter.to_json(self.results, path, indent)


def get_parquet_metadata(path: Union[str, Path]) -> dict:
    """Get metadata from Parquet file.

    Args:
        path: Parquet file path

    Returns:
        Dictionary with metadata (num_rows, num_columns, file_size, etc.)
    """
    if not ARROW_AVAILABLE:
        raise ImportError("pyarrow is required")

    parquet_file = pq.ParquetFile(str(path))

    return {
        'num_rows': parquet_file.metadata.num_rows,
        'num_columns': parquet_file.metadata.num_columns,
        'num_row_groups': parquet_file.metadata.num_row_groups,
        'format_version': parquet_file.metadata.format_version,
        'created_by': parquet_file.metadata.created_by,
        'serialized_size': parquet_file.metadata.serialized_size,
        'schema': str(parquet_file.schema),
    }
