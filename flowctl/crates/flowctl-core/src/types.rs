//! Core data types for flowctl.
//!
//! Ported from `scripts/flowctl/core/constants.py` and the Markdown
//! frontmatter format defined in the design spec.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::state_machine::Status;

// Re-export Document from frontmatter for use as a general-purpose container.
// This allows CLI CRUD code to import Document from types (not frontmatter).
pub use crate::frontmatter::Document;

// ── Constants ────────────────────────────────────────────────────────

/// Current schema version for Markdown frontmatter.
pub const SCHEMA_VERSION: u32 = 1;

/// Supported schema versions for backward compatibility.
pub const SUPPORTED_SCHEMA_VERSIONS: &[u32] = &[1, 2];

/// Directory names within `.flow/`.
pub const FLOW_DIR: &str = ".flow";
pub const EPICS_DIR: &str = "epics";
pub const SPECS_DIR: &str = "specs";
pub const TASKS_DIR: &str = "tasks";
pub const MEMORY_DIR: &str = "memory";
pub const REVIEWS_DIR: &str = "reviews";
pub const CONFIG_FILE: &str = "config.json";
pub const META_FILE: &str = "meta.json";
pub const STATE_DIR: &str = ".state";
pub const ARCHIVE_DIR: &str = ".archive";

/// Valid epic statuses.
pub const EPIC_STATUSES: &[&str] = &["open", "done"];

/// Required headings in task spec Markdown body.
pub const TASK_SPEC_HEADINGS: &[&str] = &[
    "## Description",
    "## Acceptance",
    "## Done summary",
    "## Evidence",
];

// ── Domain ───────────────────────────────────────────────────────────

/// Task domain classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Domain {
    Frontend,
    Backend,
    Architecture,
    Testing,
    Docs,
    Ops,
    #[default]
    General,
}

impl std::fmt::Display for Domain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Domain::Frontend => write!(f, "frontend"),
            Domain::Backend => write!(f, "backend"),
            Domain::Architecture => write!(f, "architecture"),
            Domain::Testing => write!(f, "testing"),
            Domain::Docs => write!(f, "docs"),
            Domain::Ops => write!(f, "ops"),
            Domain::General => write!(f, "general"),
        }
    }
}

// ── TaskSize ────────────────────────────────────────────────────────

/// Task size classification — controls the worker phase sequence length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TaskSize {
    /// Small: minimal worker sequence with investigation preserved; outputs and
    /// memory may still be added by config.
    #[serde(rename = "S")]
    Small,
    /// Medium: default phase sequence.
    #[default]
    #[serde(rename = "M")]
    Medium,
    /// Large: all defined phases.
    #[serde(rename = "L")]
    Large,
}

impl std::fmt::Display for TaskSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskSize::Small => write!(f, "S"),
            TaskSize::Medium => write!(f, "M"),
            TaskSize::Large => write!(f, "L"),
        }
    }
}

impl std::str::FromStr for TaskSize {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "S" => Ok(TaskSize::Small),
            "M" => Ok(TaskSize::Medium),
            "L" => Ok(TaskSize::Large),
            other => Err(format!(
                "invalid task size '{}': expected S, M, or L",
                other
            )),
        }
    }
}

// ── Epic ─────────────────────────────────────────────────────────────

/// Epic status (simpler than task status).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EpicStatus {
    #[default]
    Open,
    Done,
}

impl std::fmt::Display for EpicStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EpicStatus::Open => write!(f, "open"),
            EpicStatus::Done => write!(f, "done"),
        }
    }
}

/// Plan review status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStatus {
    #[default]
    Unknown,
    Passed,
    Failed,
}

impl std::fmt::Display for ReviewStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReviewStatus::Unknown => write!(f, "unknown"),
            ReviewStatus::Passed => write!(f, "passed"),
            ReviewStatus::Failed => write!(f, "failed"),
        }
    }
}

/// An epic -- a collection of related tasks.
///
/// Maps to the YAML frontmatter in `epics/fn-N-slug.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Epic {
    /// Schema version for forward compatibility.
    #[serde(default = "default_schema_version", skip_serializing)]
    pub schema_version: u32,

    /// Unique ID, e.g. `fn-1-add-auth`.
    pub id: String,

    /// Human-readable title.
    pub title: String,

    /// Current status.
    #[serde(default)]
    pub status: EpicStatus,

    /// Git branch name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_name: Option<String>,

    /// Plan review status.
    #[serde(default)]
    pub plan_review: ReviewStatus,

    /// Completion review status.
    #[serde(default)]
    pub completion_review: ReviewStatus,

    /// Epic-level dependencies (IDs of other epics).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on_epics: Vec<String>,

    /// Default implementation backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_impl: Option<String>,

    /// Default review backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_review: Option<String>,

    /// Default sync backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_sync: Option<String>,

    /// Whether auto-execute is pending for this epic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_execute_pending: Option<bool>,

    /// When auto_execute_pending was set (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_execute_set_at: Option<String>,

    /// Whether this epic has been archived.
    #[serde(default)]
    pub archived: bool,

    /// File path to the Markdown spec.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,

    /// Creation timestamp.
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Last update timestamp.
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

/// A task within an epic.
///
/// Maps to the YAML frontmatter in `tasks/fn-N-slug.M.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Schema version for forward compatibility.
    #[serde(default = "default_schema_version", skip_serializing)]
    pub schema_version: u32,

    /// Unique ID, e.g. `fn-1-add-auth.3`.
    pub id: String,

    /// Parent epic ID.
    pub epic: String,

    /// Human-readable title.
    pub title: String,

    /// Current status.
    #[serde(default)]
    pub status: Status,

    /// Priority (lower = higher priority, None = 999).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u32>,

    /// Domain classification.
    #[serde(default)]
    pub domain: Domain,

    /// Task dependencies (IDs of other tasks).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,

    /// Owned files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,

    /// Implementation backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#impl: Option<String>,

    /// Review backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<String>,

    /// Sync backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync: Option<String>,

    /// File path to the Markdown spec.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,

    /// Creation timestamp.
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Last update timestamp.
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

impl Task {
    /// Sort priority (None -> 999).
    pub fn sort_priority(&self) -> u32 {
        self.priority.unwrap_or(999)
    }
}

// ── Phase ────────────────────────────────────────────────────────────

/// Worker execution phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase {
    /// Phase ID (sequential integer, e.g. "1", "2", "5", "10").
    pub id: String,

    /// Human-readable title.
    pub title: String,

    /// Condition that must be met for the phase to be considered done.
    pub done_condition: String,

    /// Current phase status.
    #[serde(default)]
    pub status: PhaseStatus,

    /// Completion timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

/// Phase execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PhaseStatus {
    #[default]
    Pending,
    Active,
    Done,
    Skipped,
}

impl std::fmt::Display for PhaseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PhaseStatus::Pending => write!(f, "pending"),
            PhaseStatus::Active => write!(f, "active"),
            PhaseStatus::Done => write!(f, "done"),
            PhaseStatus::Skipped => write!(f, "skipped"),
        }
    }
}

/// Phase definitions — sequential integer IDs (1-12).
/// Each entry: (id, title, done_condition).
pub const PHASE_DEFS: &[(&str, &str, &str)] = &[
    (
        "1",
        "Verify Configuration",
        "OWNED_FILES verified and configuration validated",
    ),
    (
        "2",
        "Re-anchor",
        "Run flowctl show <task> and verify spec was read",
    ),
    (
        "3",
        "Investigation",
        "Required investigation target files read and patterns noted",
    ),
    (
        "4",
        "TDD Red-Green",
        "Failing tests written and confirmed to fail",
    ),
    ("5", "Implement", "Feature implemented and code compiles"),
    (
        "6",
        "Verify & Fix",
        "flowctl guard passes and diff reviewed",
    ),
    (
        "7",
        "Commit",
        "Changes committed with conventional commit message",
    ),
    ("8", "Review", "SHIP verdict received from reviewer"),
    (
        "9",
        "Outputs Dump",
        "Narrative summary written to .flow/outputs/<task-id>.md",
    ),
    (
        "10",
        "Complete",
        "flowctl done called and task status is done",
    ),
    (
        "11",
        "Memory Auto-Save",
        "Non-obvious lessons saved to memory (if any)",
    ),
    ("12", "Return", "Summary returned to main conversation"),
];

/// Phase sequences by mode.
/// Phase `9` (outputs_dump) is NOT in these static sequences — it is added
/// dynamically by `worker-phase next` based on the `outputs.enabled` config.
pub const PHASE_SEQ_DEFAULT: &[&str] = &["1", "2", "3", "5", "6", "7", "10", "11", "12"];
pub const PHASE_SEQ_TDD: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "10", "11", "12"];
pub const PHASE_SEQ_REVIEW: &[&str] = &["1", "2", "3", "5", "6", "7", "8", "10", "11", "12"];

// ── Evidence ─────────────────────────────────────────────────────────

/// Evidence of task completion, attached when task is marked done.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Evidence {
    /// Git commit hashes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commits: Vec<String>,

    /// Test commands that were run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<String>,

    /// Pull request URLs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prs: Vec<String>,

    /// Number of files changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files_changed: Option<u32>,

    /// Lines inserted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insertions: Option<u32>,

    /// Lines deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletions: Option<u32>,

    /// Number of review iterations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_iterations: Option<u32>,

    /// Workspace change tracking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_changes: Option<WorkspaceChanges>,
}

/// Workspace change summary (baseline vs final).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceChanges {
    /// Git rev at start of implementation.
    pub baseline_rev: String,

    /// Git rev at completion.
    pub final_rev: String,

    /// Number of files changed between baseline and final.
    pub files_changed: u32,

    /// Total insertions.
    pub insertions: u32,

    /// Total deletions.
    pub deletions: u32,
}

// ── Runtime state ────────────────────────────────────────────────────

/// Runtime-only fields (not stored in Markdown, only in SQLite).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeState {
    /// Task ID this state belongs to.
    pub task_id: String,

    /// Current assignee.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,

    /// When the task was claimed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<DateTime<Utc>>,

    /// When the task was completed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    /// Duration in seconds from start to completion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,

    /// Reason for being blocked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,

    /// Git rev at start of implementation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_rev: Option<String>,

    /// Git rev at completion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_rev: Option<String>,

    /// Number of retries attempted so far.
    #[serde(default)]
    pub retry_count: u32,
}

/// Runtime fields stored in state-dir (matching Python RUNTIME_FIELDS).
pub const RUNTIME_FIELDS: &[&str] = &[
    "status",
    "updated_at",
    "claimed_at",
    "assignee",
    "claim_note",
    "evidence",
    "blocked_reason",
    "phase_progress",
];

// ── Helpers ──────────────────────────────────────────────────────────

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_sort_priority() {
        let task = Task {
            schema_version: 1,
            id: "fn-1-test.1".to_string(),
            epic: "fn-1-test".to_string(),
            title: "Test".to_string(),
            status: Status::Todo,
            priority: None,
            domain: Domain::General,
            depends_on: vec![],
            files: vec![],
            r#impl: None,
            review: None,
            sync: None,
            file_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert_eq!(task.sort_priority(), 999);

        let task_with_prio = Task {
            priority: Some(1),
            ..task
        };
        assert_eq!(task_with_prio.sort_priority(), 1);
    }

    #[test]
    fn test_domain_display() {
        assert_eq!(Domain::Frontend.to_string(), "frontend");
        assert_eq!(Domain::Backend.to_string(), "backend");
        assert_eq!(Domain::General.to_string(), "general");
    }

    #[test]
    fn test_epic_status_display() {
        assert_eq!(EpicStatus::Open.to_string(), "open");
        assert_eq!(EpicStatus::Done.to_string(), "done");
    }

    #[test]
    fn test_evidence_default() {
        let evidence = Evidence::default();
        assert!(evidence.commits.is_empty());
        assert!(evidence.tests.is_empty());
        assert!(evidence.prs.is_empty());
        assert!(evidence.files_changed.is_none());
    }

    #[test]
    fn test_task_serde_roundtrip() {
        let task = Task {
            schema_version: 1,
            id: "fn-1-add-auth.1".to_string(),
            epic: "fn-1-add-auth".to_string(),
            title: "Design Auth Flow".to_string(),
            status: Status::Todo,
            priority: Some(1),
            domain: Domain::Backend,
            depends_on: vec![],
            files: vec!["src/auth.ts".to_string()],
            r#impl: None,
            review: None,
            sync: None,
            file_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let json = serde_json::to_string(&task).unwrap();
        let deserialized: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "fn-1-add-auth.1");
        assert_eq!(deserialized.status, Status::Todo);
        assert_eq!(deserialized.domain, Domain::Backend);
        assert_eq!(deserialized.priority, Some(1));
    }

    #[test]
    fn test_epic_serde_roundtrip() {
        let epic = Epic {
            schema_version: 1,
            id: "fn-1-add-auth".to_string(),
            title: "Add Authentication".to_string(),
            status: EpicStatus::Open,
            branch_name: Some("feat/add-auth".to_string()),
            plan_review: ReviewStatus::Unknown,
            completion_review: ReviewStatus::Unknown,
            depends_on_epics: vec![],
            default_impl: None,
            default_review: None,
            default_sync: None,
            auto_execute_pending: None,
            auto_execute_set_at: None,
            archived: false,
            file_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let json = serde_json::to_string(&epic).unwrap();
        let deserialized: Epic = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "fn-1-add-auth");
        assert_eq!(deserialized.status, EpicStatus::Open);
    }

    #[test]
    fn test_task_size_display_and_parse() {
        assert_eq!(TaskSize::Small.to_string(), "S");
        assert_eq!(TaskSize::Medium.to_string(), "M");
        assert_eq!(TaskSize::Large.to_string(), "L");

        assert_eq!("S".parse::<TaskSize>().unwrap(), TaskSize::Small);
        assert_eq!("m".parse::<TaskSize>().unwrap(), TaskSize::Medium);
        assert_eq!("L".parse::<TaskSize>().unwrap(), TaskSize::Large);
        assert!("X".parse::<TaskSize>().is_err());
    }

    #[test]
    fn test_task_size_default() {
        assert_eq!(TaskSize::default(), TaskSize::Medium);
    }

    #[test]
    fn test_phase_defs_complete() {
        assert_eq!(PHASE_DEFS.len(), 12);
        // Verify all phase sequences reference valid phase IDs
        let valid_ids: Vec<&str> = PHASE_DEFS.iter().map(|(id, _, _)| *id).collect();
        for seq_id in PHASE_SEQ_DEFAULT {
            assert!(
                valid_ids.contains(seq_id),
                "Invalid phase in default seq: {seq_id}"
            );
        }
        for seq_id in PHASE_SEQ_TDD {
            assert!(
                valid_ids.contains(seq_id),
                "Invalid phase in TDD seq: {seq_id}"
            );
        }
        for seq_id in PHASE_SEQ_REVIEW {
            assert!(
                valid_ids.contains(seq_id),
                "Invalid phase in review seq: {seq_id}"
            );
        }
    }
}
