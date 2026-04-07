---
name: flow-code-performance
description: "Use when investigating performance issues, optimizing hot paths, adding benchmarks, or establishing performance regression guards"
---

# Performance Methodology

## Overview

Systematic performance optimization: measure first, then fix. Never optimize without a baseline. Intuition about bottlenecks is wrong more often than it is right -- profiling data is the only reliable guide. Every optimization must be measured before and after, and guarded against regression.

## When to Use

- Performance requirements exist in the spec (latency budgets, throughput SLAs)
- Users or monitoring report slow behavior
- Adding benchmarks to hot paths or critical operations
- Pre-release performance check or audit
- Investigating a suspected performance regression
- Optimizing build times, test suite duration, or CI pipeline speed

**When NOT to use:**
- Premature optimization -- no evidence of a problem exists yet
- No baseline measurements exist (measure first, then come back)
- Micro-optimizing code that isn't on the hot path
- "Making it faster" without a target metric to hit

## Core Process

```
1. MEASURE  --> Establish baseline with profiling tools
2. IDENTIFY --> Find the actual bottleneck (profiler, not intuition)
3. FIX      --> Apply targeted fix to the measured bottleneck only
4. VERIFY   --> Re-measure. If not measurably faster, revert
5. GUARD    --> Add regression test/benchmark to prevent regression
```

### Step 1: Measure

**No baseline = no optimization.** Before touching any code, capture reproducible numbers.

#### Multi-Stack Profiling Tools

| Language | Profiler | Benchmark Tool |
|----------|----------|----------------|
| Rust | `cargo flamegraph`, `perf`, `flamegraph` | `criterion`, `cargo bench` |
| Python | `cProfile`, `py-spy` | `pytest-benchmark`, `timeit` |
| Go | `pprof` (built-in) | `go test -bench`, `testing.B` |
| General | `time`, `perf stat`, `hyperfine` | Custom harness with warm-up runs |

#### What to Capture

```bash
# Rust: criterion benchmark with baseline
cd project && cargo bench -- --save-baseline before

# Python: profile a specific function
python -m cProfile -s cumtime script.py > profile_before.txt

# Go: CPU profile
go test -bench=BenchmarkHotPath -cpuprofile=cpu_before.prof ./...

# General: wall-clock timing with statistical rigor
hyperfine --warmup 3 './my_command'
```

Record these numbers in a file or commit message. You need them for Step 4.

### Step 2: Identify

**The bottleneck is rarely where you think.** Use profiling data to find it.

```
Where is time spent?
|-- CPU-bound
|   |-- Hot loop? --> Check algorithmic complexity, data structures
|   |-- Redundant computation? --> Cache or hoist invariants
|   +-- Serialized work? --> Parallelize across cores
|-- Memory-bound
|   |-- Excessive allocation? --> Reuse buffers, pre-allocate
|   |-- Cache thrashing? --> Improve data locality
|   +-- Unbounded growth? --> Add size limits, eviction
|-- I/O-bound
|   |-- Database? --> Check queries, indexes, N+1 patterns
|   |-- Network? --> Batch requests, connection pooling
|   |-- Disk? --> Buffer I/O, async operations
+-- Concurrency
    |-- Lock contention? --> Fine-grained locks, lock-free structures
    |-- Thread starvation? --> Tune pool sizes
    +-- Synchronous blocking in async? --> Use non-blocking I/O
```

#### Reading Profiler Output

```bash
# Rust: generate flamegraph
cargo flamegraph --bin my_binary -- --my-args
# Look for wide bars (time) and tall stacks (deep call chains)

# Python: interactive profile browser
python -m cProfile -o profile.prof script.py
python -m pstats profile.prof
# sort by cumulative time: sort cumtime

# Go: interactive pprof
go tool pprof cpu_before.prof
# top10, list FunctionName, web (visualize)
```

### Step 3: Fix

**Address the measured bottleneck only.** Do not "clean up" nearby code. Do not optimize multiple things at once.

Common fixes by bottleneck type:

**Algorithmic:**
- Replace O(n^2) with O(n log n) or O(n) where possible
- Use appropriate data structures (hashmap for lookup, sorted array for range queries)
- Hoist loop invariants, eliminate redundant computation

**Memory:**
- Pre-allocate buffers to known sizes
- Reuse allocations in hot loops (object pools, arena allocators)
- Stream large data instead of loading entirely into memory

**I/O and Database:**
- Add indexes for filtered/sorted columns
- Replace N+1 queries with joins or batch loading
- Paginate unbounded queries
- Use connection pooling

**Concurrency:**
- Replace global locks with fine-grained or reader-writer locks
- Use lock-free data structures for contended hot paths
- Move CPU-bound work to dedicated threads/processes

### Step 4: Verify

**Re-measure with the same methodology as Step 1.** Compare directly.

```bash
# Rust: compare against saved baseline
cargo bench -- --baseline before

# Python: compare profiles
python -m cProfile -s cumtime script.py > profile_after.txt
diff profile_before.txt profile_after.txt

# Go: compare benchmarks
go test -bench=BenchmarkHotPath -count=5 ./... > after.txt
benchstat before.txt after.txt

# General: side-by-side comparison
hyperfine --warmup 3 './old_command' './new_command'
```

**Decision gate:**
- Measurably faster (statistically significant) --> proceed to Step 5
- Not measurably faster --> **revert the change**. It added complexity without benefit
- Faster but broke correctness --> **revert**. Correctness always wins

### Step 5: Guard

**Add a regression guard so the improvement sticks.**

```bash
# Rust: criterion benchmark checked in CI
# Add benchmark to benches/ directory, run in CI pipeline

# Python: pytest-benchmark with --benchmark-autosave
pytest --benchmark-autosave tests/benchmarks/

# Go: benchmark test committed alongside unit tests
# go test -bench=. runs automatically with test suite

# General: performance budget in CI
# Fail the build if response time > threshold
```

Guard types (choose at least one):
- **Benchmark test**: committed to repo, runs in CI, fails on regression
- **Performance budget**: threshold in CI config (e.g., "p95 < 200ms")
- **Monitoring alert**: production metric alert for latency/throughput
- **Size budget**: binary/artifact size threshold

For CI performance gates, see the `flow-code-cicd` skill.

## Amdahl's Law Reminder

```
Speedup = 1 / ((1 - P) + P/S)

P = fraction of time in the bottleneck
S = speedup factor of your optimization

Example: Bottleneck is 10% of runtime. You make it 10x faster.
Speedup = 1 / (0.9 + 0.1/10) = 1 / 0.91 = 1.10x (only 10% faster!)

Lesson: Only the dominant bottleneck matters. A 2x speedup on 80% of
runtime beats a 100x speedup on 5% of runtime.
```

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "I know where the bottleneck is" | Profile first. Intuition is wrong >50% of the time. The actual bottleneck surprises even experienced engineers. |
| "This optimization is obvious" | Obvious optimizations often aren't. Compilers and runtimes already optimize the obvious stuff. Measure to confirm. |
| "We don't have time to benchmark" | You don't have time to optimize blindly either. A 10-minute benchmark saves hours of wasted effort on the wrong bottleneck. |
| "It's fast enough" | Without a baseline, you don't know what "enough" means. Define the target, measure against it. |
| "Let's optimize everything" | Amdahl's Law: only the bottleneck matters. Optimizing non-bottleneck code adds complexity for zero user-visible improvement. |
| "We'll add benchmarks later" | Later never comes. The regression ships silently, and you re-discover it in production under load. |
| "The fix is small, no need to re-measure" | Small fixes can have surprising effects (positive or negative). The measurement takes minutes. Just do it. |

## Red Flags

- Optimizing without profiling data to justify the target
- Multiple optimizations applied simultaneously (can't isolate which helped)
- No before/after numbers in the commit message or PR description
- Optimization that makes code significantly harder to read without measured justification
- "Performance refactor" that changes architecture without baseline measurements
- Benchmarks that don't use warm-up runs or statistical methods (single-run timings are noise)
- Reverting to debug builds or removing optimizations "temporarily" for debugging

## Verification

After any performance-related change, confirm:

- [ ] Baseline measurements captured before optimization (specific numbers, not "it felt slow")
- [ ] Profiler identified the actual bottleneck (not assumed from code reading)
- [ ] After-measurements show statistically significant improvement over baseline
- [ ] Regression guard added (benchmark test, CI budget, or monitoring alert)
- [ ] Optimization did not break correctness (full test suite passes)
- [ ] Before/after numbers documented in commit message or PR description
- [ ] No unrelated "while I'm here" optimizations bundled in the same change

## References

- Detailed checklist: `references/performance-checklist.md`
- For CI performance gates, see the `flow-code-cicd` skill
- For debugging performance issues, see the `flow-code-debug` skill
