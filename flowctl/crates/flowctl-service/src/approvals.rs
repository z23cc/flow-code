//! Approval store: file-based CRUD over `.state/approvals.json`.
//!
//! Wraps `flowctl_core::approvals::Approval` protocol types with persistence.
//! Used by the CLI and MCP to keep approval state consistent.

use chrono::Utc;

use flowctl_core::approvals::{
    Approval, ApprovalStatus, CreateApprovalRequest,
};

use flowctl_db::FlowStore;

use crate::error::{ServiceError, ServiceResult};

/// File-backed approval store.
pub struct FileApprovalStore {
    store: FlowStore,
}

impl FileApprovalStore {
    pub fn new(store: FlowStore) -> Self {
        Self { store }
    }

    fn new_id() -> String {
        let now = Utc::now();
        let millis = now.timestamp_millis();
        let nanos = now.timestamp_subsec_nanos();
        format!("apv-{millis:x}-{nanos:x}")
    }

    fn load_all(&self) -> ServiceResult<Vec<Approval>> {
        let raw = self.store.approvals().read()
            .map_err(ServiceError::DbError)?;
        let mut out = Vec::new();
        for val in raw {
            if let Ok(a) = serde_json::from_value::<Approval>(val) {
                out.push(a);
            }
        }
        Ok(out)
    }

    fn save_all(&self, approvals: &[Approval]) -> ServiceResult<()> {
        let vals: Vec<serde_json::Value> = approvals
            .iter()
            .filter_map(|a| serde_json::to_value(a).ok())
            .collect();
        self.store.approvals().write(&vals)
            .map_err(ServiceError::DbError)?;
        Ok(())
    }

    pub fn create(&self, req: CreateApprovalRequest) -> ServiceResult<Approval> {
        // Validate task exists
        if flowctl_core::json_store::task_read(self.store.flow_dir(), &req.task_id).is_err() {
            return Err(ServiceError::ValidationError(format!(
                "task {} does not exist",
                req.task_id
            )));
        }

        let id = Self::new_id();
        let now = Utc::now().timestamp();

        let approval = Approval {
            id: id.clone(),
            task_id: req.task_id,
            kind: req.kind,
            payload: req.payload,
            status: ApprovalStatus::Pending,
            created_at: now,
            resolved_at: None,
            resolver: None,
            reason: None,
        };

        let mut all = self.load_all()?;
        all.push(approval.clone());
        self.save_all(&all)?;

        Ok(approval)
    }

    pub fn list(&self, status_filter: Option<ApprovalStatus>) -> ServiceResult<Vec<Approval>> {
        let all = self.load_all()?;
        let mut filtered: Vec<Approval> = if let Some(s) = status_filter {
            all.into_iter().filter(|a| a.status == s).collect()
        } else {
            all
        };
        filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(filtered)
    }

    pub fn get(&self, id: &str) -> ServiceResult<Approval> {
        let all = self.load_all()?;
        all.into_iter()
            .find(|a| a.id == id)
            .ok_or_else(|| ServiceError::TaskNotFound(format!("approval not found: {id}")))
    }

    pub fn approve(&self, id: &str, resolver: Option<String>) -> ServiceResult<Approval> {
        let mut all = self.load_all()?;
        let approval = all.iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| ServiceError::TaskNotFound(format!("approval not found: {id}")))?;

        if approval.status != ApprovalStatus::Pending {
            return Err(ServiceError::InvalidTransition(format!(
                "approval {id} is already {:?}",
                approval.status
            )));
        }

        approval.status = ApprovalStatus::Approved;
        approval.resolved_at = Some(Utc::now().timestamp());
        approval.resolver = resolver;
        let result = approval.clone();

        self.save_all(&all)?;
        Ok(result)
    }

    pub fn reject(
        &self,
        id: &str,
        resolver: Option<String>,
        reason: Option<String>,
    ) -> ServiceResult<Approval> {
        let mut all = self.load_all()?;
        let approval = all.iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| ServiceError::TaskNotFound(format!("approval not found: {id}")))?;

        if approval.status != ApprovalStatus::Pending {
            return Err(ServiceError::InvalidTransition(format!(
                "approval {id} is already {:?}",
                approval.status
            )));
        }

        approval.status = ApprovalStatus::Rejected;
        approval.resolved_at = Some(Utc::now().timestamp());
        approval.resolver = resolver;
        approval.reason = reason;
        let result = approval.clone();

        self.save_all(&all)?;
        Ok(result)
    }
}
