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
