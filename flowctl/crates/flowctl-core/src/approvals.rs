//! Approval protocol types.
//!
//! Approvals replace the stdin-blocking Teams-mode protocol. A worker
//! requests permission for a file access, mutation, or generic action by
//! creating an `Approval`. A supervisor (human or agent) inspects pending
//! approvals and resolves them via approve/reject.
//!
//! Per convention #008, protocol types live in `flowctl-core` so all other
//! crates (service, cli) share the same wire format.

use serde::{Deserialize, Serialize};

/// Kind of approval being requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    /// Worker requests access to a file outside its owned set.
    FileAccess,
    /// Worker requests a DAG mutation (split, skip, dep change).
    Mutation,
    /// Any other decision point that requires supervisor input.
    Generic,
}

impl ApprovalKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalKind::FileAccess => "file_access",
            ApprovalKind::Mutation => "mutation",
            ApprovalKind::Generic => "generic",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "file_access" => Some(ApprovalKind::FileAccess),
            "mutation" => Some(ApprovalKind::Mutation),
            "generic" => Some(ApprovalKind::Generic),
            _ => None,
        }
    }
}

/// Current lifecycle state of an approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
}

impl ApprovalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Rejected => "rejected",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(ApprovalStatus::Pending),
            "approved" => Some(ApprovalStatus::Approved),
            "rejected" => Some(ApprovalStatus::Rejected),
            _ => None,
        }
    }
}

/// A persisted approval record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Approval {
    pub id: String,
    pub task_id: String,
    pub kind: ApprovalKind,
    /// Opaque JSON payload describing the request (file paths, mutation args, etc.).
    pub payload: serde_json::Value,
    pub status: ApprovalStatus,
    /// Unix epoch seconds.
    pub created_at: i64,
    /// Unix epoch seconds when approve/reject occurred.
    pub resolved_at: Option<i64>,
    /// Who resolved the approval.
    pub resolver: Option<String>,
    /// Optional rejection reason.
    pub reason: Option<String>,
}

/// Request to create a new pending approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApprovalRequest {
    pub task_id: String,
    pub kind: ApprovalKind,
    pub payload: serde_json::Value,
}

/// Request to resolve (approve or reject) an approval.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolveRequest {
    pub resolver: Option<String>,
    pub reason: Option<String>,
}

// ── FileApprovalStore ────────────────────────────────────────────────

use std::path::{Path, PathBuf};
use chrono::Utc;
use crate::error::{ServiceError, ServiceResult};

/// File-backed approval store.
pub struct FileApprovalStore {
    flow_dir: PathBuf,
}

impl FileApprovalStore {
    pub fn new(flow_dir: PathBuf) -> Self {
        Self { flow_dir }
    }

    /// Return the flow directory path.
    pub fn flow_dir(&self) -> &Path {
        &self.flow_dir
    }

    fn new_id() -> String {
        let now = Utc::now();
        let millis = now.timestamp_millis();
        let nanos = now.timestamp_subsec_nanos();
        format!("apv-{millis:x}-{nanos:x}")
    }

    fn load_all(&self) -> ServiceResult<Vec<Approval>> {
        let raw = crate::json_store::approvals_read(&self.flow_dir)
            .map_err(|e| ServiceError::IoError(std::io::Error::other(e.to_string())))?;
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
        crate::json_store::approvals_write(&self.flow_dir, &vals)
            .map_err(|e| ServiceError::IoError(std::io::Error::other(e.to_string())))?;
        Ok(())
    }

    pub fn create(&self, req: CreateApprovalRequest) -> ServiceResult<Approval> {
        // Validate task exists
        if crate::json_store::task_read(&self.flow_dir, &req.task_id).is_err() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_roundtrip() {
        for kind in [
            ApprovalKind::FileAccess,
            ApprovalKind::Mutation,
            ApprovalKind::Generic,
        ] {
            assert_eq!(ApprovalKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(ApprovalKind::parse("bogus"), None);
    }

    #[test]
    fn status_roundtrip() {
        for s in [
            ApprovalStatus::Pending,
            ApprovalStatus::Approved,
            ApprovalStatus::Rejected,
        ] {
            assert_eq!(ApprovalStatus::parse(s.as_str()), Some(s));
        }
    }

    #[test]
    fn approval_serde() {
        let a = Approval {
            id: "apv-1".into(),
            task_id: "fn-1.1".into(),
            kind: ApprovalKind::FileAccess,
            payload: serde_json::json!({"files": ["a.rs"]}),
            status: ApprovalStatus::Pending,
            created_at: 1_700_000_000,
            resolved_at: None,
            resolver: None,
            reason: None,
        };
        let j = serde_json::to_string(&a).unwrap();
        let back: Approval = serde_json::from_str(&j).unwrap();
        assert_eq!(back.id, "apv-1");
        assert_eq!(back.kind, ApprovalKind::FileAccess);
        assert_eq!(back.status, ApprovalStatus::Pending);
    }
}
