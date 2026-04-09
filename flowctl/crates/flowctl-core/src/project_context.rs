//! Parser for `.flow/project-context.md` — extracts structured project metadata
//! from a human-readable Markdown file.

#![allow(clippy::manual_map)]

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ── Types ────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProjectContext {
    pub technology_stack: Vec<String>,
    pub guard_commands: GuardCommands,
    pub critical_rules: Vec<String>,
    pub file_conventions: HashMap<String, Vec<String>>,
    pub architecture_decisions: Vec<String>,
    pub non_goals: Vec<String>,
    pub raw: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GuardCommands {
    pub test: Option<String>,
    pub lint: Option<String>,
    pub typecheck: Option<String>,
    pub format_check: Option<String>,
}

// ── Parsing ──────────────────────────────────────────────────────────────

impl ProjectContext {
    /// Load and parse from `.flow/project-context.md`.
    /// Returns `None` if the file does not exist or cannot be read.
    pub fn load(flow_dir: &Path) -> Option<Self> {
        let path = flow_dir.join("project-context.md");
        let raw = std::fs::read_to_string(&path).ok()?;
        Some(Self::parse(&raw))
    }

    /// Load from three-layer resolution (tries .flow-config/ first, then .flow/).
    pub fn load_resolved() -> Option<Self> {
        let paths = crate::paths::FlowPaths::resolve()?;
        let path = paths.project_context();
        let raw = std::fs::read_to_string(&path).ok()?;
        Some(Self::parse(&raw))
    }

    /// Parse a markdown string into a `ProjectContext`.
    pub fn parse(raw: &str) -> Self {
        let sections = split_sections(raw);
        let mut ctx = ProjectContext {
            raw: raw.to_string(),
            ..Default::default()
        };

        for (heading, body) in &sections {
            match heading.as_str() {
                "Technology Stack" => ctx.technology_stack = extract_bullets(body),
                "Guard Commands" => ctx.guard_commands = parse_guard_commands(body),
                "Critical Implementation Rules" => ctx.critical_rules = extract_bullets(body),
                "File Conventions" => ctx.file_conventions = parse_file_conventions(body),
                "Architecture Decisions" => ctx.architecture_decisions = extract_bullets(body),
                "Non-Goals" => ctx.non_goals = extract_bullets(body),
                _ => {} // unknown sections are silently ignored
            }
        }

        ctx
    }

    /// Check if a file path matches a domain's conventions.
    /// Returns the first matching domain name, or `None`.
    pub fn infer_domain(&self, file_path: &str) -> Option<String> {
        for (domain, patterns) in &self.file_conventions {
            for pattern in patterns {
                if matches_pattern(file_path, pattern) {
                    return Some(domain.clone());
                }
            }
        }
        None
    }

    /// Check if a proposal text conflicts with any non-goals.
    /// Returns the non-goal strings whose keywords appear in the text.
    pub fn conflicts_with_non_goals(&self, text: &str) -> Vec<&str> {
        let lower = text.to_lowercase();
        self.non_goals
            .iter()
            .filter(|ng| {
                // Extract meaningful words (4+ chars) from the non-goal
                ng.to_lowercase()
                    .split_whitespace()
                    .filter(|w| w.len() >= 4)
                    // Skip common stop words
                    .filter(|w| !matches!(*w, "does" | "have" | "with" | "from" | "that" | "this" | "when" | "also" | "been" | "into"))
                    .any(|kw| lower.contains(&kw))
            })
            .map(|s| s.as_str())
            .collect()
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────

/// Split markdown by `## ` headers. Returns `(heading_text, body_text)` pairs.
fn split_sections(raw: &str) -> Vec<(String, String)> {
    let mut sections = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_body = String::new();

    for line in raw.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            // Flush previous section
            if let Some(h) = current_heading.take() {
                sections.push((h, current_body.clone()));
                current_body.clear();
            }
            current_heading = Some(heading.trim().to_string());
        } else if current_heading.is_some() {
            current_body.push_str(line);
            current_body.push('\n');
        }
    }
    // Flush last section
    if let Some(h) = current_heading {
        sections.push((h, current_body));
    }

    sections
}

/// Extract bullet items (`- text`) from a section body.
fn extract_bullets(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("- ") {
                Some(rest.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Extract a fenced yaml code block from a section body.
fn extract_yaml_block(body: &str) -> Option<String> {
    let mut in_yaml = false;
    let mut yaml = String::new();

    for line in body.lines() {
        let trimmed = line.trim();
        if !in_yaml && (trimmed == "```yaml" || trimmed == "```yml") {
            in_yaml = true;
            continue;
        }
        if in_yaml {
            if trimmed == "```" {
                break;
            }
            yaml.push_str(line);
            yaml.push('\n');
        }
    }

    if yaml.is_empty() { None } else { Some(yaml) }
}

/// Parse guard commands from a yaml block: `key: "value"` pairs.
fn parse_guard_commands(body: &str) -> GuardCommands {
    let yaml = match extract_yaml_block(body) {
        Some(y) => y,
        None => return GuardCommands::default(),
    };

    let mut gc = GuardCommands::default();
    for line in yaml.lines() {
        let trimmed = line.trim();
        if let Some((key, val)) = trimmed.split_once(':') {
            let key = key.trim();
            let val = val.trim().trim_matches('"');
            let val_opt = if val.is_empty() { None } else { Some(val.to_string()) };
            match key {
                "test" => gc.test = val_opt,
                "lint" => gc.lint = val_opt,
                "typecheck" => gc.typecheck = val_opt,
                "format_check" => gc.format_check = val_opt,
                _ => {}
            }
        }
    }
    gc
}

/// Parse file conventions from a yaml block: `domain: ["pattern1", "pattern2"]`.
fn parse_file_conventions(body: &str) -> HashMap<String, Vec<String>> {
    let yaml = match extract_yaml_block(body) {
        Some(y) => y,
        None => return HashMap::new(),
    };

    let mut map = HashMap::new();
    for line in yaml.lines() {
        let trimmed = line.trim();
        if let Some((key, val)) = trimmed.split_once(':') {
            let key = key.trim().to_string();
            // Parse ["pattern1", "pattern2"] style arrays
            let val = val.trim();
            if let (Some(start), Some(end)) = (val.find('['), val.rfind(']')) {
                let inner = &val[start + 1..end];
                let patterns: Vec<String> = inner
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                map.insert(key, patterns);
            }
        }
    }
    map
}

/// Simple pattern matching for file conventions:
/// - Patterns ending with `/` are directory prefixes
/// - Patterns starting with `*` are extension/suffix matches
/// - Patterns starting with `**/*` are suffix matches anywhere
/// - Otherwise, exact prefix match
fn matches_pattern(file_path: &str, pattern: &str) -> bool {
    if pattern.ends_with('/') {
        // Directory prefix match
        file_path.starts_with(pattern) || file_path.starts_with(pattern.trim_end_matches('/'))
    } else if let Some(suffix) = pattern.strip_prefix("**/") {
        // Recursive glob: match suffix anywhere
        // Handle patterns like *_test.* — convert wildcards to simple contains/ends_with
        if let Some(ext) = suffix.strip_prefix("*") {
            // **/*_test.* → check if filename contains "_test." (wildcard at end)
            let ext_trimmed = ext.trim_end_matches('*').trim_end_matches('.');
            if ext != ext_trimmed {
                // Had trailing wildcard — match the core substring in the filename
                let fname = file_path.rsplit('/').next().unwrap_or(file_path);
                fname.contains(ext_trimmed.trim_start_matches('.'))
            } else {
                file_path.ends_with(ext)
            }
        } else {
            file_path.ends_with(suffix) || file_path.contains(suffix)
        }
    } else if let Some(ext) = pattern.strip_prefix("*.") {
        // Extension match
        file_path.ends_with(&format!(".{ext}"))
    } else if let Some(ext) = pattern.strip_prefix("*") {
        // Suffix match
        file_path.ends_with(ext)
    } else {
        // Prefix match
        file_path.starts_with(pattern)
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const FULL_EXAMPLE: &str = r#"# Project Context

## Technology Stack
- Language: Rust
- Testing: cargo test
- Framework: Actix-web

## Guard Commands
```yaml
test: "cargo test --all"
lint: "cargo clippy --all -- -D warnings"
typecheck: ""
format_check: "cargo fmt --all -- --check"
```

## Critical Implementation Rules
- unsafe_code = "forbid"
- Edition 2024 patterns
- No unwrap in library code

## File Conventions
```yaml
frontend: ["src/components/", "*.tsx"]
backend: ["src/api/", "crates/"]
testing: ["tests/", "**/*_test.*"]
docs: ["docs/", "*.md"]
```

## Architecture Decisions
- Chose nucleo-matcher over frizbee
- JSON file storage, no database

## Non-Goals
- Do not add async runtime
- No GraphQL support
"#;

    #[test]
    fn test_missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let result = ProjectContext::load(dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_empty_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("project-context.md");
        std::fs::write(&path, "").unwrap();
        let ctx = ProjectContext::load(dir.path()).unwrap();
        assert!(ctx.technology_stack.is_empty());
        assert!(ctx.guard_commands.test.is_none());
        assert!(ctx.critical_rules.is_empty());
        assert!(ctx.file_conventions.is_empty());
    }

    #[test]
    fn test_parse_full_example() {
        let ctx = ProjectContext::parse(FULL_EXAMPLE);

        // Technology Stack
        assert_eq!(ctx.technology_stack.len(), 3);
        assert_eq!(ctx.technology_stack[0], "Language: Rust");
        assert_eq!(ctx.technology_stack[1], "Testing: cargo test");

        // Guard Commands
        assert_eq!(ctx.guard_commands.test.as_deref(), Some("cargo test --all"));
        assert_eq!(
            ctx.guard_commands.lint.as_deref(),
            Some("cargo clippy --all -- -D warnings")
        );
        assert!(ctx.guard_commands.typecheck.is_none()); // empty string → None
        assert_eq!(
            ctx.guard_commands.format_check.as_deref(),
            Some("cargo fmt --all -- --check")
        );

        // Critical rules
        assert_eq!(ctx.critical_rules.len(), 3);
        assert!(ctx.critical_rules[0].contains("forbid"));

        // File conventions
        assert_eq!(ctx.file_conventions.len(), 4);
        assert_eq!(
            ctx.file_conventions.get("frontend").unwrap(),
            &["src/components/", "*.tsx"]
        );
        assert_eq!(
            ctx.file_conventions.get("testing").unwrap(),
            &["tests/", "**/*_test.*"]
        );

        // Architecture decisions
        assert_eq!(ctx.architecture_decisions.len(), 2);
        assert!(ctx.architecture_decisions[0].contains("nucleo"));

        // Non-goals
        assert_eq!(ctx.non_goals.len(), 2);
        assert!(ctx.non_goals[0].contains("async"));

        // Raw preserved
        assert!(ctx.raw.contains("# Project Context"));
    }

    #[test]
    fn test_infer_domain() {
        let ctx = ProjectContext::parse(FULL_EXAMPLE);

        assert_eq!(ctx.infer_domain("src/components/Button.tsx"), Some("frontend".into()));
        assert_eq!(ctx.infer_domain("src/api/handler.rs"), Some("backend".into()));
        assert_eq!(ctx.infer_domain("crates/core/lib.rs"), Some("backend".into()));
        assert_eq!(ctx.infer_domain("tests/unit_test.rs"), Some("testing".into()));
        assert_eq!(ctx.infer_domain("docs/README.md"), Some("docs".into()));
        assert_eq!(ctx.infer_domain("random/file.py"), None);
    }

    #[test]
    fn test_guard_commands_parsed() {
        let ctx = ProjectContext::parse(FULL_EXAMPLE);
        assert_eq!(ctx.guard_commands.test.as_deref(), Some("cargo test --all"));
        assert_eq!(
            ctx.guard_commands.lint.as_deref(),
            Some("cargo clippy --all -- -D warnings")
        );
        assert!(ctx.guard_commands.typecheck.is_none());
        assert!(ctx.guard_commands.format_check.is_some());
    }

    #[test]
    fn test_conflicts_with_non_goals() {
        let ctx = ProjectContext::parse(FULL_EXAMPLE);

        let conflicts = ctx.conflicts_with_non_goals("Let's add an async runtime for better performance");
        assert!(!conflicts.is_empty());
        assert!(conflicts.iter().any(|c| c.contains("async")));

        let conflicts = ctx.conflicts_with_non_goals("Add a REST endpoint");
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_load_from_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("project-context.md");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(f, "{}", FULL_EXAMPLE).unwrap();

        let ctx = ProjectContext::load(dir.path()).unwrap();
        assert_eq!(ctx.technology_stack.len(), 3);
        assert!(ctx.guard_commands.test.is_some());
    }

    #[test]
    fn test_pattern_matching() {
        // Directory prefix
        assert!(matches_pattern("src/components/Button.tsx", "src/components/"));
        // Extension
        assert!(matches_pattern("App.tsx", "*.tsx"));
        assert!(!matches_pattern("App.ts", "*.tsx"));
        // Recursive glob
        assert!(matches_pattern("src/foo_test.rs", "**/*_test.*"));
        // Prefix
        assert!(matches_pattern("crates/core/lib.rs", "crates/"));
        assert!(!matches_pattern("src/main.rs", "crates/"));
    }
}
