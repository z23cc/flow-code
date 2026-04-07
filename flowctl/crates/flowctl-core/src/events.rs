//! Event-sourced domain events for flowctl.
//!
//! Defines split enums for epic and task events, a unified `FlowEvent`
//! wrapper, and `EventMetadata` for audit context. Stream IDs follow
//! the convention `"epic:<id>"` / `"task:<id>"`.

use serde::{Deserialize, Serialize};

// ── Epic events ─────────────────────────────────────────────────────

/// Domain events scoped to an epic lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EpicEvent {
    /// Epic was created.
    Created,
    /// Plan spec was written / updated.
    PlanWritten,
    /// A pipeline phase started (e.g. plan-review, work).
    PipelinePhaseStarted,
    /// A pipeline phase completed.
    PipelinePhaseCompleted,
    /// Epic was closed (all tasks done).
    Closed,
    /// Catch-all for forward-compatible deserialization.
    #[serde(other)]
    Unknown,
}

// ── Task events ─────────────────────────────────────────────────────

/// Domain events scoped to a task lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskEvent {
    /// Task was created.
    Created,
    /// Task moved to in_progress.
    Started,
    /// Task completed successfully.
    Completed,
    /// Task failed (terminal).
    Failed,
    /// Task blocked on external dependency.
    Blocked,
    /// Task deliberately skipped.
    Skipped,
    /// Worker advanced to the next phase.
    WorkerPhaseAdvanced,
    /// File lock acquired for this task.
    FileLocked,
    /// File lock released for this task.
    FileUnlocked,
    /// Catch-all for forward-compatible deserialization.
    #[serde(other)]
    Unknown,
}

// ── Unified wrapper ─────────────────────────────────────────────────

/// Tagged wrapper so a single `FlowEvent` column can hold either kind.
///
/// Uses internal tagging (`"scope": "epic"` / `"scope": "task"`) so that
/// the `#[serde(other)]` catch-all on each inner enum works correctly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", content = "event", rename_all = "snake_case")]
pub enum FlowEvent {
    Epic(EpicEvent),
    Task(TaskEvent),
}

// ── Metadata ────────────────────────────────────────────────────────

/// Contextual metadata attached to every event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Who triggered the event (e.g. "worker", "user", "ralph").
    pub actor: String,
    /// The CLI command that produced the event (e.g. "flowctl done").
    pub source_cmd: String,
    /// Session identifier for correlation.
    pub session_id: String,
    /// ISO-8601 timestamp (populated by the service layer, not the caller).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

// ── Stream ID helpers ───────────────────────────────────────────────

/// Build a stream ID for an epic: `"epic:<id>"`.
pub fn epic_stream_id(epic_id: &str) -> String {
    format!("epic:{epic_id}")
}

/// Build a stream ID for a task: `"task:<id>"`.
pub fn task_stream_id(task_id: &str) -> String {
    format!("task:{task_id}")
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epic_event_round_trip() {
        let variants = vec![
            EpicEvent::Created,
            EpicEvent::PlanWritten,
            EpicEvent::PipelinePhaseStarted,
            EpicEvent::PipelinePhaseCompleted,
            EpicEvent::Closed,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let back: EpicEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back, "round-trip failed for {json}");
        }
    }

    #[test]
    fn task_event_round_trip() {
        let variants = vec![
            TaskEvent::Created,
            TaskEvent::Started,
            TaskEvent::Completed,
            TaskEvent::Failed,
            TaskEvent::Blocked,
            TaskEvent::Skipped,
            TaskEvent::WorkerPhaseAdvanced,
            TaskEvent::FileLocked,
            TaskEvent::FileUnlocked,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let back: TaskEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back, "round-trip failed for {json}");
        }
    }

    #[test]
    fn flow_event_round_trip_epic() {
        let ev = FlowEvent::Epic(EpicEvent::Created);
        let json = serde_json::to_string(&ev).unwrap();
        // Untagged: epic variant serialises as the inner string
        let back: FlowEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, back);
    }

    #[test]
    fn flow_event_round_trip_task() {
        let ev = FlowEvent::Task(TaskEvent::Completed);
        let json = serde_json::to_string(&ev).unwrap();
        let back: FlowEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, back);
    }

    #[test]
    fn metadata_round_trip() {
        let meta = EventMetadata {
            actor: "worker".into(),
            source_cmd: "flowctl done".into(),
            session_id: "sess-abc".into(),
            timestamp: Some("2026-04-07T12:00:00Z".into()),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let back: EventMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(meta, back);
    }

    #[test]
    fn unknown_epic_event_tolerant_reader() {
        // A future event type should deserialize to Unknown.
        let json = r#""some_future_epic_event""#;
        let ev: EpicEvent = serde_json::from_str(json).unwrap();
        assert_eq!(ev, EpicEvent::Unknown);
    }

    #[test]
    fn unknown_task_event_tolerant_reader() {
        let json = r#""some_future_task_event""#;
        let ev: TaskEvent = serde_json::from_str(json).unwrap();
        assert_eq!(ev, TaskEvent::Unknown);
    }

    #[test]
    fn stream_id_format() {
        assert_eq!(epic_stream_id("fn-1"), "epic:fn-1");
        assert_eq!(task_stream_id("fn-1.3"), "task:fn-1.3");
    }
}
