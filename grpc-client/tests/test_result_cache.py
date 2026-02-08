"""Tests for result caching functionality."""

import pytest
import time
import tempfile
from pathlib import Path
from arvak_grpc.types import JobResult
from arvak_grpc.result_cache import (
    MemoryCache,
    DiskCache,
    TwoLevelCache,
    CachedClient,
)


@pytest.fixture
def sample_result():
    """Sample JobResult."""
    return JobResult(
        job_id="test-123",
        counts={"00": 500, "11": 500},
        shots=1000,
        execution_time_ms=42,
    )


@pytest.fixture
def sample_results():
    """Multiple sample JobResults."""
    return [
        JobResult(f"test-{i}", {"00": 50*i, "11": 50*(10-i)}, 500)
        for i in range(10)
    ]


class TestMemoryCache:
    """Test MemoryCache functionality."""

    def test_basic_put_get(self, sample_result):
        """Test basic put and get operations."""
        cache = MemoryCache(max_size=10)

        cache.put(sample_result)
        retrieved = cache.get(sample_result.job_id)

        assert retrieved is not None
        assert retrieved.job_id == sample_result.job_id
        assert retrieved.counts == sample_result.counts

    def test_cache_miss(self):
        """Test cache miss returns None."""
        cache = MemoryCache()

        result = cache.get("nonexistent")

        assert result is None

    def test_lru_eviction(self, sample_results):
        """Test LRU eviction when max_size is reached."""
        cache = MemoryCache(max_size=5)

        # Add 10 results, only last 5 should remain
        for result in sample_results:
            cache.put(result)

        assert cache.size() == 5

        # First 5 should be evicted
        for result in sample_results[:5]:
            assert cache.get(result.job_id) is None

        # Last 5 should be present
        for result in sample_results[5:]:
            assert cache.get(result.job_id) is not None

    def test_lru_order(self, sample_results):
        """Test that access updates LRU order."""
        cache = MemoryCache(max_size=3)

        # Add 3 results
        for result in sample_results[:3]:
            cache.put(result)

        # Access first result (moves to end)
        cache.get(sample_results[0].job_id)

        # Add one more (should evict second result)
        cache.put(sample_results[3])

        # First should still be present
        assert cache.get(sample_results[0].job_id) is not None
        # Second should be evicted
        assert cache.get(sample_results[1].job_id) is None
        # Third and fourth should be present
        assert cache.get(sample_results[2].job_id) is not None
        assert cache.get(sample_results[3].job_id) is not None

    def test_ttl_expiration(self, sample_result):
        """Test TTL-based expiration."""
        cache = MemoryCache(max_size=10, ttl=0.5)  # 0.5 second TTL

        cache.put(sample_result)

        # Should be available immediately
        assert cache.get(sample_result.job_id) is not None

        # Wait for expiration
        time.sleep(0.6)

        # Should be expired
        assert cache.get(sample_result.job_id) is None

    def test_manual_eviction(self, sample_results):
        """Test manual eviction of expired entries."""
        cache = MemoryCache(max_size=10, ttl=0.5)

        # Add results
        for result in sample_results[:5]:
            cache.put(result)

        assert cache.size() == 5

        # Wait for expiration
        time.sleep(0.6)

        # Manual eviction
        evicted = cache.evict_expired()

        assert evicted == 5
        assert cache.size() == 0

    def test_remove(self, sample_result):
        """Test manual removal."""
        cache = MemoryCache()

        cache.put(sample_result)
        assert cache.get(sample_result.job_id) is not None

        removed = cache.remove(sample_result.job_id)
        assert removed is True
        assert cache.get(sample_result.job_id) is None

        # Remove again should return False
        removed = cache.remove(sample_result.job_id)
        assert removed is False

    def test_clear(self, sample_results):
        """Test clearing entire cache."""
        cache = MemoryCache()

        for result in sample_results:
            cache.put(result)

        assert cache.size() == len(sample_results)

        cache.clear()

        assert cache.size() == 0
        for result in sample_results:
            assert cache.get(result.job_id) is None

    def test_statistics(self, sample_result):
        """Test cache statistics tracking."""
        cache = MemoryCache(max_size=10)

        # Initial stats
        stats = cache.stats()
        assert stats['hits'] == 0
        assert stats['misses'] == 0
        assert stats['hit_rate'] == 0.0

        # One miss
        cache.get("nonexistent")
        stats = cache.stats()
        assert stats['misses'] == 1

        # Add and hit
        cache.put(sample_result)
        cache.get(sample_result.job_id)
        stats = cache.stats()
        assert stats['hits'] == 1
        assert stats['misses'] == 1
        assert stats['hit_rate'] == 0.5


class TestDiskCache:
    """Test DiskCache functionality."""

    def test_basic_json_cache(self, sample_result):
        """Test basic JSON caching."""
        with tempfile.TemporaryDirectory() as tmpdir:
            cache = DiskCache(tmpdir, format="json")

            cache.put(sample_result)
            retrieved = cache.get(sample_result.job_id)

            assert retrieved is not None
            assert retrieved.job_id == sample_result.job_id
            assert retrieved.counts == sample_result.counts

    def test_basic_parquet_cache(self, sample_result):
        """Test basic Parquet caching."""
        pytest.importorskip("pyarrow")

        with tempfile.TemporaryDirectory() as tmpdir:
            cache = DiskCache(tmpdir, format="parquet")

            cache.put(sample_result)
            retrieved = cache.get(sample_result.job_id)

            assert retrieved is not None
            assert retrieved.job_id == sample_result.job_id
            assert retrieved.counts == sample_result.counts

    def test_persistence(self, sample_result):
        """Test that cache persists across instances."""
        with tempfile.TemporaryDirectory() as tmpdir:
            # First instance
            cache1 = DiskCache(tmpdir, format="json")
            cache1.put(sample_result)

            # Second instance (loads existing metadata)
            cache2 = DiskCache(tmpdir, format="json")
            retrieved = cache2.get(sample_result.job_id)

            assert retrieved is not None
            assert retrieved.counts == sample_result.counts

    def test_ttl_expiration(self, sample_result):
        """Test TTL expiration for disk cache."""
        with tempfile.TemporaryDirectory() as tmpdir:
            cache = DiskCache(tmpdir, format="json", ttl=0.5)

            cache.put(sample_result)
            assert cache.get(sample_result.job_id) is not None

            time.sleep(0.6)

            assert cache.get(sample_result.job_id) is None

    def test_remove(self, sample_result):
        """Test removing cached files."""
        with tempfile.TemporaryDirectory() as tmpdir:
            cache = DiskCache(tmpdir, format="json")

            cache.put(sample_result)
            cache_path = cache._get_cache_path(sample_result.job_id)
            assert cache_path.exists()

            removed = cache.remove(sample_result.job_id)

            assert removed is True
            assert not cache_path.exists()
            assert cache.get(sample_result.job_id) is None

    def test_disk_usage(self, sample_results):
        """Test disk usage calculation."""
        with tempfile.TemporaryDirectory() as tmpdir:
            cache = DiskCache(tmpdir, format="json")

            for result in sample_results:
                cache.put(result)

            usage = cache.disk_usage()
            assert usage > 0
            assert cache.size() == len(sample_results)

    def test_clear(self, sample_results):
        """Test clearing disk cache."""
        with tempfile.TemporaryDirectory() as tmpdir:
            cache = DiskCache(tmpdir, format="json")

            for result in sample_results:
                cache.put(result)

            assert cache.size() == len(sample_results)

            cache.clear()

            assert cache.size() == 0
            for result in sample_results:
                assert cache.get(result.job_id) is None


class TestTwoLevelCache:
    """Test TwoLevelCache functionality."""

    def test_l1_hit(self, sample_result):
        """Test L1 cache hit."""
        with tempfile.TemporaryDirectory() as tmpdir:
            cache = TwoLevelCache(memory_size=10, cache_dir=tmpdir)

            cache.put(sample_result)

            # Should be in L1
            retrieved = cache.get(sample_result.job_id)
            assert retrieved is not None

            stats = cache.stats()
            assert stats['l1']['hits'] == 1

    def test_l2_promotion(self, sample_results):
        """Test L2 to L1 promotion."""
        with tempfile.TemporaryDirectory() as tmpdir:
            cache = TwoLevelCache(memory_size=5, cache_dir=tmpdir)

            # Add 10 results
            for result in sample_results:
                cache.put(result)

            # L1 should have last 5
            stats = cache.stats()
            assert stats['l1']['size'] == 5

            # Access old result (should be in L2)
            retrieved = cache.get(sample_results[0].job_id)
            assert retrieved is not None

            # Should now be in L1 (promoted)
            l1_retrieved = cache.l1.get(sample_results[0].job_id)
            assert l1_retrieved is not None

    def test_both_caches_populated(self, sample_result):
        """Test that put populates both caches."""
        with tempfile.TemporaryDirectory() as tmpdir:
            cache = TwoLevelCache(cache_dir=tmpdir)

            cache.put(sample_result)

            # Should be in both L1 and L2
            assert cache.l1.get(sample_result.job_id) is not None
            assert cache.l2.get(sample_result.job_id) is not None

    def test_remove_from_both(self, sample_result):
        """Test that remove affects both caches."""
        with tempfile.TemporaryDirectory() as tmpdir:
            cache = TwoLevelCache(cache_dir=tmpdir)

            cache.put(sample_result)
            cache.remove(sample_result.job_id)

            # Should be removed from both
            assert cache.l1.get(sample_result.job_id) is None
            assert cache.l2.get(sample_result.job_id) is None

    def test_evict_expired(self, sample_results):
        """Test evicting expired entries from both levels."""
        with tempfile.TemporaryDirectory() as tmpdir:
            cache = TwoLevelCache(
                memory_size=5,
                cache_dir=tmpdir,
                memory_ttl=0.5,
                disk_ttl=0.5,
            )

            for result in sample_results[:5]:
                cache.put(result)

            time.sleep(0.6)

            eviction_stats = cache.evict_expired()

            assert eviction_stats['l1_evicted'] > 0
            assert eviction_stats['l2_evicted'] > 0


class TestCachedClient:
    """Test CachedClient wrapper."""

    def test_transparent_caching(self, sample_result):
        """Test that caching is transparent."""

        class MockClient:
            def get_job_result(self, job_id):
                return sample_result

        with tempfile.TemporaryDirectory() as tmpdir:
            mock_client = MockClient()
            cache = TwoLevelCache(cache_dir=tmpdir)
            cached_client = CachedClient(mock_client, cache)

            # First call goes to server
            result1 = cached_client.get_job_result(sample_result.job_id)
            assert result1.job_id == sample_result.job_id

            # Second call comes from cache
            result2 = cached_client.get_job_result(sample_result.job_id)
            assert result2.job_id == sample_result.job_id

            # Check cache was used
            stats = cached_client.cache_stats()
            assert stats['l1']['hits'] > 0

    def test_auto_cache_disabled(self, sample_result):
        """Test with auto_cache disabled."""

        class MockClient:
            def get_job_result(self, job_id):
                return sample_result

        with tempfile.TemporaryDirectory() as tmpdir:
            mock_client = MockClient()
            cache = TwoLevelCache(cache_dir=tmpdir)
            cached_client = CachedClient(mock_client, cache, auto_cache=False)

            # Call won't cache
            result = cached_client.get_job_result(sample_result.job_id)

            # Cache should be empty
            stats = cached_client.cache_stats()
            assert stats['l1']['size'] == 0

    def test_attribute_delegation(self, sample_result):
        """Test that unknown attributes delegate to wrapped client."""

        class MockClient:
            def custom_method(self):
                return "custom"

        cached_client = CachedClient(MockClient())

        # Should delegate to wrapped client
        assert cached_client.custom_method() == "custom"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
