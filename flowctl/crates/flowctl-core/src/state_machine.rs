//! Task state machine with all 8 states and validated transitions.
//!
//! Ported from the design spec's state diagram. The `skipped` state is
//! treated as equivalent to `done` for dependency resolution.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error type for invalid state transitions.
#[derive(Debug, Error)]
#[error("invalid transition from {from} to {to}")]
pub struct TransitionError {
    pub from: Status,
    pub to: Status,
}

/// Task status with all 8 states from the design spec.
///
/// State diagram:
/// ```text
///                                     ┌──────────────┐
///                                     │ up_for_retry  │
///                                     └──┬────────────┘
///                                        │ retry
///      ┌──────┐    ┌─────────────┐    ┌──▼───────┐    ┌──────┐
///      │ todo │───>│ in_progress │───>│  failed  │    │ done │
///      └──────┘    └──────┬──────┘    └──────────┘    └──────┘
///                         │                              ▲
///                         ├──────────────────────────────┘
///                         │
///                         │           ┌─────────────────┐
///                         │           │ upstream_failed  │
///                         │           └─────────────────┘
///                         │
///                  ┌──────▼──────┐    ┌─────────┐
///                  │   blocked   │    │ skipped  │
///                  └─────────────┘    └──────────┘
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Initial state -- task is ready to be started.
    #[default]
    Todo,

    /// Task is actively being worked on.
    InProgress,

    /// Task completed successfully.
    Done,

    /// Task is blocked by an external dependency.
    Blocked,

    /// Task was deliberately skipped (treated as `done` for dep resolution).
    Skipped,

    /// Task failed (terminal failure, not retriable).
    Failed,

    /// Task failed but is eligible for retry.
    UpForRetry,

    /// A dependency of this task failed; task cannot be executed.
    UpstreamFailed,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Todo => write!(f, "todo"),
            Status::InProgress => write!(f, "in_progress"),
            Status::Done => write!(f, "done"),
            Status::Blocked => write!(f, "blocked"),
            Status::Skipped => write!(f, "skipped"),
            Status::Failed => write!(f, "failed"),
            Status::UpForRetry => write!(f, "up_for_retry"),
            Status::UpstreamFailed => write!(f, "upstream_failed"),
        }
    }
}

impl Status {
    /// All valid status values.
    pub const ALL: &[Status] = &[
        Status::Todo,
        Status::InProgress,
        Status::Done,
        Status::Blocked,
        Status::Skipped,
        Status::Failed,
        Status::UpForRetry,
        Status::UpstreamFailed,
    ];

    /// Whether this status is considered "satisfied" for dependency resolution.
    ///
    /// Both `done` and `skipped` satisfy downstream dependencies.
    pub fn is_satisfied(&self) -> bool {
        matches!(self, Status::Done | Status::Skipped)
    }

    /// Whether this status represents a terminal state (no further transitions).
    pub fn is_terminal(&self) -> bool {
        matches!(self, Status::Done | Status::Skipped)
    }

    /// Whether this status indicates the task is in a failure state.
    pub fn is_failed(&self) -> bool {
        matches!(self, Status::Failed | Status::UpstreamFailed)
    }

    /// Whether this status indicates the task is actively running.
    pub fn is_active(&self) -> bool {
        matches!(self, Status::InProgress)
    }

    /// Parse a status string. Case-insensitive, supports both snake_case
    /// and the plain form.
    pub fn parse(s: &str) -> Option<Status> {
        match s.to_lowercase().as_str() {
            "todo" => Some(Status::Todo),
            "in_progress" | "in-progress" | "inprogress" => Some(Status::InProgress),
            "done" => Some(Status::Done),
            "blocked" => Some(Status::Blocked),
            "skipped" => Some(Status::Skipped),
            "failed" => Some(Status::Failed),
            "up_for_retry" | "up-for-retry" | "upforretry" => Some(Status::UpForRetry),
            "upstream_failed" | "upstream-failed" | "upstreamfailed" => {
                Some(Status::UpstreamFailed)
            }
            _ => None,
        }
    }
}

/// A validated state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    pub from: Status,
    pub to: Status,
}

impl Transition {
    /// Attempt to create a validated transition. Returns an error if the
    /// transition is not allowed by the state machine rules.
    ///
    /// Valid transitions (from design spec):
    /// - `todo -> in_progress` (flowctl start)
    /// - `todo -> skipped` (flowctl task skip)
    /// - `in_progress -> done` (flowctl done)
    /// - `in_progress -> failed` (guard failure, timeout)
    /// - `in_progress -> blocked` (flowctl block)
    /// - `failed -> up_for_retry` (auto, if retries remaining)
    /// - `up_for_retry -> in_progress` (scheduler auto-retry)
    /// - `blocked -> todo` (flowctl restart)
    /// - `failed -> todo` (flowctl restart)
    /// - `* -> upstream_failed` (dependency entered failed)
    pub fn new(from: Status, to: Status) -> Result<Self, TransitionError> {
        if Self::is_valid(from, to) {
            Ok(Transition { from, to })
        } else {
            Err(TransitionError { from, to })
        }
    }

    /// Check whether a transition is valid without creating one.
    pub fn is_valid(from: Status, to: Status) -> bool {
        // Any status can transition to upstream_failed (propagation).
        if to == Status::UpstreamFailed {
            return true;
        }

        matches!(
            (from, to),
            (Status::Todo, Status::InProgress)
                | (Status::Todo, Status::Skipped)
                | (Status::InProgress, Status::Done)
                | (Status::InProgress, Status::Failed)
                | (Status::InProgress, Status::Blocked)
                | (Status::Failed, Status::UpForRetry)
                | (Status::UpForRetry, Status::InProgress)
                | (Status::Blocked, Status::Todo)
                | (Status::Failed, Status::Todo)
        )
    }

    /// Get all valid target states from a given state.
    pub fn valid_targets(from: Status) -> Vec<Status> {
        Status::ALL
            .iter()
            .copied()
            .filter(|&to| Self::is_valid(from, to))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_8_states_exist() {
        assert_eq!(Status::ALL.len(), 8);
    }

    #[test]
    fn test_status_display() {
        assert_eq!(Status::Todo.to_string(), "todo");
        assert_eq!(Status::InProgress.to_string(), "in_progress");
        assert_eq!(Status::Done.to_string(), "done");
        assert_eq!(Status::Blocked.to_string(), "blocked");
        assert_eq!(Status::Skipped.to_string(), "skipped");
        assert_eq!(Status::Failed.to_string(), "failed");
        assert_eq!(Status::UpForRetry.to_string(), "up_for_retry");
        assert_eq!(Status::UpstreamFailed.to_string(), "upstream_failed");
    }

    #[test]
    fn test_status_parse() {
        assert_eq!(Status::parse("todo"), Some(Status::Todo));
        assert_eq!(Status::parse("in_progress"), Some(Status::InProgress));
        assert_eq!(Status::parse("in-progress"), Some(Status::InProgress));
        assert_eq!(Status::parse("done"), Some(Status::Done));
        assert_eq!(Status::parse("blocked"), Some(Status::Blocked));
        assert_eq!(Status::parse("skipped"), Some(Status::Skipped));
        assert_eq!(Status::parse("failed"), Some(Status::Failed));
        assert_eq!(Status::parse("up_for_retry"), Some(Status::UpForRetry));
        assert_eq!(Status::parse("upstream_failed"), Some(Status::UpstreamFailed));
        assert_eq!(Status::parse("DONE"), Some(Status::Done));
        assert_eq!(Status::parse("invalid"), None);
    }

    #[test]
    fn test_status_serde_roundtrip() {
        for &status in Status::ALL {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: Status = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, status, "Serde roundtrip failed for {status}");
        }
    }

    #[test]
    fn test_status_is_satisfied() {
        assert!(Status::Done.is_satisfied());
        assert!(Status::Skipped.is_satisfied());
        assert!(!Status::Todo.is_satisfied());
        assert!(!Status::InProgress.is_satisfied());
        assert!(!Status::Blocked.is_satisfied());
        assert!(!Status::Failed.is_satisfied());
    }

    #[test]
    fn test_valid_transitions() {
        // All explicitly valid transitions from the spec.
        let valid = [
            (Status::Todo, Status::InProgress),
            (Status::Todo, Status::Skipped),
            (Status::InProgress, Status::Done),
            (Status::InProgress, Status::Failed),
            (Status::InProgress, Status::Blocked),
            (Status::Failed, Status::UpForRetry),
            (Status::UpForRetry, Status::InProgress),
            (Status::Blocked, Status::Todo),
            (Status::Failed, Status::Todo),
        ];

        for (from, to) in valid {
            assert!(
                Transition::is_valid(from, to),
                "Expected valid: {from} -> {to}"
            );
            assert!(
                Transition::new(from, to).is_ok(),
                "Expected Ok: {from} -> {to}"
            );
        }
    }

    #[test]
    fn test_upstream_failed_from_any_state() {
        // Any state can transition to upstream_failed.
        for &from in Status::ALL {
            assert!(
                Transition::is_valid(from, Status::UpstreamFailed),
                "Expected valid: {from} -> upstream_failed"
            );
        }
    }

    #[test]
    fn test_invalid_transitions() {
        let invalid = [
            (Status::Todo, Status::Done),           // Must go through in_progress
            (Status::Todo, Status::Failed),          // Must go through in_progress
            (Status::Done, Status::InProgress),      // Terminal
            (Status::Done, Status::Todo),            // Terminal
            (Status::Skipped, Status::Todo),         // Terminal
            (Status::Skipped, Status::InProgress),   // Terminal
            (Status::Blocked, Status::Done),         // Must go through todo -> in_progress
            (Status::InProgress, Status::Todo),      // Can't go back without restart
            (Status::InProgress, Status::Skipped),   // Can't skip while running
            (Status::UpstreamFailed, Status::Todo),  // Propagated failure is sticky
        ];

        for (from, to) in invalid {
            assert!(
                !Transition::is_valid(from, to),
                "Expected invalid: {from} -> {to}"
            );
            assert!(
                Transition::new(from, to).is_err(),
                "Expected Err: {from} -> {to}"
            );
        }
    }

    #[test]
    fn test_valid_targets() {
        let todo_targets = Transition::valid_targets(Status::Todo);
        assert!(todo_targets.contains(&Status::InProgress));
        assert!(todo_targets.contains(&Status::Skipped));
        assert!(todo_targets.contains(&Status::UpstreamFailed));
        assert_eq!(todo_targets.len(), 3);

        let in_progress_targets = Transition::valid_targets(Status::InProgress);
        assert!(in_progress_targets.contains(&Status::Done));
        assert!(in_progress_targets.contains(&Status::Failed));
        assert!(in_progress_targets.contains(&Status::Blocked));
        assert!(in_progress_targets.contains(&Status::UpstreamFailed));
        assert_eq!(in_progress_targets.len(), 4);

        let done_targets = Transition::valid_targets(Status::Done);
        // Done is terminal -- only upstream_failed
        assert_eq!(done_targets, vec![Status::UpstreamFailed]);
    }

    #[test]
    fn test_default_status_is_todo() {
        assert_eq!(Status::default(), Status::Todo);
    }

    #[test]
    fn test_transition_error_display() {
        let err = TransitionError {
            from: Status::Todo,
            to: Status::Done,
        };
        assert_eq!(err.to_string(), "invalid transition from todo to done");
    }
}
