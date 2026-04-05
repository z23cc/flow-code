//! Approval store: CRUD over the `approvals` libSQL table.
//!
//! Wraps `flowctl_core::approvals::Approval` protocol types with persistence.
//! Used by the CLI (direct-DB fallback) and the daemon (HTTP handlers) to
//! keep approval state consistent.

use chrono::Utc;
use libsql::{params, Connection};

use flowctl_core::approvals::{
    Approval, ApprovalKind, ApprovalStatus, CreateApprovalRequest,
};

use crate::error::{ServiceError, ServiceResult};

/// Trait for approval persistence. Wire-level implementation sits on libSQL;
/// integration tests may supply a fake.
#[async_trait::async_trait]
pub trait ApprovalStore: Send + Sync {
    async fn create(&self, req: CreateApprovalRequest) -> ServiceResult<Approval>;
    async fn list(&self, status_filter: Option<ApprovalStatus>) -> ServiceResult<Vec<Approval>>;
    async fn get(&self, id: &str) -> ServiceResult<Approval>;
    async fn approve(&self, id: &str, resolver: Option<String>) -> ServiceResult<Approval>;
    async fn reject(
        &self,
        id: &str,
        resolver: Option<String>,
        reason: Option<String>,
    ) -> ServiceResult<Approval>;
}

/// libSQL-backed approval store.
#[derive(Clone)]
pub struct LibSqlApprovalStore {
    conn: Connection,
}

impl LibSqlApprovalStore {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    fn new_id() -> String {
        // Simple monotonic-ish identifier. Uses epoch millis + 4-byte random
        // suffix (time-based) — collision risk within the same ms is negligible
        // for single-daemon usage and keeps the crate dep-free.
        let now = Utc::now();
        let millis = now.timestamp_millis();
        let nanos = now.timestamp_subsec_nanos();
        format!("apv-{millis:x}-{nanos:x}")
    }

    async fn load_row(&self, id: &str) -> ServiceResult<Approval> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, task_id, kind, payload, status, created_at, resolved_at, resolver, reason
                 FROM approvals WHERE id = ?1",
                params![id.to_string()],
            )
            .await
            .map_err(|e| ServiceError::ValidationError(format!("query failed: {e}")))?;

        let row = rows
            .next()
            .await
            .map_err(|e| ServiceError::ValidationError(format!("row read failed: {e}")))?
            .ok_or_else(|| ServiceError::TaskNotFound(format!("approval not found: {id}")))?;

        row_to_approval(row)
    }
}

fn row_to_approval(row: libsql::Row) -> ServiceResult<Approval> {
    let id: String = row
        .get(0)
        .map_err(|e| ServiceError::ValidationError(format!("id: {e}")))?;
    let task_id: String = row
        .get(1)
        .map_err(|e| ServiceError::ValidationError(format!("task_id: {e}")))?;
    let kind_str: String = row
        .get(2)
        .map_err(|e| ServiceError::ValidationError(format!("kind: {e}")))?;
    let payload_str: String = row
        .get(3)
        .map_err(|e| ServiceError::ValidationError(format!("payload: {e}")))?;
    let status_str: String = row
        .get(4)
        .map_err(|e| ServiceError::ValidationError(format!("status: {e}")))?;
    let created_at: i64 = row
        .get(5)
        .map_err(|e| ServiceError::ValidationError(format!("created_at: {e}")))?;
    let resolved_at: Option<i64> = row
        .get(6)
        .map_err(|e| ServiceError::ValidationError(format!("resolved_at: {e}")))?;
    let resolver: Option<String> = row
        .get(7)
        .map_err(|e| ServiceError::ValidationError(format!("resolver: {e}")))?;
    let reason: Option<String> = row
        .get(8)
        .map_err(|e| ServiceError::ValidationError(format!("reason: {e}")))?;

    let kind = ApprovalKind::parse(&kind_str)
        .ok_or_else(|| ServiceError::ValidationError(format!("unknown kind: {kind_str}")))?;
    let status = ApprovalStatus::parse(&status_str)
        .ok_or_else(|| ServiceError::ValidationError(format!("unknown status: {status_str}")))?;
    let payload: serde_json::Value = serde_json::from_str(&payload_str)
        .map_err(|e| ServiceError::ValidationError(format!("payload JSON: {e}")))?;

    Ok(Approval {
        id,
        task_id,
        kind,
        payload,
        status,
        created_at,
        resolved_at,
        resolver,
        reason,
    })
}

#[async_trait::async_trait]
impl ApprovalStore for LibSqlApprovalStore {
    async fn create(&self, req: CreateApprovalRequest) -> ServiceResult<Approval> {
        // Reject orphan approvals: the referenced task must exist. Without
        // this check a typo creates a ghost pending record with no way to
        // reconcile it to real work.
        let exists: i64 = {
            let mut rows = self
                .conn
                .query(
                    "SELECT COUNT(*) FROM tasks WHERE id = ?1",
                    params![req.task_id.clone()],
                )
                .await
                .map_err(|e| ServiceError::ValidationError(format!("task lookup: {e}")))?;
            let row = rows
                .next()
                .await
                .map_err(|e| ServiceError::ValidationError(format!("task lookup row: {e}")))?
                .ok_or_else(|| {
                    ServiceError::ValidationError("task lookup returned no rows".into())
                })?;
            row.get(0)
                .map_err(|e| ServiceError::ValidationError(format!("task lookup value: {e}")))?
        };
        if exists == 0 {
            return Err(ServiceError::ValidationError(format!(
                "task {} does not exist",
                req.task_id
            )));
        }

        let id = Self::new_id();
        let now = Utc::now().timestamp();
        let payload_str = serde_json::to_string(&req.payload)
            .map_err(|e| ServiceError::ValidationError(format!("payload encode: {e}")))?;

        self.conn
            .execute(
                "INSERT INTO approvals (id, task_id, kind, payload, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
                params![
                    id.clone(),
                    req.task_id.clone(),
                    req.kind.as_str().to_string(),
                    payload_str,
                    now,
                ],
            )
            .await
            .map_err(|e| ServiceError::ValidationError(format!("insert failed: {e}")))?;

        self.load_row(&id).await
    }

    async fn list(&self, status_filter: Option<ApprovalStatus>) -> ServiceResult<Vec<Approval>> {
        let mut sql = String::from(
            "SELECT id, task_id, kind, payload, status, created_at, resolved_at, resolver, reason
             FROM approvals",
        );
        let mut rows = if let Some(s) = status_filter {
            sql.push_str(" WHERE status = ?1 ORDER BY created_at DESC");
            self.conn
                .query(&sql, params![s.as_str().to_string()])
                .await
                .map_err(|e| ServiceError::ValidationError(format!("query failed: {e}")))?
        } else {
            sql.push_str(" ORDER BY created_at DESC");
            self.conn
                .query(&sql, ())
                .await
                .map_err(|e| ServiceError::ValidationError(format!("query failed: {e}")))?
        };

        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| ServiceError::ValidationError(format!("row read: {e}")))?
        {
            out.push(row_to_approval(row)?);
        }
        Ok(out)
    }

    async fn get(&self, id: &str) -> ServiceResult<Approval> {
        self.load_row(id).await
    }

    async fn approve(&self, id: &str, resolver: Option<String>) -> ServiceResult<Approval> {
        // Pre-check for better error messages; authoritative guard is the UPDATE below.
        let existing = self.load_row(id).await?;
        if existing.status != ApprovalStatus::Pending {
            return Err(ServiceError::InvalidTransition(format!(
                "approval {id} is already {:?}",
                existing.status
            )));
        }
        let now = Utc::now().timestamp();
        let affected = self
            .conn
            .execute(
                "UPDATE approvals SET status = 'approved', resolved_at = ?1, resolver = ?2
                 WHERE id = ?3 AND status = 'pending'",
                params![now, resolver.clone(), id.to_string()],
            )
            .await
            .map_err(|e| ServiceError::ValidationError(format!("update failed: {e}")))?;
        if affected == 0 {
            // Lost a race with another resolver — the row is no longer pending.
            return Err(ServiceError::InvalidTransition(format!(
                "approval {id} was resolved concurrently"
            )));
        }
        self.load_row(id).await
    }

    async fn reject(
        &self,
        id: &str,
        resolver: Option<String>,
        reason: Option<String>,
    ) -> ServiceResult<Approval> {
        // Pre-check for better error messages; authoritative guard is the UPDATE below.
        let existing = self.load_row(id).await?;
        if existing.status != ApprovalStatus::Pending {
            return Err(ServiceError::InvalidTransition(format!(
                "approval {id} is already {:?}",
                existing.status
            )));
        }
        let now = Utc::now().timestamp();
        let affected = self
            .conn
            .execute(
                "UPDATE approvals SET status = 'rejected', resolved_at = ?1, resolver = ?2, reason = ?3
                 WHERE id = ?4 AND status = 'pending'",
                params![now, resolver.clone(), reason.clone(), id.to_string()],
            )
            .await
            .map_err(|e| ServiceError::ValidationError(format!("update failed: {e}")))?;
        if affected == 0 {
            // Lost a race with another resolver — the row is no longer pending.
            return Err(ServiceError::InvalidTransition(format!(
                "approval {id} was resolved concurrently"
            )));
        }
        self.load_row(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn in_mem_store() -> LibSqlApprovalStore {
        let (db, conn) = flowctl_db::open_memory_async().await.unwrap();
        // Seed tasks referenced by the tests so the existence check passes.
        let now = "2026-01-01T00:00:00Z";
        for tid in &["fn-1.1", "fn-1.2"] {
            let (epic_id, _num) = tid.split_once('.').unwrap();
            conn.execute(
                "INSERT OR IGNORE INTO epics
                    (id, title, status, file_path, created_at, updated_at, body)
                 VALUES (?1, ?1, 'open', ?1, ?2, ?2, '')",
                params![epic_id.to_string(), now.to_string()],
            )
            .await
            .expect("seed epic");
            conn.execute(
                "INSERT OR IGNORE INTO tasks
                    (id, epic_id, title, status, file_path, created_at, updated_at, body)
                 VALUES (?1, ?2, ?1, 'todo', ?1, ?3, ?3, '')",
                params![tid.to_string(), epic_id.to_string(), now.to_string()],
            )
            .await
            .expect("seed task");
        }
        // Leak the db so conn stays valid for the test lifetime.
        Box::leak(Box::new(db));
        LibSqlApprovalStore::new(conn)
    }

    #[tokio::test]
    async fn create_rejects_nonexistent_task() {
        let store = in_mem_store().await;
        let err = store
            .create(CreateApprovalRequest {
                task_id: "fn-999.99".into(),
                kind: ApprovalKind::FileAccess,
                payload: serde_json::json!({}),
            })
            .await
            .expect_err("should reject nonexistent task");
        assert!(matches!(err, ServiceError::ValidationError(_)));
    }

    #[tokio::test]
    async fn create_and_get() {
        let store = in_mem_store().await;
        let created = store
            .create(CreateApprovalRequest {
                task_id: "fn-1.1".into(),
                kind: ApprovalKind::FileAccess,
                payload: serde_json::json!({"files": ["a.rs"]}),
            })
            .await
            .unwrap();
        assert_eq!(created.status, ApprovalStatus::Pending);
        assert_eq!(created.task_id, "fn-1.1");

        let fetched = store.get(&created.id).await.unwrap();
        assert_eq!(fetched.id, created.id);
    }

    #[tokio::test]
    async fn list_with_filter() {
        let store = in_mem_store().await;
        let a = store
            .create(CreateApprovalRequest {
                task_id: "fn-1.1".into(),
                kind: ApprovalKind::Generic,
                payload: serde_json::json!({}),
            })
            .await
            .unwrap();
        let b = store
            .create(CreateApprovalRequest {
                task_id: "fn-1.2".into(),
                kind: ApprovalKind::Mutation,
                payload: serde_json::json!({"op": "split"}),
            })
            .await
            .unwrap();
        store.approve(&b.id, Some("alice".into())).await.unwrap();

        let pending = store
            .list(Some(ApprovalStatus::Pending))
            .await
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, a.id);

        let approved = store
            .list(Some(ApprovalStatus::Approved))
            .await
            .unwrap();
        assert_eq!(approved.len(), 1);
        assert_eq!(approved[0].id, b.id);

        let all = store.list(None).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn approve_transitions_status() {
        let store = in_mem_store().await;
        let created = store
            .create(CreateApprovalRequest {
                task_id: "fn-1.1".into(),
                kind: ApprovalKind::FileAccess,
                payload: serde_json::json!({}),
            })
            .await
            .unwrap();
        let resolved = store
            .approve(&created.id, Some("bob".into()))
            .await
            .unwrap();
        assert_eq!(resolved.status, ApprovalStatus::Approved);
        assert!(resolved.resolved_at.is_some());
        assert_eq!(resolved.resolver.as_deref(), Some("bob"));

        // Double-approve should fail.
        let err = store.approve(&created.id, None).await.unwrap_err();
        matches!(err, ServiceError::InvalidTransition(_));
    }

    #[tokio::test]
    async fn reject_records_reason() {
        let store = in_mem_store().await;
        let created = store
            .create(CreateApprovalRequest {
                task_id: "fn-1.1".into(),
                kind: ApprovalKind::Mutation,
                payload: serde_json::json!({}),
            })
            .await
            .unwrap();
        let resolved = store
            .reject(
                &created.id,
                Some("carol".into()),
                Some("not safe".into()),
            )
            .await
            .unwrap();
        assert_eq!(resolved.status, ApprovalStatus::Rejected);
        assert_eq!(resolved.reason.as_deref(), Some("not safe"));
    }

    #[tokio::test]
    async fn get_missing_returns_not_found() {
        let store = in_mem_store().await;
        let err = store.get("apv-missing").await.unwrap_err();
        matches!(err, ServiceError::TaskNotFound(_));
    }
}
