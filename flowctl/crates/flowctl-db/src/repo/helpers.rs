//! Parsing helpers and ID-extraction utilities shared across repo sub-modules.

use chrono::{DateTime, Utc};
use libsql::{params, Connection};

use flowctl_core::state_machine::Status;
use flowctl_core::types::{Domain, EpicStatus, ReviewStatus};

use crate::error::DbError;

// ── Parsing helpers ─────────────────────────────────────────────────

pub(crate) fn parse_status(s: &str) -> Status {
    Status::parse(s).unwrap_or_default()
}

pub(crate) fn parse_epic_status(s: &str) -> EpicStatus {
    match s {
        "done" => EpicStatus::Done,
        _ => EpicStatus::Open,
    }
}

pub(crate) fn parse_review_status(s: &str) -> ReviewStatus {
    match s {
        "passed" => ReviewStatus::Passed,
        "failed" => ReviewStatus::Failed,
        _ => ReviewStatus::Unknown,
    }
}

pub(crate) fn parse_domain(s: &str) -> Domain {
    match s {
        "frontend" => Domain::Frontend,
        "backend" => Domain::Backend,
        "architecture" => Domain::Architecture,
        "testing" => Domain::Testing,
        "docs" => Domain::Docs,
        "ops" => Domain::Ops,
        _ => Domain::General,
    }
}

pub(crate) fn parse_datetime(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

// ── Max-ID queries ─────────────────────────────────────────────────

/// Extract the maximum epic number from existing epic IDs.
/// Epic IDs follow the format `fn-N-slug`, where N is the number.
pub async fn max_epic_num(conn: &Connection) -> Result<i64, DbError> {
    let mut rows = conn
        .query("SELECT id FROM epics", ())
        .await?;

    let mut max_n: i64 = 0;
    while let Some(row) = rows.next().await? {
        let id: String = row.get(0)?;
        if let Some(n) = parse_epic_number(&id) {
            if n > max_n {
                max_n = n;
            }
        }
    }
    Ok(max_n)
}

/// Extract the maximum task number for a given epic.
/// Task IDs follow the format `<epic-id>.N`.
pub async fn max_task_num(conn: &Connection, epic_id: &str) -> Result<i64, DbError> {
    let mut rows = conn
        .query(
            "SELECT id FROM tasks WHERE epic_id = ?1",
            params![epic_id.to_string()],
        )
        .await?;

    let mut max_n: i64 = 0;
    while let Some(row) = rows.next().await? {
        let id: String = row.get(0)?;
        if let Some(n) = parse_task_number(&id) {
            if n > max_n {
                max_n = n;
            }
        }
    }
    Ok(max_n)
}

/// Parse the numeric portion from an epic ID (fn-N or fn-N-slug).
fn parse_epic_number(id: &str) -> Option<i64> {
    let parts: Vec<&str> = id.splitn(3, '-').collect();
    if parts.len() >= 2 && parts[0] == "fn" {
        parts[1].parse::<i64>().ok()
    } else {
        None
    }
}

/// Parse the task number from a task ID (<epic-id>.N).
fn parse_task_number(id: &str) -> Option<i64> {
    let dot_pos = id.rfind('.')?;
    id[dot_pos + 1..].parse::<i64>().ok()
}
