"""Result caching for efficient result storage and retrieval."""

import time
import json
import threading
from pathlib import Path
from typing import Optional, Dict, List, Union
from collections import OrderedDict
from dataclasses import dataclass
import hashlib

from .types import JobResult


@dataclass
class CacheEntry:
    """Cache entry with metadata."""
    result: JobResult
    timestamp: float
    access_count: int = 0
    last_access: float = 0.0

    def __post_init__(self):
        """Initialize access time."""
        if self.last_access == 0.0:
            self.last_access = self.timestamp


class MemoryCache:
    """In-memory LRU cache for JobResult objects.

    Features:
    - LRU eviction policy
    - TTL (time-to-live) support
    - Thread-safe operations
    - Access statistics
    """

    def __init__(self, max_size: int = 100, ttl: Optional[float] = None):
        """Initialize memory cache.

        Args:
            max_size: Maximum number of results to cache
            ttl: Time-to-live in seconds (None = no expiration)
        """
        self.max_size = max_size
        self.ttl = ttl
        self._lock = threading.Lock()
        self._cache: OrderedDict[str, CacheEntry] = OrderedDict()
        self._hits = 0
        self._misses = 0

    def get(self, job_id: str) -> Optional[JobResult]:
        """Get result from cache.

        Args:
            job_id: Job ID to retrieve

        Returns:
            JobResult if found and not expired, None otherwise
        """
        with self._lock:
            if job_id not in self._cache:
                self._misses += 1
                return None

            entry = self._cache[job_id]

            # Check TTL
            if self.ttl is not None:
                age = time.time() - entry.timestamp
                if age > self.ttl:
                    # Expired, remove from cache
                    del self._cache[job_id]
                    self._misses += 1
                    return None

            # Update access metadata
            entry.access_count += 1
            entry.last_access = time.time()

            # Move to end (most recently used)
            self._cache.move_to_end(job_id)

            self._hits += 1
            return entry.result

    def put(self, result: JobResult):
        """Put result in cache.

        Args:
            result: JobResult to cache
        """
        job_id = result.job_id

        with self._lock:
            # If already exists, update and move to end
            if job_id in self._cache:
                entry = self._cache[job_id]
                entry.result = result
                entry.timestamp = time.time()
                entry.last_access = time.time()
                self._cache.move_to_end(job_id)
                return

            # Check size limit
            if len(self._cache) >= self.max_size:
                # Remove least recently used (first item)
                self._cache.popitem(last=False)

            # Add new entry
            entry = CacheEntry(
                result=result,
                timestamp=time.time(),
                last_access=time.time(),
            )
            self._cache[job_id] = entry

    def remove(self, job_id: str) -> bool:
        """Remove result from cache.

        Args:
            job_id: Job ID to remove

        Returns:
            True if removed, False if not found
        """
        with self._lock:
            if job_id in self._cache:
                del self._cache[job_id]
                return True
            return False

    def clear(self):
        """Clear all cached results."""
        with self._lock:
            self._cache.clear()
            self._hits = 0
            self._misses = 0

    def size(self) -> int:
        """Get current cache size."""
        with self._lock:
            return len(self._cache)

    def stats(self) -> dict:
        """Get cache statistics.

        Returns:
            Dictionary with hits, misses, hit_rate, size
        """
        with self._lock:
            total = self._hits + self._misses
            hit_rate = self._hits / total if total > 0 else 0.0

            return {
                'hits': self._hits,
                'misses': self._misses,
                'hit_rate': hit_rate,
                'size': len(self._cache),
                'max_size': self.max_size,
            }

    def evict_expired(self) -> int:
        """Remove all expired entries.

        Returns:
            Number of entries evicted
        """
        if self.ttl is None:
            return 0

        with self._lock:
            current_time = time.time()
            expired = [
                job_id for job_id, entry in self._cache.items()
                if current_time - entry.timestamp > self.ttl
            ]

            for job_id in expired:
                del self._cache[job_id]

            return len(expired)


class DiskCache:
    """Disk-based persistent cache for JobResult objects.

    Supports multiple storage formats:
    - JSON (human-readable, slower)
    - Parquet (compressed, faster, requires pyarrow)
    """

    def __init__(
        self,
        cache_dir: Union[str, Path],
        format: str = "json",
        ttl: Optional[float] = None
    ):
        """Initialize disk cache.

        Args:
            cache_dir: Directory for cache files
            format: Storage format ('json' or 'parquet')
            ttl: Time-to-live in seconds (None = no expiration)
        """
        self.cache_dir = Path(cache_dir)
        self.format = format
        self.ttl = ttl

        # Create cache directory
        self.cache_dir.mkdir(parents=True, exist_ok=True)

        # Metadata file
        self.metadata_file = self.cache_dir / "_cache_metadata.json"
        self._metadata: Dict[str, dict] = self._load_metadata()

    def _load_metadata(self) -> dict:
        """Load cache metadata from disk."""
        if self.metadata_file.exists():
            with open(self.metadata_file, 'r') as f:
                return json.load(f)
        return {}

    def _save_metadata(self):
        """Save cache metadata to disk."""
        with open(self.metadata_file, 'w') as f:
            json.dump(self._metadata, f, indent=2)

    def _get_cache_path(self, job_id: str) -> Path:
        """Get cache file path for job ID.

        Uses hash prefix for better directory distribution.
        """
        # Use first 2 chars of hash as subdirectory
        hash_prefix = hashlib.sha256(job_id.encode()).hexdigest()[:2]
        subdir = self.cache_dir / hash_prefix
        subdir.mkdir(exist_ok=True)

        if self.format == "json":
            return subdir / f"{job_id}.json"
        elif self.format == "parquet":
            return subdir / f"{job_id}.parquet"
        else:
            raise ValueError(f"Unknown format: {self.format}")

    def get(self, job_id: str) -> Optional[JobResult]:
        """Get result from disk cache.

        Args:
            job_id: Job ID to retrieve

        Returns:
            JobResult if found and not expired, None otherwise
        """
        # Check metadata
        if job_id not in self._metadata:
            return None

        metadata = self._metadata[job_id]

        # Check TTL
        if self.ttl is not None:
            age = time.time() - metadata['timestamp']
            if age > self.ttl:
                self.remove(job_id)
                return None

        # Load from disk
        cache_path = self._get_cache_path(job_id)
        if not cache_path.exists():
            # File missing, clean up metadata
            del self._metadata[job_id]
            self._save_metadata()
            return None

        try:
            if self.format == "json":
                return self._load_json(cache_path)
            elif self.format == "parquet":
                return self._load_parquet(cache_path)
        except Exception:
            # Corrupted file, remove it
            self.remove(job_id)
            return None

    def put(self, result: JobResult):
        """Put result in disk cache.

        Args:
            result: JobResult to cache
        """
        job_id = result.job_id
        cache_path = self._get_cache_path(job_id)

        # Save to disk
        if self.format == "json":
            self._save_json(result, cache_path)
        elif self.format == "parquet":
            self._save_parquet(result, cache_path)

        # Update metadata
        self._metadata[job_id] = {
            'timestamp': time.time(),
            'path': str(cache_path),
            'shots': result.shots,
            'num_states': len(result.counts),
        }
        self._save_metadata()

    def remove(self, job_id: str) -> bool:
        """Remove result from cache.

        Args:
            job_id: Job ID to remove

        Returns:
            True if removed, False if not found
        """
        if job_id not in self._metadata:
            return False

        # Remove file
        cache_path = Path(self._metadata[job_id]['path'])
        if cache_path.exists():
            cache_path.unlink()

        # Remove metadata
        del self._metadata[job_id]
        self._save_metadata()

        return True

    def clear(self):
        """Clear entire cache."""
        # Remove all cache files
        for job_id in list(self._metadata.keys()):
            self.remove(job_id)

    def size(self) -> int:
        """Get number of cached results."""
        return len(self._metadata)

    def disk_usage(self) -> int:
        """Get total disk usage in bytes."""
        total = 0
        for metadata in self._metadata.values():
            path = Path(metadata['path'])
            if path.exists():
                total += path.stat().st_size
        return total

    def evict_expired(self) -> int:
        """Remove all expired entries.

        Returns:
            Number of entries evicted
        """
        if self.ttl is None:
            return 0

        current_time = time.time()
        expired = [
            job_id for job_id, metadata in self._metadata.items()
            if current_time - metadata['timestamp'] > self.ttl
        ]

        for job_id in expired:
            self.remove(job_id)

        return len(expired)

    def _save_json(self, result: JobResult, path: Path):
        """Save result as JSON."""
        data = {
            'job_id': result.job_id,
            'counts': result.counts,
            'shots': result.shots,
            'execution_time_ms': result.execution_time_ms,
            'metadata': result.metadata,
        }
        with open(path, 'w') as f:
            json.dump(data, f)

    def _load_json(self, path: Path) -> JobResult:
        """Load result from JSON."""
        with open(path, 'r') as f:
            data = json.load(f)
        return JobResult(**data)

    def _save_parquet(self, result: JobResult, path: Path):
        """Save result as Parquet."""
        from .result_export import ResultExporter
        ResultExporter.to_parquet(result, path, compression='snappy')

    def _load_parquet(self, path: Path) -> JobResult:
        """Load result from Parquet."""
        from .result_export import ResultExporter
        results = ResultExporter.from_parquet(path)
        return results[0] if results else None


class TwoLevelCache:
    """Two-level cache with memory (L1) and disk (L2).

    Provides fast memory access with disk persistence.
    """

    def __init__(
        self,
        memory_size: int = 100,
        cache_dir: Union[str, Path] = ".arvak_cache",
        format: str = "json",
        memory_ttl: Optional[float] = None,
        disk_ttl: Optional[float] = None,
    ):
        """Initialize two-level cache.

        Args:
            memory_size: L1 cache size
            cache_dir: L2 cache directory
            format: Disk cache format ('json' or 'parquet')
            memory_ttl: L1 TTL in seconds
            disk_ttl: L2 TTL in seconds
        """
        self.l1 = MemoryCache(max_size=memory_size, ttl=memory_ttl)
        self.l2 = DiskCache(cache_dir=cache_dir, format=format, ttl=disk_ttl)

    def get(self, job_id: str) -> Optional[JobResult]:
        """Get result from cache (L1 first, then L2).

        Args:
            job_id: Job ID to retrieve

        Returns:
            JobResult if found, None otherwise
        """
        # Try L1 first
        result = self.l1.get(job_id)
        if result is not None:
            return result

        # Try L2
        result = self.l2.get(job_id)
        if result is not None:
            # Promote to L1
            self.l1.put(result)
            return result

        return None

    def put(self, result: JobResult):
        """Put result in both caches.

        Args:
            result: JobResult to cache
        """
        self.l1.put(result)
        self.l2.put(result)

    def remove(self, job_id: str) -> bool:
        """Remove result from both caches.

        Args:
            job_id: Job ID to remove

        Returns:
            True if removed from at least one cache
        """
        l1_removed = self.l1.remove(job_id)
        l2_removed = self.l2.remove(job_id)
        return l1_removed or l2_removed

    def clear(self):
        """Clear both caches."""
        self.l1.clear()
        self.l2.clear()

    def stats(self) -> dict:
        """Get cache statistics.

        Returns:
            Dictionary with L1 and L2 stats
        """
        return {
            'l1': self.l1.stats(),
            'l2': {
                'size': self.l2.size(),
                'disk_usage_bytes': self.l2.disk_usage(),
            }
        }

    def evict_expired(self) -> dict:
        """Evict expired entries from both caches.

        Returns:
            Dictionary with eviction counts
        """
        return {
            'l1_evicted': self.l1.evict_expired(),
            'l2_evicted': self.l2.evict_expired(),
        }


class CachedClient:
    """ArvakClient wrapper with automatic result caching.

    Transparently caches job results for faster repeated access.
    """

    def __init__(
        self,
        client,
        cache: Optional[Union[MemoryCache, DiskCache, TwoLevelCache]] = None,
        auto_cache: bool = True,
    ):
        """Initialize cached client.

        Args:
            client: ArvakClient instance to wrap
            cache: Cache instance (default: TwoLevelCache)
            auto_cache: Automatically cache results on retrieval
        """
        self.client = client
        self.cache = cache or TwoLevelCache()
        self.auto_cache = auto_cache

    def __getattr__(self, name):
        """Delegate unknown attributes to wrapped client."""
        return getattr(self.client, name)

    def get_job_result(self, job_id: str):
        """Get job result with caching.

        Args:
            job_id: Job ID to retrieve

        Returns:
            JobResult
        """
        # Check cache first
        result = self.cache.get(job_id)
        if result is not None:
            return result

        # Fetch from server
        result = self.client.get_job_result(job_id)

        # Cache if enabled
        if self.auto_cache:
            self.cache.put(result)

        return result

    def wait_for_job(self, job_id: str, **kwargs):
        """Wait for job and cache result.

        Args:
            job_id: Job ID to wait for
            **kwargs: Arguments passed to client.wait_for_job()

        Returns:
            JobResult
        """
        result = self.client.wait_for_job(job_id, **kwargs)

        if self.auto_cache:
            self.cache.put(result)

        return result

    def cache_stats(self) -> dict:
        """Get cache statistics."""
        if hasattr(self.cache, 'stats'):
            return self.cache.stats()
        return {}
