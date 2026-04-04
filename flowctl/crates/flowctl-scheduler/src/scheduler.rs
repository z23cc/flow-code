//! DAG-driven task scheduler with bounded parallelism.
//!
//! Uses `TaskDag` from flowctl-core for dependency resolution. Tasks are
//! dispatched as `tokio::spawn` handles, bounded by a `Semaphore` (--jobs N).
//! On completion the DAG is queried for newly-ready tasks and they are
//! dispatched immediately.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use flowctl_core::dag::TaskDag;
use flowctl_core::state_machine::Status;

use crate::circuit_breaker::CircuitBreaker;

/// Per-domain historical performance data used for adaptive parallelism.
#[derive(Debug, Clone)]
pub struct DomainPerf {
    /// Number of completed tasks with duration history.
    pub completed_count: i64,
    /// Average task duration in seconds.
    pub avg_duration_secs: f64,
}

/// Configuration for domain-based adaptive parallelism.
///
/// When a domain has >= `min_samples` completed tasks with duration history,
/// the scheduler adjusts the effective semaphore size for tasks in that domain.
/// Domains below the threshold use the static `SchedulerConfig::max_parallel`.
#[derive(Debug, Clone)]
pub struct AdaptiveConfig {
    /// Minimum completed tasks before adaptive sizing kicks in.
    pub min_samples: i64,
    /// Absolute floor for computed parallelism (never go below this).
    pub min_parallel: usize,
    /// Absolute ceiling for computed parallelism (never exceed this).
    pub max_parallel: usize,
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self {
            min_samples: 5,
            min_parallel: 1,
            max_parallel: 8,
        }
    }
}

/// Result of a single task execution.
#[derive(Debug, Clone)]
pub struct TaskResult {
    /// Task ID that completed.
    pub task_id: String,
    /// Whether the task succeeded.
    pub success: bool,
    /// Optional error message on failure.
    pub error: Option<String>,
}

/// Configuration for the scheduler.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Maximum concurrent tasks (--jobs N).
    pub max_parallel: usize,
    /// Maximum retries per task before marking failed.
    pub max_retries: u32,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_parallel: 4,
            max_retries: 2,
        }
    }
}

/// Callback type for executing a single task. The scheduler calls this
/// for each dispatched task; the implementation should do the actual work
/// and return a `TaskResult`.
pub type TaskExecutor = Arc<dyn Fn(String) -> TaskResult + Send + Sync>;

/// DAG scheduler: discovers ready tasks, dispatches them with bounded
/// parallelism, and feeds completions back to discover the next wave.
///
/// When CPM weights are provided, ready tasks are dispatched in descending
/// order of their CPM distance (longest remaining path). This ensures that
/// tasks on the critical path are started first, minimizing total makespan.
pub struct Scheduler {
    /// The task dependency graph.
    dag: TaskDag,
    /// Current status of every task.
    statuses: HashMap<String, Status>,
    /// Per-task retry counts.
    retries: HashMap<String, u32>,
    /// Configuration.
    config: SchedulerConfig,
    /// Cooperative cancellation token shared with all spawned tasks.
    cancel: CancellationToken,
    /// Circuit breaker for consecutive-failure detection.
    circuit_breaker: CircuitBreaker,
    /// CPM priorities: task_id -> distance on longest weighted path.
    /// Tasks with higher values are dispatched first.
    cpm_priorities: HashMap<String, f64>,
    /// Per-domain historical performance for adaptive parallelism.
    domain_perf: HashMap<String, DomainPerf>,
    /// Adaptive parallelism config. `None` means use static `max_parallel`.
    adaptive_config: Option<AdaptiveConfig>,
    /// Task-id to domain mapping (set alongside domain_perf).
    task_domains: HashMap<String, String>,
}

impl Scheduler {
    /// Create a new scheduler from a DAG, initial statuses, and config.
    pub fn new(
        dag: TaskDag,
        statuses: HashMap<String, Status>,
        config: SchedulerConfig,
        cancel: CancellationToken,
        circuit_breaker: CircuitBreaker,
    ) -> Self {
        Self {
            dag,
            statuses,
            retries: HashMap::new(),
            config,
            cancel,
            circuit_breaker,
            cpm_priorities: HashMap::new(),
            domain_perf: HashMap::new(),
            adaptive_config: None,
            task_domains: HashMap::new(),
        }
    }

    /// Set CPM weights and compute dispatch priorities.
    ///
    /// Call this before `run()` to enable CPM-based dispatch ordering.
    /// Tasks with higher CPM distance (more downstream work) are dispatched first.
    pub fn set_cpm_weights(&mut self, weights: &HashMap<String, f64>) {
        self.cpm_priorities = self.dag.cpm_priorities(weights);
    }

    /// Enable adaptive parallelism with per-domain performance data.
    ///
    /// Call this before `run()`. For each domain with enough history
    /// (>= `adaptive_config.min_samples`), the scheduler will compute an
    /// effective parallelism level based on average task duration:
    ///
    /// - Short-duration domains (fast tasks) get higher parallelism.
    /// - Long-duration domains (slow tasks) get lower parallelism.
    ///
    /// `task_domains` maps task_id -> domain string so the scheduler knows
    /// which domain each task belongs to at dispatch time.
    pub fn set_adaptive(
        &mut self,
        config: AdaptiveConfig,
        domain_perf: HashMap<String, DomainPerf>,
        task_domains: HashMap<String, String>,
    ) {
        self.domain_perf = domain_perf;
        self.task_domains = task_domains;
        self.adaptive_config = Some(config);
    }

    /// Compute effective parallelism for a domain.
    ///
    /// If adaptive is not configured or the domain lacks sufficient samples,
    /// returns `self.config.max_parallel` (the static fallback).
    ///
    /// The heuristic: normalize domain durations relative to the global mean
    /// across all warm domains. Domains with below-average duration get more
    /// slots; above-average get fewer. The ratio is clamped to
    /// `[adaptive.min_parallel, adaptive.max_parallel]`.
    fn effective_parallelism(&self, domain: Option<&str>) -> usize {
        let adaptive = match &self.adaptive_config {
            Some(c) => c,
            None => return self.config.max_parallel,
        };

        let domain_key = match domain {
            Some(d) => d,
            None => return self.config.max_parallel,
        };

        let perf = match self.domain_perf.get(domain_key) {
            Some(p) if p.completed_count >= adaptive.min_samples => p,
            _ => return self.config.max_parallel, // cold start
        };

        // Global mean across all warm domains.
        let warm: Vec<&DomainPerf> = self
            .domain_perf
            .values()
            .filter(|p| p.completed_count >= adaptive.min_samples)
            .collect();

        if warm.is_empty() {
            return self.config.max_parallel;
        }

        let global_mean: f64 =
            warm.iter().map(|p| p.avg_duration_secs).sum::<f64>() / warm.len() as f64;

        if global_mean <= 0.0 || perf.avg_duration_secs <= 0.0 {
            return self.config.max_parallel;
        }

        // ratio > 1 means domain is faster than average -> more slots.
        let ratio = global_mean / perf.avg_duration_secs;
        let base = self.config.max_parallel as f64;
        let computed = (base * ratio).round() as usize;

        computed.clamp(adaptive.min_parallel, adaptive.max_parallel)
    }

    /// Run the scheduling loop to completion.
    ///
    /// Returns the final status map when all tasks are done/failed or the
    /// circuit breaker trips.
    pub async fn run<F, Fut>(&mut self, executor: F) -> HashMap<String, Status>
    where
        F: Fn(String, CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = TaskResult> + Send + 'static,
    {
        // Build per-domain semaphores when adaptive config is present,
        // otherwise a single shared semaphore for all tasks.
        let domain_semaphores = self.build_domain_semaphores();
        let default_semaphore = Arc::new(Semaphore::new(self.config.max_parallel));
        let (result_tx, mut result_rx) = mpsc::unbounded_channel::<TaskResult>();

        let executor = Arc::new(executor);
        let mut in_flight: usize = 0;

        // Initial wave: discover all ready tasks, sorted by CPM priority (highest first).
        let mut ready = self.dag.ready_tasks(&self.statuses);
        self.sort_by_cpm(&mut ready);
        for task_id in ready {
            if self.cancel.is_cancelled() || self.circuit_breaker.is_open() {
                break;
            }
            let sem = self.semaphore_for_task(&task_id, &domain_semaphores, &default_semaphore);
            self.dispatch_task(
                &task_id,
                &sem,
                &result_tx,
                &executor,
            );
            in_flight += 1;
        }

        // Main loop: wait for completions and dispatch newly-ready tasks.
        while in_flight > 0 {
            let result = tokio::select! {
                _ = self.cancel.cancelled() => {
                    info!("scheduler cancelled, draining in-flight tasks");
                    break;
                }
                res = result_rx.recv() => {
                    match res {
                        Some(r) => r,
                        None => break,
                    }
                }
            };

            in_flight -= 1;
            self.handle_result(&result);

            // Check circuit breaker after handling the result.
            if self.circuit_breaker.is_open() {
                warn!("circuit breaker open, cancelling all in-flight tasks");
                self.cancel.cancel();
                break;
            }

            // Re-dispatch if task is UpForRetry (retry after failure).
            if self.statuses.get(&result.task_id).copied() == Some(Status::UpForRetry)
                && !self.cancel.is_cancelled() && !self.circuit_breaker.is_open() {
                    let sem = self.semaphore_for_task(&result.task_id, &domain_semaphores, &default_semaphore);
                    self.dispatch_task(&result.task_id, &sem, &result_tx, &executor);
                    in_flight += 1;
                }

            // Discover newly-ready tasks, sorted by CPM priority.
            let mut newly_ready = self.dag.complete(&result.task_id, &self.statuses);
            self.sort_by_cpm(&mut newly_ready);
            for task_id in newly_ready {
                if self.cancel.is_cancelled() || self.circuit_breaker.is_open() {
                    break;
                }
                // Dispatch if task is Todo or UpForRetry (retry after failure).
                let status = self.statuses.get(&task_id).copied().unwrap_or(Status::Todo);
                if status == Status::Todo || status == Status::UpForRetry {
                    let sem = self.semaphore_for_task(&task_id, &domain_semaphores, &default_semaphore);
                    self.dispatch_task(
                        &task_id,
                        &sem,
                        &result_tx,
                        &executor,
                    );
                    in_flight += 1;
                }
            }
        }

        self.statuses.clone()
    }

    /// Build per-domain semaphores based on adaptive config and domain perf data.
    /// Returns an empty map when adaptive is not configured.
    fn build_domain_semaphores(&self) -> HashMap<String, Arc<Semaphore>> {
        let mut map = HashMap::new();
        if self.adaptive_config.is_none() {
            return map;
        }
        // Create a semaphore for each domain that has perf data.
        let domains: std::collections::HashSet<&String> = self.task_domains.values().collect();
        for domain in domains {
            let par = self.effective_parallelism(Some(domain));
            debug!(domain = %domain, parallelism = par, "adaptive semaphore");
            map.insert(domain.clone(), Arc::new(Semaphore::new(par)));
        }
        map
    }

    /// Pick the right semaphore for a task: domain-specific if available,
    /// otherwise the default.
    fn semaphore_for_task(
        &self,
        task_id: &str,
        domain_semaphores: &HashMap<String, Arc<Semaphore>>,
        default: &Arc<Semaphore>,
    ) -> Arc<Semaphore> {
        if let Some(domain) = self.task_domains.get(task_id) {
            if let Some(sem) = domain_semaphores.get(domain) {
                return sem.clone();
            }
        }
        default.clone()
    }

    /// Dispatch a single task as a tokio::spawn with semaphore-bounded parallelism.
    fn dispatch_task<F, Fut>(
        &mut self,
        task_id: &str,
        semaphore: &Arc<Semaphore>,
        result_tx: &mpsc::UnboundedSender<TaskResult>,
        executor: &Arc<F>,
    ) where
        F: Fn(String, CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = TaskResult> + Send + 'static,
    {
        debug!(task_id, "dispatching task");
        self.statuses.insert(task_id.to_string(), Status::InProgress);

        let sem = semaphore.clone();
        let tx = result_tx.clone();
        let exec = executor.clone();
        let id = task_id.to_string();
        let child_token = self.cancel.child_token();

        tokio::spawn(async move {
            // Acquire semaphore permit (bounded parallelism).
            let _permit = sem.acquire().await;
            if child_token.is_cancelled() {
                let _ = tx.send(TaskResult {
                    task_id: id,
                    success: false,
                    error: Some("cancelled".to_string()),
                });
                return;
            }

            let result = exec(id, child_token).await;
            let _ = tx.send(result);
        });
    }

    /// Handle a completed task result: update statuses, retries, and
    /// propagate failures.
    fn handle_result(&mut self, result: &TaskResult) {
        if result.success {
            info!(task_id = %result.task_id, "task completed successfully");
            self.statuses.insert(result.task_id.clone(), Status::Done);
            self.circuit_breaker.record_success();
        } else {
            let retries = self.retries.entry(result.task_id.clone()).or_insert(0);
            *retries += 1;

            if *retries <= self.config.max_retries {
                info!(
                    task_id = %result.task_id,
                    attempt = *retries,
                    max = self.config.max_retries,
                    "task failed, marking up_for_retry"
                );
                self.statuses.insert(result.task_id.clone(), Status::UpForRetry);
                // UpForRetry is recognized by dispatch_ready as dispatchable
                // (transitions UpForRetry → InProgress on next dispatch cycle).
            } else {
                warn!(
                    task_id = %result.task_id,
                    error = ?result.error,
                    "task failed permanently"
                );
                self.statuses.insert(result.task_id.clone(), Status::Failed);
                self.circuit_breaker.record_failure();

                // Propagate failure to downstream tasks.
                let affected = self.dag.propagate_failure(&result.task_id);
                for downstream_id in affected {
                    self.statuses.insert(downstream_id, Status::UpstreamFailed);
                }
            }
        }
    }

    /// Sort task IDs by CPM priority (descending). Tasks with higher CPM
    /// distance are placed first so they get dispatched/semaphore-acquired
    /// earlier, reducing total makespan.
    fn sort_by_cpm(&self, tasks: &mut [String]) {
        if self.cpm_priorities.is_empty() {
            return; // No CPM data — keep default alphabetical order.
        }
        tasks.sort_by(|a, b| {
            let pa = self.cpm_priorities.get(a).copied().unwrap_or(0.0);
            let pb = self.cpm_priorities.get(b).copied().unwrap_or(0.0);
            pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Get a snapshot of current statuses.
    pub fn statuses(&self) -> &HashMap<String, Status> {
        &self.statuses
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flowctl_core::types::{Domain, Task};
    use chrono::Utc;

    fn make_task(id: &str, deps: &[&str]) -> Task {
        Task {
            schema_version: 1,
            id: id.to_string(),
            epic: "test-epic".to_string(),
            title: format!("Task {id}"),
            status: Status::Todo,
            priority: None,
            domain: Domain::General,
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            files: vec![],
            r#impl: None,
            review: None,
            sync: None,
            file_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn status_map(entries: &[(&str, Status)]) -> HashMap<String, Status> {
        entries.iter().map(|(id, s)| (id.to_string(), *s)).collect()
    }

    #[tokio::test]
    async fn test_scheduler_all_succeed() {
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &["a"]),
            make_task("c", &["a"]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let statuses = status_map(&[
            ("a", Status::Todo),
            ("b", Status::Todo),
            ("c", Status::Todo),
        ]);
        let cancel = CancellationToken::new();
        let cb = CircuitBreaker::new(5);

        let mut scheduler = Scheduler::new(dag, statuses, SchedulerConfig::default(), cancel, cb);

        let final_statuses = scheduler
            .run(|task_id, _cancel| async move {
                TaskResult {
                    task_id,
                    success: true,
                    error: None,
                }
            })
            .await;

        assert_eq!(final_statuses["a"], Status::Done);
        assert_eq!(final_statuses["b"], Status::Done);
        assert_eq!(final_statuses["c"], Status::Done);
    }

    #[tokio::test]
    async fn test_scheduler_failure_propagates() {
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &["a"]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let statuses = status_map(&[
            ("a", Status::Todo),
            ("b", Status::Todo),
        ]);
        let cancel = CancellationToken::new();
        let cb = CircuitBreaker::new(5);

        let config = SchedulerConfig {
            max_parallel: 4,
            max_retries: 0, // No retries
        };
        let mut scheduler = Scheduler::new(dag, statuses, config, cancel, cb);

        let final_statuses = scheduler
            .run(|task_id, _cancel| async move {
                TaskResult {
                    task_id,
                    success: false,
                    error: Some("boom".to_string()),
                }
            })
            .await;

        assert_eq!(final_statuses["a"], Status::Failed);
        assert_eq!(final_statuses["b"], Status::UpstreamFailed);
    }

    #[tokio::test]
    async fn test_scheduler_respects_cancellation() {
        let tasks = vec![make_task("a", &[])];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let statuses = status_map(&[("a", Status::Todo)]);
        let cancel = CancellationToken::new();
        let cb = CircuitBreaker::new(5);

        // Cancel immediately.
        cancel.cancel();

        let mut scheduler = Scheduler::new(
            dag,
            statuses,
            SchedulerConfig::default(),
            cancel,
            cb,
        );

        let final_statuses = scheduler
            .run(|task_id, _cancel| async move {
                TaskResult {
                    task_id,
                    success: true,
                    error: None,
                }
            })
            .await;

        // Task should not have completed since we cancelled before dispatch.
        assert_ne!(final_statuses.get("a").copied(), Some(Status::Done));
    }

    #[tokio::test]
    async fn test_scheduler_bounded_parallelism() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &[]),
            make_task("c", &[]),
            make_task("d", &[]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let statuses = status_map(&[
            ("a", Status::Todo),
            ("b", Status::Todo),
            ("c", Status::Todo),
            ("d", Status::Todo),
        ]);
        let cancel = CancellationToken::new();
        let cb = CircuitBreaker::new(5);
        let config = SchedulerConfig {
            max_parallel: 2,
            max_retries: 0,
        };

        let peak = Arc::new(AtomicUsize::new(0));
        let current = Arc::new(AtomicUsize::new(0));

        let peak_clone = peak.clone();
        let current_clone = current.clone();

        let mut scheduler = Scheduler::new(dag, statuses, config, cancel, cb);

        scheduler
            .run(move |task_id, _cancel| {
                let p = peak_clone.clone();
                let c = current_clone.clone();
                async move {
                    let val = c.fetch_add(1, Ordering::SeqCst) + 1;
                    p.fetch_max(val, Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    c.fetch_sub(1, Ordering::SeqCst);
                    TaskResult {
                        task_id,
                        success: true,
                        error: None,
                    }
                }
            })
            .await;

        assert!(peak.load(Ordering::SeqCst) <= 2);
    }

    #[test]
    fn test_effective_parallelism_cold_start() {
        let tasks = vec![make_task("a", &[])];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let statuses = status_map(&[("a", Status::Todo)]);
        let cancel = CancellationToken::new();
        let cb = CircuitBreaker::new(5);
        let config = SchedulerConfig { max_parallel: 4, max_retries: 0 };

        let mut scheduler = Scheduler::new(dag, statuses, config, cancel, cb);

        // No adaptive config -> always returns static max_parallel.
        assert_eq!(scheduler.effective_parallelism(Some("backend")), 4);

        // With adaptive config but no perf data -> cold start fallback.
        scheduler.set_adaptive(
            AdaptiveConfig::default(),
            HashMap::new(),
            HashMap::new(),
        );
        assert_eq!(scheduler.effective_parallelism(Some("backend")), 4);
    }

    #[test]
    fn test_effective_parallelism_warm_domains() {
        let tasks = vec![make_task("a", &[])];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let statuses = status_map(&[("a", Status::Todo)]);
        let cancel = CancellationToken::new();
        let cb = CircuitBreaker::new(5);
        let config = SchedulerConfig { max_parallel: 4, max_retries: 0 };

        let mut scheduler = Scheduler::new(dag, statuses, config, cancel, cb);

        let mut domain_perf = HashMap::new();
        // "frontend" tasks are fast (avg 30s), "backend" tasks are slow (avg 120s).
        domain_perf.insert("frontend".to_string(), DomainPerf {
            completed_count: 10,
            avg_duration_secs: 30.0,
        });
        domain_perf.insert("backend".to_string(), DomainPerf {
            completed_count: 8,
            avg_duration_secs: 120.0,
        });

        let task_domains = HashMap::from([
            ("a".to_string(), "frontend".to_string()),
        ]);

        scheduler.set_adaptive(
            AdaptiveConfig {
                min_samples: 5,
                min_parallel: 1,
                max_parallel: 8,
            },
            domain_perf,
            task_domains,
        );

        // Global mean = (30 + 120) / 2 = 75.
        // Frontend ratio = 75/30 = 2.5, computed = 4 * 2.5 = 10 -> clamped to 8.
        assert_eq!(scheduler.effective_parallelism(Some("frontend")), 8);
        // Backend ratio = 75/120 = 0.625, computed = 4 * 0.625 = 2.5 -> round = 3.
        assert_eq!(scheduler.effective_parallelism(Some("backend")), 3);
    }

    #[test]
    fn test_effective_parallelism_below_threshold() {
        let tasks = vec![make_task("a", &[])];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let statuses = status_map(&[("a", Status::Todo)]);
        let cancel = CancellationToken::new();
        let cb = CircuitBreaker::new(5);
        let config = SchedulerConfig { max_parallel: 4, max_retries: 0 };

        let mut scheduler = Scheduler::new(dag, statuses, config, cancel, cb);

        let mut domain_perf = HashMap::new();
        // Only 3 samples — below default min_samples of 5.
        domain_perf.insert("testing".to_string(), DomainPerf {
            completed_count: 3,
            avg_duration_secs: 60.0,
        });

        scheduler.set_adaptive(
            AdaptiveConfig::default(),
            domain_perf,
            HashMap::new(),
        );

        // Below threshold -> cold start fallback.
        assert_eq!(scheduler.effective_parallelism(Some("testing")), 4);
    }

    #[tokio::test]
    async fn test_adaptive_scheduler_runs_to_completion() {
        // Verify the scheduler completes all tasks when adaptive is enabled.
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &[]),
            make_task("c", &["a"]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let statuses = status_map(&[
            ("a", Status::Todo),
            ("b", Status::Todo),
            ("c", Status::Todo),
        ]);
        let cancel = CancellationToken::new();
        let cb = CircuitBreaker::new(5);
        let config = SchedulerConfig { max_parallel: 4, max_retries: 0 };

        let mut scheduler = Scheduler::new(dag, statuses, config, cancel, cb);

        let mut domain_perf = HashMap::new();
        domain_perf.insert("general".to_string(), DomainPerf {
            completed_count: 10,
            avg_duration_secs: 60.0,
        });
        let task_domains = HashMap::from([
            ("a".to_string(), "general".to_string()),
            ("b".to_string(), "general".to_string()),
            ("c".to_string(), "general".to_string()),
        ]);
        scheduler.set_adaptive(AdaptiveConfig::default(), domain_perf, task_domains);

        let final_statuses = scheduler
            .run(|task_id, _cancel| async move {
                TaskResult { task_id, success: true, error: None }
            })
            .await;

        assert_eq!(final_statuses["a"], Status::Done);
        assert_eq!(final_statuses["b"], Status::Done);
        assert_eq!(final_statuses["c"], Status::Done);
    }
}
