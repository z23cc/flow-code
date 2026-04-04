//! YAML frontmatter parser and writer for `.flow/` Markdown files.
//!
//! Frontmatter is delimited by `---` on its own line. Only the first pair
//! of `---` markers is treated as frontmatter; subsequent `---` in the
//! Markdown body are left untouched.
//!
//! Uses `serde_saphyr` (not the deprecated `serde_yaml` or RUSTSEC-flagged
//! `serde_yml`).

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::CoreError;

/// A parsed Markdown document with YAML frontmatter and a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document<T> {
    /// Deserialized frontmatter.
    pub frontmatter: T,
    /// Markdown body after the closing `---`.
    pub body: String,
}

/// Parse a Markdown string into frontmatter + body.
///
/// The input must start with `---\n`. The frontmatter ends at the next
/// `---\n` (or `---` at EOF). Everything after that second delimiter is
/// the body.
pub fn parse<T: DeserializeOwned>(input: &str) -> Result<Document<T>, CoreError> {
    let trimmed = input.trim_start();

    if !trimmed.starts_with("---") {
        return Err(CoreError::FrontmatterParse(
            "document does not start with ---".to_string(),
        ));
    }

    // Skip the opening "---" line.
    let after_open = match trimmed.strip_prefix("---") {
        Some(rest) => {
            // Consume the newline (or the rest if it's just "---" at EOF).
            rest.strip_prefix('\n').unwrap_or(rest)
        }
        None => unreachable!(),
    };

    // Find the closing "---".
    let (yaml_str, body) = find_closing_delimiter(after_open)?;

    let frontmatter: T = serde_saphyr::from_str(yaml_str).map_err(|e| {
        CoreError::FrontmatterParse(format!("YAML parse error: {e}"))
    })?;

    Ok(Document {
        frontmatter,
        body: body.to_string(),
    })
}

/// Serialize frontmatter + body back to a Markdown string.
pub fn write<T: Serialize>(doc: &Document<T>) -> Result<String, CoreError> {
    let yaml = serde_saphyr::to_string(&doc.frontmatter).map_err(|e| {
        CoreError::FrontmatterSerialize(format!("YAML serialize error: {e}"))
    })?;

    let mut out = String::with_capacity(yaml.len() + doc.body.len() + 16);
    out.push_str("---\n");
    out.push_str(&yaml);
    // serde_saphyr::to_string includes a trailing newline; ensure exactly one.
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("---\n");
    out.push_str(&doc.body);
    Ok(out)
}

/// Parse only the frontmatter, discarding the body.
pub fn parse_frontmatter<T: DeserializeOwned>(input: &str) -> Result<T, CoreError> {
    parse(input).map(|doc| doc.frontmatter)
}

/// Find the closing `---` delimiter and split into (yaml, body).
fn find_closing_delimiter(s: &str) -> Result<(&str, &str), CoreError> {
    // Search for "\n---\n" or "\n---" at end of string.
    let mut search_from = 0;
    while search_from < s.len() {
        if let Some(pos) = s[search_from..].find("\n---") {
            let abs_pos = search_from + pos;
            let after_dashes = abs_pos + 4; // skip "\n---"

            // The "---" must be the entire line (not "----" or "--- text").
            if after_dashes >= s.len() {
                // "---" at end of string.
                return Ok((&s[..abs_pos], ""));
            }

            let next_char = s.as_bytes()[after_dashes];
            if next_char == b'\n' {
                let body_start = after_dashes + 1;
                return Ok((&s[..abs_pos], &s[body_start..]));
            } else if next_char == b'\r' {
                // Handle \r\n.
                let body_start = if after_dashes + 1 < s.len()
                    && s.as_bytes()[after_dashes + 1] == b'\n'
                {
                    after_dashes + 2
                } else {
                    after_dashes + 1
                };
                return Ok((&s[..abs_pos], &s[body_start..]));
            }

            // Not a clean delimiter (e.g. "----"), keep searching.
            search_from = after_dashes;
        } else {
            break;
        }
    }

    // Also handle the case where the YAML is empty and closing --- is the first line.
    if s.starts_with("---\n") {
        return Ok(("", &s[4..]));
    }
    if s.starts_with("---") && s.len() == 3 {
        return Ok(("", ""));
    }

    Err(CoreError::FrontmatterParse(
        "no closing --- delimiter found".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state_machine::Status;
    use crate::types::{Domain, Epic, EpicStatus, ReviewStatus, Task};
    use chrono::Utc;

    #[test]
    fn test_parse_epic_frontmatter() {
        let input = r#"---
schema_version: 1
id: fn-1-add-auth
title: Add Authentication
status: open
plan_review: unknown
created_at: "2026-01-01T00:00:00Z"
updated_at: "2026-01-01T00:00:00Z"
---
## Description
Add OAuth2 authentication.
"#;
        let doc: Document<Epic> = parse(input).unwrap();
        assert_eq!(doc.frontmatter.id, "fn-1-add-auth");
        assert_eq!(doc.frontmatter.title, "Add Authentication");
        assert_eq!(doc.frontmatter.status, EpicStatus::Open);
        assert_eq!(doc.frontmatter.schema_version, 1);
        assert!(doc.body.contains("## Description"));
    }

    #[test]
    fn test_parse_task_frontmatter() {
        let input = r#"---
schema_version: 1
id: fn-1-add-auth.1
epic: fn-1-add-auth
title: Design Auth Flow
status: todo
domain: backend
depends_on:
  - fn-1-add-auth.0
files:
  - src/auth.rs
created_at: "2026-01-01T00:00:00Z"
updated_at: "2026-01-01T00:00:00Z"
---
## Description
Design the auth flow.

## Acceptance
- Flow documented
"#;
        let doc: Document<Task> = parse(input).unwrap();
        assert_eq!(doc.frontmatter.id, "fn-1-add-auth.1");
        assert_eq!(doc.frontmatter.epic, "fn-1-add-auth");
        assert_eq!(doc.frontmatter.status, Status::Todo);
        assert_eq!(doc.frontmatter.domain, Domain::Backend);
        assert_eq!(doc.frontmatter.depends_on, vec!["fn-1-add-auth.0"]);
        assert_eq!(doc.frontmatter.files, vec!["src/auth.rs"]);
    }

    #[test]
    fn test_roundtrip_epic() {
        let epic = Epic {
            schema_version: 1,
            id: "fn-2-rewrite".to_string(),
            title: "Rust Rewrite".to_string(),
            status: EpicStatus::Open,
            branch_name: Some("feat/rust-rewrite".to_string()),
            plan_review: ReviewStatus::Passed,
            completion_review: ReviewStatus::Unknown,
            depends_on_epics: vec!["fn-1-base".to_string()],
            default_impl: None,
            default_review: None,
            default_sync: None,
            file_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let doc = Document {
            frontmatter: epic,
            body: "# Epic body\n".to_string(),
        };

        let serialized = write(&doc).unwrap();
        let parsed: Document<Epic> = parse(&serialized).unwrap();

        assert_eq!(parsed.frontmatter.id, doc.frontmatter.id);
        assert_eq!(parsed.frontmatter.title, doc.frontmatter.title);
        assert_eq!(parsed.frontmatter.status, doc.frontmatter.status);
        assert_eq!(parsed.frontmatter.branch_name, doc.frontmatter.branch_name);
        assert_eq!(parsed.frontmatter.plan_review, doc.frontmatter.plan_review);
        assert_eq!(
            parsed.frontmatter.depends_on_epics,
            doc.frontmatter.depends_on_epics
        );
        assert_eq!(parsed.body, doc.body);
    }

    #[test]
    fn test_roundtrip_task() {
        let task = Task {
            schema_version: 1,
            id: "fn-1-test.3".to_string(),
            epic: "fn-1-test".to_string(),
            title: "Write Tests".to_string(),
            status: Status::InProgress,
            priority: Some(2),
            domain: Domain::Testing,
            depends_on: vec!["fn-1-test.1".to_string(), "fn-1-test.2".to_string()],
            files: vec!["tests/auth.rs".to_string()],
            r#impl: None,
            review: None,
            sync: None,
            file_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let doc = Document {
            frontmatter: task,
            body: "## Description\nTest stuff.\n".to_string(),
        };

        let serialized = write(&doc).unwrap();
        let parsed: Document<Task> = parse(&serialized).unwrap();

        assert_eq!(parsed.frontmatter.id, doc.frontmatter.id);
        assert_eq!(parsed.frontmatter.epic, doc.frontmatter.epic);
        assert_eq!(parsed.frontmatter.status, doc.frontmatter.status);
        assert_eq!(parsed.frontmatter.priority, doc.frontmatter.priority);
        assert_eq!(parsed.frontmatter.domain, doc.frontmatter.domain);
        assert_eq!(parsed.frontmatter.depends_on, doc.frontmatter.depends_on);
        assert_eq!(parsed.frontmatter.files, doc.frontmatter.files);
        assert_eq!(parsed.body, doc.body);
    }

    #[test]
    fn test_body_with_triple_dashes() {
        // Only the first --- pair is frontmatter; body can contain ---.
        let input = "---\nschema_version: 1\nid: fn-1-test\ntitle: Test\nstatus: open\nplan_review: unknown\ncreated_at: \"2026-01-01T00:00:00Z\"\nupdated_at: \"2026-01-01T00:00:00Z\"\n---\n## Section\n\n---\n\nMore content after horizontal rule.\n";
        let doc: Document<Epic> = parse(input).unwrap();
        assert!(doc.body.contains("---"));
        assert!(doc.body.contains("More content after horizontal rule."));
    }

    #[test]
    fn test_missing_frontmatter_delimiter() {
        let input = "No frontmatter here.\n";
        let result = parse::<Epic>(input);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not start with ---"), "Got: {err}");
    }

    #[test]
    fn test_no_closing_delimiter() {
        let input = "---\nid: test\ntitle: Test\n";
        let result = parse::<Epic>(input);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("no closing ---"), "Got: {err}");
    }

    #[test]
    fn test_invalid_yaml() {
        let input = "---\n: : : invalid yaml [[[\n---\n";
        let result = parse::<Epic>(input);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("YAML parse error"), "Got: {err}");
    }

    #[test]
    fn test_missing_required_field() {
        // Epic requires id and title.
        let input = "---\nschema_version: 1\nstatus: open\n---\n";
        let result = parse::<Epic>(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_default_optional_fields() {
        // Minimal task with only required fields.
        let input = "---\nid: fn-1-t.1\nepic: fn-1-t\ntitle: Minimal\ncreated_at: \"2026-01-01T00:00:00Z\"\nupdated_at: \"2026-01-01T00:00:00Z\"\n---\n";
        let doc: Document<Task> = parse(input).unwrap();
        assert_eq!(doc.frontmatter.status, Status::Todo);
        assert_eq!(doc.frontmatter.domain, Domain::General);
        assert!(doc.frontmatter.depends_on.is_empty());
        assert!(doc.frontmatter.files.is_empty());
        assert_eq!(doc.frontmatter.priority, None);
    }

    #[test]
    fn test_empty_body() {
        let input = "---\nschema_version: 1\nid: fn-1-test\ntitle: Test\nstatus: open\nplan_review: unknown\ncreated_at: \"2026-01-01T00:00:00Z\"\nupdated_at: \"2026-01-01T00:00:00Z\"\n---\n";
        let doc: Document<Epic> = parse(input).unwrap();
        assert_eq!(doc.body, "");
    }

    #[test]
    fn test_parse_frontmatter_only() {
        let input = "---\nschema_version: 1\nid: fn-1-test\ntitle: Test\nstatus: open\nplan_review: unknown\ncreated_at: \"2026-01-01T00:00:00Z\"\nupdated_at: \"2026-01-01T00:00:00Z\"\n---\nBody ignored.\n";
        let epic: Epic = parse_frontmatter(input).unwrap();
        assert_eq!(epic.id, "fn-1-test");
    }

    #[test]
    fn test_schema_version_defaults_to_1() {
        let input = "---\nid: fn-1-test\ntitle: Test\nstatus: open\ncreated_at: \"2026-01-01T00:00:00Z\"\nupdated_at: \"2026-01-01T00:00:00Z\"\n---\n";
        let doc: Document<Epic> = parse(input).unwrap();
        assert_eq!(doc.frontmatter.schema_version, 1);
    }
}
