#!/usr/bin/env python3
"""Example: Result caching for improved performance."""

import time
import tempfile
from pathlib import Path
from arvak_grpc import (
    ArvakClient,
    MemoryCache,
    DiskCache,
    TwoLevelCache,
    CachedClient,
)

BELL_STATE = """
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
"""


def example_memory_cache():
    """Basic in-memory caching."""
    print("=" * 60)
    print("Example 1: Memory Cache")
    print("=" * 60)

    client = ArvakClient("localhost:50051")
    cache = MemoryCache(max_size=50, ttl=300)  # 50 results, 5 min TTL

    # Submit and cache results
    print("\nSubmitting 3 jobs...")
    job_ids = []
    for i in range(3):
        job_id = client.submit_qasm(BELL_STATE, "simulator", shots=1000)
        job_ids.append(job_id)

    results = []
    for job_id in job_ids:
        result = client.wait_for_job(job_id)
        cache.put(result)
        results.append(result)
        print(f"  Cached result: {job_id[:12]}...")

    # Retrieve from cache (fast)
    print("\nRetrieving from cache:")
    for job_id in job_ids:
        cached = cache.get(job_id)
        if cached:
            print(f"  Cache hit: {job_id[:12]}...")
        else:
            print(f"  Cache miss: {job_id[:12]}...")

    # Check statistics
    print("\nCache statistics:")
    stats = cache.stats()
    for key, value in stats.items():
        print(f"  {key}: {value}")

    client.close()


def example_disk_cache():
    """Persistent disk-based caching."""
    print("\n" + "=" * 60)
    print("Example 2: Disk Cache")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    with tempfile.TemporaryDirectory() as tmpdir:
        # JSON format
        print("\nUsing JSON format:")
        cache = DiskCache(tmpdir, format="json")

        job_id = client.submit_qasm(BELL_STATE, "simulator", shots=1000)
        result = client.wait_for_job(job_id)

        print(f"  Caching result: {job_id[:12]}...")
        cache.put(result)

        print(f"  Cache size: {cache.size()}")
        print(f"  Disk usage: {cache.disk_usage()} bytes")

        # Retrieve from disk
        print("\n  Retrieving from disk...")
        cached = cache.get(job_id)
        print(f"  Success: {cached is not None}")
        print(f"  Counts match: {cached.counts == result.counts}")

    # Parquet format (compressed)
    with tempfile.TemporaryDirectory() as tmpdir:
        print("\nUsing Parquet format:")
        cache = DiskCache(tmpdir, format="parquet")

        job_id = client.submit_qasm(BELL_STATE, "simulator", shots=2000)
        result = client.wait_for_job(job_id)

        cache.put(result)
        print(f"  Disk usage: {cache.disk_usage()} bytes (compressed)")

        cached = cache.get(job_id)
        print(f"  Counts match: {cached.counts == result.counts}")

    client.close()


def example_two_level_cache():
    """Two-level cache with memory and disk."""
    print("\n" + "=" * 60)
    print("Example 3: Two-Level Cache (L1 + L2)")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    with tempfile.TemporaryDirectory() as tmpdir:
        cache = TwoLevelCache(
            memory_size=10,
            cache_dir=tmpdir,
            format="parquet",
        )

        # Submit multiple jobs
        print("\nSubmitting 15 jobs...")
        job_ids = []
        for i in range(15):
            job_id = client.submit_qasm(BELL_STATE, "simulator", shots=500)
            job_ids.append(job_id)

        # Wait and cache
        print("Caching results...")
        for i, job_id in enumerate(job_ids, 1):
            result = client.wait_for_job(job_id)
            cache.put(result)
            if i % 5 == 0:
                print(f"  Cached {i}/15 results")

        # Check statistics
        stats = cache.stats()
        print(f"\nCache statistics:")
        print(f"  L1 size: {stats['l1']['size']}/{stats['l1']['max_size']}")
        print(f"  L2 size: {stats['l2']['size']}")
        print(f"  L2 disk usage: {stats['l2']['disk_usage_bytes']} bytes")

        # Access pattern: recent jobs are in L1
        print("\nAccessing recent jobs (L1 hits):")
        for job_id in job_ids[-3:]:
            cached = cache.get(job_id)
            print(f"  Retrieved: {job_id[:12]}...")

        print("\nAccessing older jobs (L2 hits, promoted to L1):")
        for job_id in job_ids[:3]:
            cached = cache.get(job_id)
            print(f"  Retrieved: {job_id[:12]}...")

        # Final L1 stats
        l1_stats = cache.stats()['l1']
        print(f"\nL1 hit rate: {l1_stats['hit_rate']:.2%}")

    client.close()


def example_cached_client():
    """Use CachedClient for transparent caching."""
    print("\n" + "=" * 60)
    print("Example 4: CachedClient Wrapper")
    print("=" * 60)

    base_client = ArvakClient("localhost:50051")

    with tempfile.TemporaryDirectory() as tmpdir:
        # Wrap client with caching
        client = CachedClient(
            base_client,
            cache=TwoLevelCache(cache_dir=tmpdir),
            auto_cache=True,
        )

        # Submit job
        print("\nSubmitting job...")
        job_id = client.submit_qasm(BELL_STATE, "simulator", shots=1000)

        # First retrieval: from server (slow)
        print("\nFirst retrieval (from server):")
        start = time.time()
        result1 = client.get_job_result(job_id)
        elapsed1 = time.time() - start
        print(f"  Time: {elapsed1*1000:.1f} ms")
        print(f"  Result: {len(result1.counts)} states")

        # Second retrieval: from cache (fast)
        print("\nSecond retrieval (from cache):")
        start = time.time()
        result2 = client.get_job_result(job_id)
        elapsed2 = time.time() - start
        print(f"  Time: {elapsed2*1000:.1f} ms")
        print(f"  Speedup: {elapsed1/elapsed2:.1f}x")
        print(f"  Results match: {result1.counts == result2.counts}")

        # Check cache stats
        print("\nCache statistics:")
        stats = client.cache_stats()
        print(f"  L1 hits: {stats['l1']['hits']}")
        print(f"  L1 hit rate: {stats['l1']['hit_rate']:.2%}")

    base_client.close()


def example_ttl_expiration():
    """Test TTL-based cache expiration."""
    print("\n" + "=" * 60)
    print("Example 5: TTL Expiration")
    print("=" * 60)

    client = ArvakClient("localhost:50051")
    cache = MemoryCache(max_size=100, ttl=2.0)  # 2 second TTL

    # Cache a result
    print("\nSubmitting job...")
    job_id = client.submit_qasm(BELL_STATE, "simulator", shots=1000)
    result = client.wait_for_job(job_id)

    cache.put(result)
    print(f"  Cached: {job_id[:12]}...")

    # Immediate retrieval works
    print("\nImmediate retrieval:")
    cached = cache.get(job_id)
    print(f"  Found: {cached is not None}")

    # Wait for expiration
    print("\nWaiting 3 seconds for expiration...")
    time.sleep(3)

    # Should be expired now
    print("Retrieval after expiration:")
    cached = cache.get(job_id)
    print(f"  Found: {cached is not None} (should be False)")

    # Check eviction
    cache.put(result)  # Re-add
    print(f"\nCache size before eviction: {cache.size()}")
    evicted = cache.evict_expired()
    print(f"Evicted: {evicted} entries")

    client.close()


def example_performance_comparison():
    """Compare performance with and without caching."""
    print("\n" + "=" * 60)
    print("Example 6: Performance Comparison")
    print("=" * 60)

    client = ArvakClient("localhost:50051")

    # Submit batch
    print("\nSubmitting 10 jobs...")
    job_ids = []
    for _ in range(10):
        job_id = client.submit_qasm(BELL_STATE, "simulator", shots=1000)
        job_ids.append(job_id)

    results = [client.wait_for_job(job_id) for job_id in job_ids]
    print(f"  All jobs completed")

    # Without caching: retrieve all results
    print("\nWithout caching (10 retrievals):")
    start = time.time()
    for job_id in job_ids:
        _ = client.get_job_result(job_id)
    elapsed_no_cache = time.time() - start
    print(f"  Time: {elapsed_no_cache*1000:.1f} ms")

    # With caching: first pass populates cache
    with tempfile.TemporaryDirectory() as tmpdir:
        cached_client = CachedClient(client, TwoLevelCache(cache_dir=tmpdir))

        print("\nWith caching - first pass (populate cache):")
        start = time.time()
        for job_id in job_ids:
            _ = cached_client.get_job_result(job_id)
        elapsed_cache_first = time.time() - start
        print(f"  Time: {elapsed_cache_first*1000:.1f} ms")

        # Second pass uses cache
        print("\nWith caching - second pass (from cache):")
        start = time.time()
        for job_id in job_ids:
            _ = cached_client.get_job_result(job_id)
        elapsed_cache_second = time.time() - start
        print(f"  Time: {elapsed_cache_second*1000:.1f} ms")
        print(f"  Speedup: {elapsed_cache_first/elapsed_cache_second:.1f}x")

        # Show cache stats
        stats = cached_client.cache_stats()
        print(f"\nCache hit rate: {stats['l1']['hit_rate']:.2%}")

    client.close()


if __name__ == "__main__":
    print("Arvak Result Caching Examples")
    print()

    example_memory_cache()
    example_disk_cache()
    example_two_level_cache()
    example_cached_client()
    example_ttl_expiration()
    example_performance_comparison()

    print("\n" + "=" * 60)
    print("All caching examples completed!")
    print("=" * 60)
