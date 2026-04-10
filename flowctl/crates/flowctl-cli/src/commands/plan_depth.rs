//! Plan-depth command: stateless keyword detection for adaptive plan depth.
//!
//! Analyzes a request string and returns a depth classification (quick/standard/deep)
//! with the corresponding semantic step names to execute.

use serde_json::json;

use crate::output::json_output;

// ── Keyword sets ──────────────────────────────────────────────────────

const QUICK_KEYWORDS: &[&str] = &["fix", "bug", "typo", "错误", "修复"];
const DEEP_KEYWORDS: &[&str] = &["architecture", "refactor", "migration", "重构", "架构"];

// ── Step definitions ──────────────────────────────────────────────────

const QUICK_STEPS: &[&str] = &["init", "write", "validate", "return"];
const STANDARD_STEPS: &[&str] = &[
    "init",
    "clarity",
    "skill-route",
    "research",
    "gap-analysis",
    "write",
    "validate",
];
const DEEP_STEPS: &[&str] = &[
    "init",
    "clarity",
    "skill-route",
    "research",
    "gap-analysis",
    "depth",
    "constraints",
    "dependencies",
    "risks",
    "write",
    "review",
    "refine",
    "validate",
    "memory",
    "return",
];

// ── File reference counting ──────────────────────────────────────────

/// Count approximate file references in the request text.
/// Matches common patterns: paths with extensions, quoted filenames.
fn count_file_refs(request: &str) -> usize {
    request
        .split_whitespace()
        .filter(|word| {
            let w = word.trim_matches(|c: char| c == '"' || c == '\'' || c == '`' || c == ',');
            // path-like: contains / or \ with an extension
            if (w.contains('/') || w.contains('\\')) && w.contains('.') {
                return true;
            }
            // bare filename with common code extensions
            if let Some((_name, ext)) = w.rsplit_once('.') {
                matches!(
                    ext,
                    "rs" | "py"
                        | "js"
                        | "ts"
                        | "tsx"
                        | "jsx"
                        | "go"
                        | "java"
                        | "rb"
                        | "md"
                        | "json"
                        | "yaml"
                        | "yml"
                        | "toml"
                        | "css"
                        | "html"
                        | "sql"
                        | "sh"
                        | "c"
                        | "cpp"
                        | "h"
                )
            } else {
                false
            }
        })
        .count()
}

// ── Core classification ──────────────────────────────────────────────

/// Classify a request into a depth level.
fn classify(request: &str) -> (&'static str, &'static [&'static str], usize) {
    let lower = request.to_lowercase();
    let file_refs = count_file_refs(request);

    let has_quick = QUICK_KEYWORDS.iter().any(|kw| lower.contains(kw));
    let has_deep = DEEP_KEYWORDS.iter().any(|kw| lower.contains(kw));

    // Mixed keywords: take deepest
    if has_deep && file_refs >= 5 {
        return ("deep", DEEP_STEPS, 15);
    }
    // Deep keywords present (even without 5+ file refs, deep wins over quick in mixed)
    if has_deep {
        return ("deep", DEEP_STEPS, 15);
    }
    if has_quick && file_refs <= 2 {
        return ("quick", QUICK_STEPS, 4);
    }
    // Quick keywords but too many files → standard
    ("standard", STANDARD_STEPS, 10)
}

// ── Public entry point ───────────────────────────────────────────────

pub fn cmd_plan_depth(json: bool, request: &str) {
    let (depth, steps, step_count) = classify(request);

    if json {
        json_output(json!({
            "depth": depth,
            "steps": steps,
            "step_count": step_count,
        }));
    } else {
        println!("depth: {depth}");
        println!("steps: {step_count}");
        println!("sequence: {}", steps.join(", "));
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quick_detection() {
        let (depth, steps, count) = classify("fix the typo in main.rs");
        assert_eq!(depth, "quick");
        assert_eq!(count, 4);
        assert_eq!(steps, QUICK_STEPS);
    }

    #[test]
    fn test_deep_detection() {
        let (depth, steps, count) = classify(
            "refactor the architecture across src/a.rs src/b.rs src/c.rs src/d.rs src/e.rs src/f.rs",
        );
        assert_eq!(depth, "deep");
        assert_eq!(count, 15);
        assert_eq!(steps, DEEP_STEPS);
    }

    #[test]
    fn test_mixed_keywords() {
        // "fix" (quick) + "refactor" (deep) → deep wins
        let (depth, _, _) = classify("fix and refactor the auth module");
        assert_eq!(depth, "deep");
    }

    #[test]
    fn test_chinese_keywords() {
        let (depth, _, _) = classify("修复这个错误");
        assert_eq!(depth, "quick");

        let (depth, _, _) = classify("重构架构设计");
        assert_eq!(depth, "deep");
    }

    #[test]
    fn test_empty_request() {
        let (depth, steps, count) = classify("");
        assert_eq!(depth, "standard");
        assert_eq!(count, 10);
        assert_eq!(steps, STANDARD_STEPS);
    }

    #[test]
    fn test_quick_keywords_many_files_becomes_standard() {
        // Quick keyword but 3+ file refs → standard
        let (depth, _, _) = classify("fix errors in a.rs b.rs c.rs d.rs");
        assert_eq!(depth, "standard");
    }

    #[test]
    fn test_standard_default() {
        let (depth, _, count) = classify("add a new login feature");
        assert_eq!(depth, "standard");
        assert_eq!(count, 10);
    }
}
