//! EventStore — per-goal append-only event log at .flow/goals/{id}/events.jsonl

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// An event in the goal's lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalEvent {
    pub timestamp: DateTime<Utc>,
    pub kind: GoalEventKind,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalEventKind {
    GoalCreated,
    PlanCreated,
    PlanMutated,
    NodeStarted,
    NodeCompleted,
    NodeFailed,
    EscalationTriggered,
    LearningRecorded,
    ScoreUpdated,
    GoalCompleted,
}

/// Append-only event log per goal.
pub struct EventStore {
    root: PathBuf,
}

impl EventStore {
    pub fn new(flow_root: &Path) -> Self {
        Self {
            root: flow_root.join("goals"),
        }
    }

    fn events_path(&self, goal_id: &str) -> PathBuf {
        self.root.join(goal_id).join("events.jsonl")
    }

    /// Append an event to the goal's log.
    pub fn append(&self, goal_id: &str, event: &GoalEvent) -> Result<(), String> {
        let path = self.events_path(goal_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
        }
        let mut line = serde_json::to_string(event).map_err(|e| format!("serialize: {e}"))?;
        line.push('\n');
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("open events: {e}"))?;
        file.write_all(line.as_bytes()).map_err(|e| format!("write event: {e}"))
    }

    /// Read all events for a goal.
    pub fn list(&self, goal_id: &str) -> Result<Vec<GoalEvent>, String> {
        let path = self.events_path(goal_id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let data = fs::read_to_string(&path).map_err(|e| format!("read events: {e}"))?;
        let mut events = Vec::new();
        for line in data.lines() {
            if line.trim().is_empty() { continue; }
            let event: GoalEvent = serde_json::from_str(line).map_err(|e| format!("parse event: {e}"))?;
            events.push(event);
        }
        Ok(events)
    }

    /// Helper to emit a standard event.
    pub fn emit(&self, goal_id: &str, kind: GoalEventKind, detail: &str) -> Result<(), String> {
        self.append(goal_id, &GoalEvent {
            timestamp: Utc::now(),
            kind,
            detail: detail.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, EventStore) {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("goals").join("g-1")).unwrap();
        let store = EventStore::new(tmp.path());
        (tmp, store)
    }

    #[test]
    fn test_append_and_list() {
        let (_tmp, store) = setup();
        store.emit("g-1", GoalEventKind::GoalCreated, "test goal").unwrap();
        store.emit("g-1", GoalEventKind::PlanCreated, "plan v1").unwrap();
        let events = store.list("g-1").unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0].kind, GoalEventKind::GoalCreated));
        assert!(matches!(events[1].kind, GoalEventKind::PlanCreated));
    }

    #[test]
    fn test_list_empty() {
        let (_tmp, store) = setup();
        let events = store.list("g-1").unwrap();
        assert!(events.is_empty());
    }
}
