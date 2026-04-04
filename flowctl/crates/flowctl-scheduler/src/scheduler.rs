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
        }
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
        let semaphore = Arc::new(Semaphore::new(self.config.max_parallel));
        let (result_tx, mut result_rx) = mpsc::unbounded_channel::<TaskResult>();

        let executor = Arc::new(executor);
        let mut in_flight: usize = 0;

        // Initial wave: discover all ready tasks.
        let ready = self.dag.ready_tasks(&self.statuses);
        for task_id in ready {
            if self.cancel.is_cancelled() || self.circuit_breaker.is_open() {
                break;
            }
            self.dispatch_task(
                &task_id,
                &semaphore,
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

            // Discover newly-ready tasks.
            let newly_ready = self.dag.complete(&result.task_id, &self.statuses);
            for task_id in newly_ready {
                if self.cancel.is_cancelled() || self.circuit_breaker.is_open() {
                    break;
                }
                // Only dispatch if the task is actually Todo.
                if self.statuses.get(&task_id).copied().unwrap_or(Status::Todo) == Status::Todo {
                    self.dispatch_task(
                        &task_id,
                        &semaphore,
                        &result_tx,
                        &executor,
                    );
                    in_flight += 1;
                }
            }
        }

        self.statuses.clone()
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
                // Reset to Todo so it can be re-dispatched.
                self.statuses.insert(result.task_id.clone(), Status::Todo);
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
}
