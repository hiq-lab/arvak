"""Arvak gRPC Client Library.

This package provides a Python client for the Arvak gRPC service,
enabling remote quantum circuit submission and execution.

Supports both synchronous and asynchronous APIs:
- ArvakClient: Synchronous blocking client
- AsyncArvakClient: Async/await client with connection pooling
"""

from .client import ArvakClient
from .async_client import AsyncArvakClient, ConnectionPool
from .job_future import JobFuture, CancelledError, as_completed, wait
from .retry_policy import (
    RetryPolicy,
    RetryStrategy,
    CircuitBreaker,
    CircuitBreakerConfig,
    CircuitBreakerError,
    CircuitState,
    ResilientClient,
    with_retry,
    with_circuit_breaker,
)
from .batch_manager import (
    BatchJobManager,
    BatchStatus,
    BatchProgress,
    BatchResult,
    print_progress_bar,
)
from .result_export import (
    ResultExporter,
    BatchExporter,
    get_parquet_metadata,
)
from .dataframe_integration import (
    DataFrameConverter,
    StatisticalAnalyzer,
    Visualizer,
    to_pandas,
    to_polars,
    batch_to_pandas,
    batch_to_polars,
)
from .result_cache import (
    MemoryCache,
    DiskCache,
    TwoLevelCache,
    CachedClient,
)
from .result_analysis import (
    ResultAggregator,
    ResultComparator,
    ConvergenceAnalyzer,
    ResultTransformer,
    AggregatedResult,
    ComparisonResult,
    ConvergenceAnalysis,
    batch_compare,
    group_by_similarity,
)
from .types import Job, JobResult, JobState, BackendInfo
from .exceptions import (
    ArvakError,
    ArvakJobNotFoundError,
    ArvakBackendNotFoundError,
    ArvakInvalidCircuitError,
    ArvakJobNotCompletedError,
)

__version__ = "1.5.2"
__all__ = [
    "ArvakClient",
    "AsyncArvakClient",
    "ConnectionPool",
    "JobFuture",
    "CancelledError",
    "as_completed",
    "wait",
    "RetryPolicy",
    "RetryStrategy",
    "CircuitBreaker",
    "CircuitBreakerConfig",
    "CircuitBreakerError",
    "CircuitState",
    "ResilientClient",
    "with_retry",
    "with_circuit_breaker",
    "BatchJobManager",
    "BatchStatus",
    "BatchProgress",
    "BatchResult",
    "print_progress_bar",
    "ResultExporter",
    "BatchExporter",
    "get_parquet_metadata",
    "DataFrameConverter",
    "StatisticalAnalyzer",
    "Visualizer",
    "to_pandas",
    "to_polars",
    "batch_to_pandas",
    "batch_to_polars",
    "MemoryCache",
    "DiskCache",
    "TwoLevelCache",
    "CachedClient",
    "ResultAggregator",
    "ResultComparator",
    "ConvergenceAnalyzer",
    "ResultTransformer",
    "AggregatedResult",
    "ComparisonResult",
    "ConvergenceAnalysis",
    "batch_compare",
    "group_by_similarity",
    "Job",
    "JobResult",
    "JobState",
    "BackendInfo",
    "ArvakError",
    "ArvakJobNotFoundError",
    "ArvakBackendNotFoundError",
    "ArvakInvalidCircuitError",
    "ArvakJobNotCompletedError",
]
