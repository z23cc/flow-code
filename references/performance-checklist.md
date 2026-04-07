# Performance Checklist

Quick reference for application performance. Stack-agnostic — covers backend, CLI, and system-level patterns.

## Table of Contents

- [Measurement First](#measurement-first)
- [Algorithmic Efficiency](#algorithmic-efficiency)
- [Memory Management](#memory-management)
- [Concurrency](#concurrency)
- [Database Performance](#database-performance)
- [Network and I/O](#network-and-io)
- [Caching](#caching)
- [Build and Deploy](#build-and-deploy)
- [Common Anti-Patterns](#common-anti-patterns)

## Measurement First

- [ ] Performance targets defined before optimizing (latency p50/p95/p99, throughput)
- [ ] Benchmarks exist for hot paths (reproducible, tracked in CI)
- [ ] Profiler used before guessing bottleneck location
- [ ] Before/after measurements accompany every optimization PR

| Language | Profiler | Benchmark Tool |
|----------|---------|---------------|
| Rust | `perf`, `flamegraph`, `cargo-flamegraph` | `criterion`, `#[bench]` |
| Python | `cProfile`, `py-spy` | `pytest-benchmark`, `timeit` |
| Go | `pprof` (built-in) | `testing.B` (built-in) |
| TypeScript | Chrome DevTools, `clinic.js` | `vitest bench`, `tinybench` |

## Algorithmic Efficiency

- [ ] Hot-path algorithms are appropriate complexity (no O(n^2) where O(n log n) works)
- [ ] Data structures match access patterns (hashmap for lookup, sorted array for range)
- [ ] No redundant computation in loops (hoist invariants out)
- [ ] String concatenation in loops uses builder/buffer pattern
- [ ] Sorting uses appropriate algorithm for data characteristics

## Memory Management

- [ ] No unbounded growth (buffers, caches, queues have size limits)
- [ ] Large allocations reuse buffers where possible
- [ ] Streaming used for large data (not loading entire files into memory)
- [ ] Object pools used for frequently allocated/freed objects in hot paths
- [ ] Memory leaks checked (event listeners, timers, closures holding references)

| Language | Leak Detection |
|----------|---------------|
| Rust | `valgrind`, `miri`, ownership system prevents most leaks |
| Python | `tracemalloc`, `objgraph` |
| Go | `pprof heap`, `runtime.MemStats` |
| TypeScript | Chrome DevTools heap snapshot, `--inspect` |

## Concurrency

- [ ] I/O-bound work uses async/non-blocking patterns
- [ ] CPU-bound work parallelized across cores where beneficial
- [ ] Shared state minimized; prefer message passing or immutable data
- [ ] Lock granularity appropriate (no global locks in hot paths)
- [ ] Connection pools sized for expected concurrency
- [ ] Backpressure implemented for producer-consumer patterns

## Database Performance

- [ ] No N+1 query patterns (use joins, batch loading, or eager loading)
- [ ] Queries have appropriate indexes (check query plans)
- [ ] List endpoints paginated (never unbounded SELECT)
- [ ] Connection pooling configured and sized
- [ ] Slow query logging enabled (identify regressions)
- [ ] Bulk operations used instead of row-by-row inserts/updates
- [ ] Read replicas used for read-heavy workloads (if applicable)
- [ ] Transactions held for minimum duration

## Network and I/O

- [ ] API response times < 200ms (p95 target)
- [ ] Response compression enabled (gzip/brotli)
- [ ] Batch APIs available for multi-resource operations
- [ ] No synchronous blocking in async handlers
- [ ] File I/O uses buffered readers/writers
- [ ] DNS and connection reuse via keep-alive / connection pooling
- [ ] Timeouts set on all outbound calls (prevent hanging)

## Caching

- [ ] Cache invalidation strategy defined (TTL, event-based, or versioned keys)
- [ ] Cache hit rate monitored (< 80% hit rate = review strategy)
- [ ] Hot data cached close to consumer (in-process > local > distributed)
- [ ] Cache stampede protection (singleflight, probabilistic early expiry)
- [ ] Cache size bounded (LRU or similar eviction policy)
- [ ] Cached data has freshness expectations documented

## Build and Deploy

- [ ] Release builds use full optimizations (not debug builds in prod)
- [ ] Binary size minimized (strip symbols, LTO where appropriate)
- [ ] Startup time measured and optimized (lazy initialization)
- [ ] Health check endpoints respond quickly (no heavy initialization)
- [ ] Graceful shutdown drains in-flight requests

## Common Anti-Patterns

| Anti-Pattern | Impact | Fix |
|---|---|---|
| N+1 queries | Linear DB load growth | Use joins or batch loading |
| Unbounded queries | Memory exhaustion, timeouts | Always paginate, add LIMIT |
| Missing indexes | Slow reads as data grows | Add indexes for filtered/sorted columns |
| Premature optimization | Wasted effort, worse readability | Profile first, optimize measured bottlenecks |
| Global locks in hot paths | Serialized throughput | Fine-grained locks or lock-free structures |
| Allocating in tight loops | GC pressure, cache thrashing | Pre-allocate, reuse buffers |
| Synchronous I/O in async code | Thread starvation | Use async I/O or spawn blocking tasks |
| No timeouts on network calls | Resource exhaustion on failure | Set timeouts on every outbound call |
| Caching without eviction | Unbounded memory growth | Set max size with LRU eviction |
| Logging in hot paths | I/O bottleneck | Sample or batch log writes |
