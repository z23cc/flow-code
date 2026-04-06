# Performance Checklist

Quick reference for performance in flow-code. Rust backend with libSQL, async runtime.

## Measurement Commands

```bash
# Rust benchmarks (add benches/ directory with criterion)
cargo bench

# Criterion (statistical benchmarking)
cargo bench --bench my_benchmark

# Flamegraph (CPU profiling)
cargo install flamegraph
cargo flamegraph --bin flowctl -- status --json

# Memory profiling (Linux)
valgrind --tool=massif target/release/flowctl status --json

# Binary size
ls -lh target/release/flowctl
cargo bloat --release --crates

# Compile time
cargo build --release --timings
```

| Tool | What It Measures | When to Use |
|---|---|---|
| `cargo bench` (criterion) | Function-level latency | Before/after optimization |
| `cargo flamegraph` | CPU hotspots | Investigating slow commands |
| `cargo bloat` | Binary size by crate | When binary grows unexpectedly |
| `valgrind --tool=massif` | Heap allocations | Investigating memory usage |
| `hyperfine` | CLI command latency | Comparing command performance |

## Database Anti-Patterns (libSQL)

| Anti-Pattern | Impact | Fix |
|---|---|---|
| N+1 queries | Linear DB round-trips | Use JOINs or batch queries |
| Unbounded queries | Memory exhaustion, timeouts | Always use LIMIT, paginate |
| Missing indexes | Slow reads as data grows | Add indexes on filtered/sorted columns |
| No connection pooling | Connection overhead per query | Reuse connections |
| SELECT * | Fetches unused columns | Select only needed columns |
| String concatenation in SQL | Injection risk + no query cache | Use parameterized queries |
| No transactions for batch ops | Partial failures, inconsistency | Wrap in BEGIN/COMMIT |

## Backend CLI Checklist

| Area | Target | How to Verify |
|---|---|---|
| Command startup | < 50ms | `hyperfine 'flowctl status --json'` |
| Task list (100 tasks) | < 200ms | Benchmark with test data |
| Database open | < 20ms | Profile with `tracing` spans |
| JSON serialization | < 5ms for typical output | `cargo bench` |
| Binary size | < 20MB release | `ls -lh target/release/flowctl` |
| Memory usage (idle) | < 10MB | `valgrind` or `/usr/bin/time -v` |

## Rust-Specific Performance

### Async/Await Patterns

| Do | Don't |
|---|---|
| Use `tokio::spawn` for concurrent I/O | Block the async runtime with sync I/O |
| Use `tokio::task::spawn_blocking` for CPU work | Run CPU-heavy code in async context |
| Use `tokio::join!` for parallel awaits | Await sequentially when independent |
| Use bounded channels for backpressure | Use unbounded channels (memory leak risk) |

```rust
// Good: parallel async operations
let (tasks, epics) = tokio::join!(
    db.list_tasks(),
    db.list_epics()
);

// Bad: sequential when independent
let tasks = db.list_tasks().await?;
let epics = db.list_epics().await?;  // Waits unnecessarily
```

### Memory Patterns

| Do | Don't |
|---|---|
| Use `&str` over `String` where possible | Clone strings unnecessarily |
| Use `Cow<str>` for maybe-owned data | Allocate when borrowing suffices |
| Pre-allocate with `Vec::with_capacity` | Push to vec without size hint |
| Use iterators over collecting into vecs | `collect()` when you can chain |
| Use `Arc` for shared read-only data | Clone large structs across threads |

```rust
// Good: pre-allocate
let mut results = Vec::with_capacity(tasks.len());

// Bad: repeated reallocation
let mut results = Vec::new();  // Grows 1, 2, 4, 8...
```

### String Performance

| Do | Don't |
|---|---|
| Use `write!` or `format!` once | Repeated `push_str` / `+` concatenation |
| Use `String::with_capacity` for building | Build strings without size hint |
| Return `impl Display` for lazy formatting | Format eagerly when output may not be used |
| Use `serde_json::to_writer` for streaming | Serialize to String then write |

## Tokio Runtime

| Setting | Recommendation |
|---|---|
| Runtime flavor | `current_thread` for CLI (lower overhead) |
| Worker threads | Default for server, 1 for CLI tools |
| Blocking threads | Default (512), reduce for CLI |
| Task budget | Let tokio manage; avoid manual yielding |

```rust
// CLI tool: single-threaded runtime (faster startup)
#[tokio::main(flavor = "current_thread")]
async fn main() { ... }

// Server: multi-threaded (default)
#[tokio::main]
async fn main() { ... }
```

## Performance Anti-Patterns

| Anti-Pattern | Impact | Fix |
|---|---|---|
| Blocking async runtime | Starves other tasks | Use `spawn_blocking` |
| N+1 database queries | Linear latency growth | Batch or JOIN queries |
| Cloning large structs | Memory + CPU waste | Use references or `Arc` |
| Unbounded collections | Memory exhaustion | Add limits, use streaming |
| Sync file I/O in async | Blocks executor thread | Use `tokio::fs` |
| Excessive logging in hot path | I/O overhead | Use log levels, sample |
| Compiling in debug mode for perf tests | 10-100x slower | Always `--release` |
| Not reusing DB connections | Connection overhead | Use connection pool |
| Serializing then parsing JSON | Double work | Pass typed structs |
| Formatting strings nobody reads | CPU waste | Lazy formatting with `Display` |

## Profiling Workflow

| Step | Command | Purpose |
|---|---|---|
| 1. Baseline | `hyperfine 'flowctl status --json'` | Measure current perf |
| 2. Profile | `cargo flamegraph --bin flowctl -- status` | Find hotspots |
| 3. Optimize | Edit code targeting hotspot | Reduce cost |
| 4. Verify | Re-run baseline command | Confirm improvement |
| 5. Benchmark | `cargo bench` | Prevent regression |
