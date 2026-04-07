//! Async repository abstractions over libSQL.
//!
//! Ported from `flowctl-db::repo` (sync rusqlite). Each repo owns a
//! `libsql::Connection` (cheap Clone) and exposes async methods that
//! return `DbError`. Mirrors the sync API surface where it makes sense.

mod deps;
mod epic;
mod event;
mod evidence;
mod file_lock;
mod file_ownership;
mod gap;
pub(crate) mod helpers;
mod phase_progress;
mod runtime;
mod scout_cache;
mod task;

pub use deps::DepRepo;
pub use epic::EpicRepo;
pub use event::{EventRepo, EventRow};
pub use evidence::EvidenceRepo;
pub use file_lock::{FileLockRepo, LockEntry, LockMode};
pub use file_ownership::FileOwnershipRepo;
pub use gap::{GapRepo, GapRow};
pub use helpers::{max_epic_num, max_task_num};
pub use phase_progress::PhaseProgressRepo;
pub use runtime::RuntimeRepo;
pub use scout_cache::ScoutCacheRepo;
pub use task::TaskRepo;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DbError;
    use crate::pool::open_memory_async;
    use chrono::Utc;
    use flowctl_core::types::{Domain, Epic, EpicStatus, Evidence, ReviewStatus, RuntimeState, Task};
    use flowctl_core::state_machine::Status;

    fn sample_epic(id: &str) -> Epic {
        let now = Utc::now();
        Epic {
            schema_version: 1,
            id: id.to_string(),
            title: format!("Title of {id}"),
            status: EpicStatus::Open,
            branch_name: Some("feat/x".to_string()),
            plan_review: ReviewStatus::Unknown,
            completion_review: ReviewStatus::Unknown,
            depends_on_epics: Vec::new(),
            default_impl: None,
            default_review: None,
            default_sync: None,
            auto_execute_pending: None,
            auto_execute_set_at: None,
            archived: false,
            file_path: Some(format!("epics/{id}.md")),
            created_at: now,
            updated_at: now,
        }
    }

    fn sample_task(epic: &str, id: &str) -> Task {
        let now = Utc::now();
        Task {
            schema_version: 1,
            id: id.to_string(),
            epic: epic.to_string(),
            title: format!("Task {id}"),
            status: Status::Todo,
            priority: Some(1),
            domain: Domain::Backend,
            depends_on: Vec::new(),
            files: Vec::new(),
            r#impl: None,
            review: None,
            sync: None,
            file_path: Some(format!("tasks/{id}.md")),
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn epic_upsert_get_roundtrip() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EpicRepo::new(conn.clone());

        let e = sample_epic("fn-1-test");
        repo.upsert(&e).await.unwrap();

        let got = repo.get("fn-1-test").await.unwrap();
        assert_eq!(got.id, "fn-1-test");
        assert_eq!(got.title, "Title of fn-1-test");
        assert_eq!(got.branch_name.as_deref(), Some("feat/x"));
        assert!(matches!(got.status, EpicStatus::Open));
    }

    #[tokio::test]
    async fn epic_upsert_with_body_preserves() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EpicRepo::new(conn.clone());
        let e = sample_epic("fn-2-body");

        repo.upsert_with_body(&e, "# Body v1").await.unwrap();
        let (_, body) = repo.get_with_body("fn-2-body").await.unwrap();
        assert_eq!(body, "# Body v1");

        // Empty body preserves existing.
        repo.upsert_with_body(&e, "").await.unwrap();
        let (_, body2) = repo.get_with_body("fn-2-body").await.unwrap();
        assert_eq!(body2, "# Body v1");

        // Non-empty overwrites.
        repo.upsert_with_body(&e, "# Body v2").await.unwrap();
        let (_, body3) = repo.get_with_body("fn-2-body").await.unwrap();
        assert_eq!(body3, "# Body v2");
    }

    #[tokio::test]
    async fn epic_list_and_update_status_and_delete() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EpicRepo::new(conn.clone());

        repo.upsert(&sample_epic("fn-a")).await.unwrap();
        repo.upsert(&sample_epic("fn-b")).await.unwrap();

        let all = repo.list(None).await.unwrap();
        assert_eq!(all.len(), 2);

        repo.update_status("fn-a", EpicStatus::Done).await.unwrap();
        let done = repo.list(Some("done")).await.unwrap();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].id, "fn-a");

        repo.delete("fn-b").await.unwrap();
        let remaining = repo.list(None).await.unwrap();
        assert_eq!(remaining.len(), 1);

        let err = repo.get("nope").await.unwrap_err();
        assert!(matches!(err, DbError::NotFound(_)));
    }

    #[tokio::test]
    async fn epic_get_missing_is_not_found() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EpicRepo::new(conn.clone());
        let err = repo.get("does-not-exist").await.unwrap_err();
        assert!(matches!(err, DbError::NotFound(_)));
    }

    #[tokio::test]
    async fn task_upsert_get_with_deps_and_files() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let erepo = EpicRepo::new(conn.clone());
        erepo.upsert(&sample_epic("fn-1")).await.unwrap();

        let trepo = TaskRepo::new(conn.clone());
        let mut t = sample_task("fn-1", "fn-1.1");
        t.depends_on = vec!["fn-1.0".to_string()];
        t.files = vec!["src/a.rs".to_string(), "src/b.rs".to_string()];
        trepo.upsert(&t).await.unwrap();

        let got = trepo.get("fn-1.1").await.unwrap();
        assert_eq!(got.epic, "fn-1");
        assert_eq!(got.priority, Some(1));
        assert!(matches!(got.domain, Domain::Backend));
        assert_eq!(got.depends_on, vec!["fn-1.0".to_string()]);
        assert_eq!(got.files.len(), 2);
        assert!(got.files.contains(&"src/a.rs".to_string()));
    }

    #[tokio::test]
    async fn task_list_by_epic_status_domain() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let erepo = EpicRepo::new(conn.clone());
        erepo.upsert(&sample_epic("fn-1")).await.unwrap();
        erepo.upsert(&sample_epic("fn-2")).await.unwrap();

        let trepo = TaskRepo::new(conn.clone());
        let mut t1 = sample_task("fn-1", "fn-1.1");
        let mut t2 = sample_task("fn-1", "fn-1.2");
        t2.domain = Domain::Frontend;
        let t3 = sample_task("fn-2", "fn-2.1");
        trepo.upsert(&t1).await.unwrap();
        trepo.upsert(&t2).await.unwrap();
        trepo.upsert(&t3).await.unwrap();

        let ep1 = trepo.list_by_epic("fn-1").await.unwrap();
        assert_eq!(ep1.len(), 2);

        let all = trepo.list_all(None, None).await.unwrap();
        assert_eq!(all.len(), 3);

        let fe = trepo.list_all(None, Some("frontend")).await.unwrap();
        assert_eq!(fe.len(), 1);
        assert_eq!(fe[0].id, "fn-1.2");

        t1.status = Status::Done;
        trepo.upsert(&t1).await.unwrap();
        let done = trepo.list_by_status(Status::Done).await.unwrap();
        assert_eq!(done.len(), 1);

        let todo_fe = trepo
            .list_all(Some("todo"), Some("frontend"))
            .await
            .unwrap();
        assert_eq!(todo_fe.len(), 1);
    }

    #[tokio::test]
    async fn task_update_status_and_delete() {
        let (_db, conn) = open_memory_async().await.unwrap();
        EpicRepo::new(conn.clone())
            .upsert(&sample_epic("fn-1"))
            .await
            .unwrap();

        let trepo = TaskRepo::new(conn.clone());
        let mut t = sample_task("fn-1", "fn-1.1");
        t.depends_on = vec!["fn-1.0".to_string()];
        t.files = vec!["src/a.rs".to_string()];
        trepo.upsert(&t).await.unwrap();

        trepo
            .update_status("fn-1.1", Status::InProgress)
            .await
            .unwrap();
        let got = trepo.get("fn-1.1").await.unwrap();
        assert!(matches!(got.status, Status::InProgress));

        trepo.delete("fn-1.1").await.unwrap();
        assert!(matches!(
            trepo.get("fn-1.1").await.unwrap_err(),
            DbError::NotFound(_)
        ));

        // Update missing -> NotFound.
        let err = trepo
            .update_status("missing", Status::Done)
            .await
            .unwrap_err();
        assert!(matches!(err, DbError::NotFound(_)));
    }

    #[tokio::test]
    async fn dep_repo_add_list_remove() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let deps = DepRepo::new(conn.clone());

        deps.add_task_dep("fn-1.2", "fn-1.1").await.unwrap();
        deps.add_task_dep("fn-1.2", "fn-1.0").await.unwrap();
        // Idempotent.
        deps.add_task_dep("fn-1.2", "fn-1.1").await.unwrap();

        let mut got = deps.list_task_deps("fn-1.2").await.unwrap();
        got.sort();
        assert_eq!(got, vec!["fn-1.0".to_string(), "fn-1.1".to_string()]);

        deps.remove_task_dep("fn-1.2", "fn-1.1").await.unwrap();
        let after = deps.list_task_deps("fn-1.2").await.unwrap();
        assert_eq!(after, vec!["fn-1.0".to_string()]);

        deps.add_epic_dep("fn-2", "fn-1").await.unwrap();
        deps.add_epic_dep("fn-2", "fn-0").await.unwrap();
        let mut elist = deps.list_epic_deps("fn-2").await.unwrap();
        elist.sort();
        assert_eq!(elist, vec!["fn-0".to_string(), "fn-1".to_string()]);

        deps.remove_epic_dep("fn-2", "fn-0").await.unwrap();
        assert_eq!(
            deps.list_epic_deps("fn-2").await.unwrap(),
            vec!["fn-1".to_string()]
        );
    }

    #[tokio::test]
    async fn dep_repo_reverse_deps_and_transitive() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let deps = DepRepo::new(conn.clone());

        // Build chain: fn-1.1 -> fn-1.2 -> fn-1.3, fn-1.1 -> fn-1.4
        deps.add_task_dep("fn-1.2", "fn-1.1").await.unwrap();
        deps.add_task_dep("fn-1.3", "fn-1.2").await.unwrap();
        deps.add_task_dep("fn-1.4", "fn-1.1").await.unwrap();

        // Direct dependents of fn-1.1: fn-1.2 and fn-1.4
        let direct = deps.list_dependents("fn-1.1").await.unwrap();
        assert_eq!(direct, vec!["fn-1.2".to_string(), "fn-1.4".to_string()]);

        // Direct dependents of fn-1.2: fn-1.3
        let direct2 = deps.list_dependents("fn-1.2").await.unwrap();
        assert_eq!(direct2, vec!["fn-1.3".to_string()]);

        // No dependents of fn-1.3
        let direct3 = deps.list_dependents("fn-1.3").await.unwrap();
        assert!(direct3.is_empty());

        // Transitive dependents of fn-1.1: fn-1.2, fn-1.3, fn-1.4
        let all = deps.list_all_dependents("fn-1.1").await.unwrap();
        assert_eq!(
            all,
            vec!["fn-1.2".to_string(), "fn-1.3".to_string(), "fn-1.4".to_string()]
        );

        // Transitive dependents of fn-1.2: fn-1.3
        let all2 = deps.list_all_dependents("fn-1.2").await.unwrap();
        assert_eq!(all2, vec!["fn-1.3".to_string()]);

        // Remove fn-1.2 -> fn-1.1 dep: reverse index should update
        deps.remove_task_dep("fn-1.2", "fn-1.1").await.unwrap();
        let after = deps.list_dependents("fn-1.1").await.unwrap();
        assert_eq!(after, vec!["fn-1.4".to_string()]);

        // Transitive from fn-1.1 no longer includes fn-1.2 or fn-1.3
        let all_after = deps.list_all_dependents("fn-1.1").await.unwrap();
        assert_eq!(all_after, vec!["fn-1.4".to_string()]);
    }

    #[tokio::test]
    async fn file_ownership_repo_roundtrip() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let f = FileOwnershipRepo::new(conn.clone());

        f.add("src/a.rs", "fn-1.1").await.unwrap();
        f.add("src/b.rs", "fn-1.1").await.unwrap();
        f.add("src/a.rs", "fn-1.2").await.unwrap();
        // Idempotent.
        f.add("src/a.rs", "fn-1.1").await.unwrap();

        let mut t1 = f.list_for_task("fn-1.1").await.unwrap();
        t1.sort();
        assert_eq!(t1, vec!["src/a.rs".to_string(), "src/b.rs".to_string()]);

        let mut owners = f.list_for_file("src/a.rs").await.unwrap();
        owners.sort();
        assert_eq!(owners, vec!["fn-1.1".to_string(), "fn-1.2".to_string()]);

        f.remove("src/a.rs", "fn-1.2").await.unwrap();
        let owners2 = f.list_for_file("src/a.rs").await.unwrap();
        assert_eq!(owners2, vec!["fn-1.1".to_string()]);
    }

    // ── RuntimeRepo ─────────────────────────────────────────────────

    #[tokio::test]
    async fn runtime_upsert_get_roundtrip() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = RuntimeRepo::new(conn.clone());
        let now = Utc::now();
        let state = RuntimeState {
            task_id: "fn-1.1".to_string(),
            assignee: Some("worker-1".to_string()),
            claimed_at: Some(now),
            completed_at: None,
            duration_secs: Some(42),
            blocked_reason: None,
            baseline_rev: Some("abc123".to_string()),
            final_rev: None,
            retry_count: 2,
        };
        repo.upsert(&state).await.unwrap();

        let got = repo.get("fn-1.1").await.unwrap().expect("should exist");
        assert_eq!(got.task_id, "fn-1.1");
        assert_eq!(got.assignee.as_deref(), Some("worker-1"));
        assert_eq!(got.duration_secs, Some(42));
        assert_eq!(got.baseline_rev.as_deref(), Some("abc123"));
        assert_eq!(got.retry_count, 2);
        assert!(got.claimed_at.is_some());
        assert!(got.completed_at.is_none());

        // Update (upsert) the same task.
        let updated = RuntimeState {
            retry_count: 3,
            final_rev: Some("def456".to_string()),
            ..state
        };
        repo.upsert(&updated).await.unwrap();
        let got2 = repo.get("fn-1.1").await.unwrap().unwrap();
        assert_eq!(got2.retry_count, 3);
        assert_eq!(got2.final_rev.as_deref(), Some("def456"));
    }

    #[tokio::test]
    async fn runtime_get_missing_returns_none() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = RuntimeRepo::new(conn.clone());
        assert!(repo.get("does-not-exist").await.unwrap().is_none());
    }

    // ── EvidenceRepo ────────────────────────────────────────────────

    #[tokio::test]
    async fn evidence_upsert_get_roundtrip() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EvidenceRepo::new(conn.clone());
        let ev = Evidence {
            commits: vec!["abc123".to_string(), "def456".to_string()],
            tests: vec!["cargo test".to_string(), "bash smoke.sh".to_string()],
            prs: Vec::new(),
            files_changed: Some(5),
            insertions: Some(120),
            deletions: Some(30),
            review_iterations: Some(1),
            workspace_changes: None,
        };
        repo.upsert("fn-1.1", &ev).await.unwrap();

        let got = repo.get("fn-1.1").await.unwrap().expect("should exist");
        assert_eq!(got.commits, vec!["abc123".to_string(), "def456".to_string()]);
        assert_eq!(
            got.tests,
            vec!["cargo test".to_string(), "bash smoke.sh".to_string()]
        );
        assert_eq!(got.files_changed, Some(5));
        assert_eq!(got.insertions, Some(120));
        assert_eq!(got.deletions, Some(30));
        assert_eq!(got.review_iterations, Some(1));
    }

    #[tokio::test]
    async fn evidence_get_missing_returns_none() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EvidenceRepo::new(conn.clone());
        assert!(repo.get("nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn evidence_empty_vecs_roundtrip() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EvidenceRepo::new(conn.clone());
        let ev = Evidence {
            commits: Vec::new(),
            tests: Vec::new(),
            prs: Vec::new(),
            files_changed: None,
            insertions: None,
            deletions: None,
            review_iterations: None,
            workspace_changes: None,
        };
        repo.upsert("fn-2.1", &ev).await.unwrap();
        let got = repo.get("fn-2.1").await.unwrap().unwrap();
        assert!(got.commits.is_empty());
        assert!(got.tests.is_empty());
        assert_eq!(got.files_changed, None);
    }

    // ── FileLockRepo ────────────────────────────────────────────────

    #[tokio::test]
    async fn file_lock_acquire_twice_conflicts() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = FileLockRepo::new(conn.clone());

        repo.acquire("src/a.rs", "fn-1.1", &LockMode::Write).await.unwrap();
        let err = repo.acquire("src/a.rs", "fn-1.2", &LockMode::Write).await.unwrap_err();
        assert!(
            matches!(err, DbError::Constraint(_)),
            "expected Constraint, got {err:?}"
        );
    }

    #[tokio::test]
    async fn file_lock_release_for_task_and_check() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = FileLockRepo::new(conn.clone());

        repo.acquire("src/a.rs", "fn-1.1", &LockMode::Write).await.unwrap();
        repo.acquire("src/b.rs", "fn-1.1", &LockMode::Write).await.unwrap();
        repo.acquire("src/c.rs", "fn-1.2", &LockMode::Write).await.unwrap();

        assert_eq!(
            repo.check("src/a.rs").await.unwrap().as_deref(),
            Some("fn-1.1")
        );
        assert!(repo.check("src/missing.rs").await.unwrap().is_none());

        let n = repo.release_for_task("fn-1.1").await.unwrap();
        assert_eq!(n, 2);
        assert!(repo.check("src/a.rs").await.unwrap().is_none());
        assert!(repo.check("src/b.rs").await.unwrap().is_none());
        // fn-1.2 still holds its lock.
        assert_eq!(
            repo.check("src/c.rs").await.unwrap().as_deref(),
            Some("fn-1.2")
        );

        // Re-acquiring a released file works.
        repo.acquire("src/a.rs", "fn-1.3", &LockMode::Write).await.unwrap();
        assert_eq!(
            repo.check("src/a.rs").await.unwrap().as_deref(),
            Some("fn-1.3")
        );

        // release_all clears remaining locks.
        let n2 = repo.release_all().await.unwrap();
        assert_eq!(n2, 2);
        assert!(repo.check("src/a.rs").await.unwrap().is_none());
        assert!(repo.check("src/c.rs").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn file_lock_idempotent_reacquire() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = FileLockRepo::new(conn.clone());

        // Acquiring the same file for the same task twice should succeed (idempotent).
        repo.acquire("src/a.rs", "fn-1.1", &LockMode::Write).await.unwrap();
        repo.acquire("src/a.rs", "fn-1.1", &LockMode::Write).await.unwrap();
        assert_eq!(
            repo.check("src/a.rs").await.unwrap().as_deref(),
            Some("fn-1.1")
        );
    }

    #[tokio::test]
    async fn file_lock_expired_cleanup() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = FileLockRepo::new(conn.clone());

        // Insert a lock with an already-expired TTL directly.
        let past = (chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
        conn.execute(
            "INSERT INTO file_locks (file_path, task_id, locked_at, holder_pid, expires_at)
             VALUES ('src/expired.rs', 'fn-old', ?1, 99999, ?2)",
            libsql::params![past.clone(), past],
        )
        .await
        .unwrap();

        // The expired lock should be visible before cleanup.
        assert!(repo.check("src/expired.rs").await.unwrap().is_some());

        // cleanup_stale should remove it.
        let cleaned = repo.cleanup_stale().await.unwrap();
        assert!(cleaned >= 1);
        assert!(repo.check("src/expired.rs").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn file_lock_heartbeat_extends_ttl() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = FileLockRepo::new(conn.clone());

        repo.acquire("src/a.rs", "fn-1.1", &LockMode::Write).await.unwrap();
        repo.acquire("src/b.rs", "fn-1.1", &LockMode::Write).await.unwrap();

        let extended = repo.heartbeat("fn-1.1").await.unwrap();
        assert_eq!(extended, 2);

        // Heartbeat on a non-existent task returns 0.
        let none = repo.heartbeat("fn-nonexistent").await.unwrap();
        assert_eq!(none, 0);
    }

    #[tokio::test]
    async fn file_lock_acquire_cleans_expired_before_insert() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = FileLockRepo::new(conn.clone());

        // Insert expired lock for a file.
        let past = (chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
        conn.execute(
            "INSERT INTO file_locks (file_path, task_id, locked_at, holder_pid, expires_at)
             VALUES ('src/a.rs', 'fn-old', ?1, 99999, ?2)",
            libsql::params![past.clone(), past],
        )
        .await
        .unwrap();

        // Acquiring the same file should succeed because the old lock is expired.
        repo.acquire("src/a.rs", "fn-1.1", &LockMode::Write).await.unwrap();
        assert_eq!(
            repo.check("src/a.rs").await.unwrap().as_deref(),
            Some("fn-1.1")
        );
    }

    // ── PhaseProgressRepo ───────────────────────────────────────────

    #[tokio::test]
    async fn event_repo_insert_list_by_epic_and_type() {
        let (_db, conn) = open_memory_async().await.unwrap();
        // Need an epic row since events.epic_id is TEXT NOT NULL (no FK but we'll be honest).
        conn.execute(
            "INSERT INTO epics (id, title, status, file_path, created_at, updated_at)
             VALUES ('fn-9-evt', 'Evt Test', 'open', 'e.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            (),
        ).await.unwrap();

        let repo = EventRepo::new(conn.clone());
        let id1 = repo.insert("fn-9-evt", Some("fn-9-evt.1"), "task_started", Some("w1"), None, None).await.unwrap();
        let id2 = repo.insert("fn-9-evt", Some("fn-9-evt.1"), "task_completed", Some("w1"), Some("{}"), None).await.unwrap();
        let id3 = repo.insert("fn-9-evt", Some("fn-9-evt.2"), "task_started", Some("w1"), None, None).await.unwrap();
        assert!(id1 > 0 && id2 > id1 && id3 > id2);

        let by_epic = repo.list_by_epic("fn-9-evt", 10).await.unwrap();
        assert_eq!(by_epic.len(), 3);
        // Most recent first.
        assert_eq!(by_epic[0].id, id3);

        let started = repo.list_by_type("task_started", 10).await.unwrap();
        assert_eq!(started.len(), 2);
        let completed = repo.list_by_type("task_completed", 10).await.unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].payload.as_deref(), Some("{}"));
    }

    #[tokio::test]
    async fn phase_progress_mark_done_and_get() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = PhaseProgressRepo::new(conn.clone());

        repo.mark_done("fn-1.1", "plan").await.unwrap();
        repo.mark_done("fn-1.1", "implement").await.unwrap();

        let phases = repo.get_completed("fn-1.1").await.unwrap();
        assert_eq!(phases, vec!["plan".to_string(), "implement".to_string()]);

        // Idempotent re-mark.
        repo.mark_done("fn-1.1", "plan").await.unwrap();
        let phases2 = repo.get_completed("fn-1.1").await.unwrap();
        assert_eq!(phases2.len(), 2);

        let n = repo.reset("fn-1.1").await.unwrap();
        assert_eq!(n, 2);
        assert!(repo.get_completed("fn-1.1").await.unwrap().is_empty());
    }
}
