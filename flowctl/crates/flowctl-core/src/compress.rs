// Ported from rtk (https://github.com/rtk-ai/rtk), Apache License 2.0
// Original: src/core/toml_filter.rs + src/core/utils.rs
// Modifications: adapted for flowctl-specific filter loading (embedded at compile time),
// singleton uses std::sync::OnceLock instead of lazy_static, removed trust/discovery/registry
// layers (builtin-only), kept the 8-stage apply_filter pipeline and inline test runner.
//
// Full rtk LICENSE is preserved at flowctl/crates/flowctl-core/LICENSE-APACHE-rtk.

//! Applies TOML-defined filter rules to flowctl command output.
//!
//! Provides a declarative pipeline of 8 stages that can be configured via
//! embedded TOML files (built-in only — no runtime discovery).
//!
//! Pipeline stages (applied in order):
//!   1. strip_ansi           — remove ANSI escape codes
//!   2. replace              — regex substitutions, line-by-line, chainable
//!   3. match_output         — short-circuit: if blob matches a pattern, return message
//!   4. strip/keep_lines     — filter lines by regex
//!   5. truncate_lines_at    — truncate each line to N chars
//!   6. head/tail_lines      — keep first/last N lines
//!   7. max_lines            — absolute line cap
//!   8. on_empty             — message if result is empty

use regex::{Regex, RegexSet};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

// Built-in filters: each filter TOML is embedded individually at compile time
// via include_str! and concatenated at runtime (on first access).
const EPICS_TOML: &str = include_str!("filters/epics.toml");
const TASKS_TOML: &str = include_str!("filters/tasks.toml");
const STATUS_TOML: &str = include_str!("filters/status.toml");
const GAP_TOML: &str = include_str!("filters/gap.toml");
const MEMORY_TOML: &str = include_str!("filters/memory.toml");
const DAG_TOML: &str = include_str!("filters/dag.toml");
const FILES_TOML: &str = include_str!("filters/files.toml");
const READY_TOML: &str = include_str!("filters/ready.toml");
const HOOK_PRECOMPACT_TOML: &str = include_str!("filters/hook_precompact.toml");
const HOOK_SUBAGENT_TOML: &str = include_str!("filters/hook_subagent.toml");

// ---------------------------------------------------------------------------
// Deserialization types (TOML schema)
// ---------------------------------------------------------------------------

/// A match-output rule: if `pattern` matches anywhere in the full output blob,
/// the filter short-circuits and returns `message` immediately.
/// First matching rule wins; remaining rules are not evaluated.
/// Optional `unless`: if this regex also matches the blob, the rule is skipped
/// (prevents short-circuiting when errors or warnings are present).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MatchOutputRule {
    pattern: String,
    message: String,
    #[serde(default)]
    unless: Option<String>,
}

/// A regex substitution applied line-by-line. Rules are chained sequentially:
/// rule N+1 operates on the output of rule N.
/// Backreferences (`$1`, `$2`, ...) are supported via the `regex` crate.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaceRule {
    pattern: String,
    replacement: String,
}

/// An inline test case attached to a filter in the TOML.
/// Lives in `[[tests.<filter-name>]]` sections, separate from `[filters.*]`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomlFilterTestDef {
    pub name: String,
    pub input: String,
    pub expected: String,
}

#[derive(Deserialize)]
struct TomlFilterFile {
    schema_version: u32,
    #[serde(default)]
    filters: BTreeMap<String, TomlFilterDef>,
    /// Inline tests keyed by filter name. Kept separate from `filters` so that
    /// `TomlFilterDef` can keep `deny_unknown_fields` without touching test data.
    #[serde(default)]
    tests: BTreeMap<String, Vec<TomlFilterTestDef>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlFilterDef {
    description: Option<String>,
    #[allow(dead_code)]
    match_command: String,
    #[serde(default)]
    strip_ansi: bool,
    /// Regex substitutions, applied line-by-line before match_output (stage 2).
    #[serde(default)]
    replace: Vec<ReplaceRule>,
    /// Short-circuit rules: if the full output blob matches, return the message (stage 3).
    #[serde(default)]
    match_output: Vec<MatchOutputRule>,
    #[serde(default)]
    strip_lines_matching: Vec<String>,
    #[serde(default)]
    keep_lines_matching: Vec<String>,
    truncate_lines_at: Option<usize>,
    head_lines: Option<usize>,
    tail_lines: Option<usize>,
    max_lines: Option<usize>,
    on_empty: Option<String>,
}

// ---------------------------------------------------------------------------
// Compiled types (post-validation, ready to use)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct CompiledMatchOutputRule {
    pattern: Regex,
    message: String,
    /// If set and matches the blob, this rule is skipped (prevents swallowing errors).
    unless: Option<Regex>,
}

#[derive(Debug)]
struct CompiledReplaceRule {
    pattern: Regex,
    replacement: String,
}

#[derive(Debug)]
enum LineFilter {
    None,
    Strip(RegexSet),
    Keep(RegexSet),
}

/// A filter that has been parsed and compiled — all regexes are ready.
#[derive(Debug)]
pub struct CompiledFilter {
    pub name: String,
    #[allow(dead_code)]
    pub description: Option<String>,
    strip_ansi: bool,
    replace: Vec<CompiledReplaceRule>,
    match_output: Vec<CompiledMatchOutputRule>,
    line_filter: LineFilter,
    truncate_lines_at: Option<usize>,
    head_lines: Option<usize>,
    tail_lines: Option<usize>,
    pub max_lines: Option<usize>,
    on_empty: Option<String>,
}

// ---------------------------------------------------------------------------
// Compilation
// ---------------------------------------------------------------------------

fn compile_filter(name: String, def: TomlFilterDef) -> Result<CompiledFilter, String> {
    // Mutual exclusion: strip and keep cannot both be set
    if !def.strip_lines_matching.is_empty() && !def.keep_lines_matching.is_empty() {
        return Err("strip_lines_matching and keep_lines_matching are mutually exclusive".into());
    }

    let replace = def
        .replace
        .into_iter()
        .map(|r| {
            let pat = r.pattern.clone();
            Regex::new(&r.pattern)
                .map(|pattern| CompiledReplaceRule {
                    pattern,
                    replacement: r.replacement,
                })
                .map_err(|e| format!("invalid replace pattern '{}': {}", pat, e))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let match_output = def
        .match_output
        .into_iter()
        .map(|r| -> Result<CompiledMatchOutputRule, String> {
            let pat = r.pattern.clone();
            let pattern = Regex::new(&r.pattern)
                .map_err(|e| format!("invalid match_output pattern '{}': {}", pat, e))?;
            let unless = r
                .unless
                .as_deref()
                .map(|u| {
                    Regex::new(u)
                        .map_err(|e| format!("invalid match_output unless pattern '{}': {}", u, e))
                })
                .transpose()?;
            Ok(CompiledMatchOutputRule {
                pattern,
                message: r.message,
                unless,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let line_filter = if !def.strip_lines_matching.is_empty() {
        let set = RegexSet::new(&def.strip_lines_matching)
            .map_err(|e| format!("invalid strip_lines_matching regex: {}", e))?;
        LineFilter::Strip(set)
    } else if !def.keep_lines_matching.is_empty() {
        let set = RegexSet::new(&def.keep_lines_matching)
            .map_err(|e| format!("invalid keep_lines_matching regex: {}", e))?;
        LineFilter::Keep(set)
    } else {
        LineFilter::None
    };

    Ok(CompiledFilter {
        name,
        description: def.description,
        strip_ansi: def.strip_ansi,
        replace,
        match_output,
        line_filter,
        truncate_lines_at: def.truncate_lines_at,
        head_lines: def.head_lines,
        tail_lines: def.tail_lines,
        max_lines: def.max_lines,
        on_empty: def.on_empty,
    })
}

fn parse_and_compile(content: &str, source: &str) -> Result<Vec<CompiledFilter>, String> {
    let file: TomlFilterFile = toml::from_str(content)
        .map_err(|e| format!("TOML parse error in {}: {}", source, e))?;

    if file.schema_version != 1 {
        return Err(format!(
            "unsupported schema_version {} in {} (expected 1)",
            file.schema_version, source
        ));
    }

    let mut compiled = Vec::new();
    for (name, def) in file.filters {
        match compile_filter(name.clone(), def) {
            Ok(f) => compiled.push(f),
            Err(e) => eprintln!(
                "[flowctl:compress] warning: filter '{}' in {}: {}",
                name, source, e
            ),
        }
    }
    Ok(compiled)
}

// ---------------------------------------------------------------------------
// Registry (singleton, lazy-loaded, one-time cost)
// ---------------------------------------------------------------------------

/// Each embedded filter TOML with a source label for diagnostics.
const BUILTIN_SOURCES: &[(&str, &str)] = &[
    ("epics", EPICS_TOML),
    ("tasks", TASKS_TOML),
    ("status", STATUS_TOML),
    ("gap", GAP_TOML),
    ("memory", MEMORY_TOML),
    ("dag", DAG_TOML),
    ("files", FILES_TOML),
    ("ready", READY_TOML),
    ("hook_precompact", HOOK_PRECOMPACT_TOML),
    ("hook_subagent", HOOK_SUBAGENT_TOML),
];

fn load_registry() -> Vec<CompiledFilter> {
    let mut filters = Vec::new();
    for (source, content) in BUILTIN_SOURCES {
        match parse_and_compile(content, source) {
            Ok(f) => filters.extend(f),
            Err(e) => eprintln!("[flowctl:compress] warning: {}", e),
        }
    }
    filters
}

fn registry() -> &'static Vec<CompiledFilter> {
    static REGISTRY: OnceLock<Vec<CompiledFilter>> = OnceLock::new();
    REGISTRY.get_or_init(load_registry)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Apply a compiled filter pipeline to raw stdout. Pure String -> String.
///
/// Pipeline stages (in order):
///   1. strip_ansi           — remove ANSI escape codes
///   2. replace              — regex substitutions, line-by-line, chainable
///   3. match_output         — short-circuit if blob matches a pattern
///   4. strip/keep_lines     — filter lines by regex
///   5. truncate_lines_at    — truncate each line to N chars
///   6. head/tail_lines      — keep first/last N lines
///   7. max_lines            — absolute line cap
///   8. on_empty             — message if result is empty
pub fn apply_compiled_filter(filter: &CompiledFilter, stdout: &str) -> String {
    let mut lines: Vec<String> = stdout.lines().map(String::from).collect();

    // 1. strip_ansi
    if filter.strip_ansi {
        lines = lines.into_iter().map(|l| strip_ansi(&l)).collect();
    }

    // 2. replace — line-by-line, rules chained sequentially
    if !filter.replace.is_empty() {
        lines = lines
            .into_iter()
            .map(|mut line| {
                for rule in &filter.replace {
                    line = rule
                        .pattern
                        .replace_all(&line, rule.replacement.as_str())
                        .into_owned();
                }
                line
            })
            .collect();
    }

    // 3. match_output — short-circuit on full blob match (first rule wins)
    //    If `unless` is set and also matches the blob, the rule is skipped.
    if !filter.match_output.is_empty() {
        let blob = lines.join("\n");
        for rule in &filter.match_output {
            if rule.pattern.is_match(&blob) {
                if let Some(ref unless_re) = rule.unless {
                    if unless_re.is_match(&blob) {
                        continue; // errors/warnings present — skip this rule
                    }
                }
                return rule.message.clone();
            }
        }
    }

    // 4. strip OR keep (mutually exclusive)
    match &filter.line_filter {
        LineFilter::Strip(set) => lines.retain(|l| !set.is_match(l)),
        LineFilter::Keep(set) => lines.retain(|l| set.is_match(l)),
        LineFilter::None => {}
    }

    // 5. truncate_lines_at — uses truncate (unicode-safe)
    if let Some(max_chars) = filter.truncate_lines_at {
        lines = lines
            .into_iter()
            .map(|l| truncate(&l, max_chars))
            .collect();
    }

    // 6. head + tail
    let total = lines.len();
    if let (Some(head), Some(tail)) = (filter.head_lines, filter.tail_lines) {
        if total > head + tail {
            let mut result = lines[..head].to_vec();
            result.push(format!("... ({} lines omitted)", total - head - tail));
            result.extend_from_slice(&lines[total - tail..]);
            lines = result;
        }
    } else if let Some(head) = filter.head_lines {
        if total > head {
            lines.truncate(head);
            lines.push(format!("... ({} lines omitted)", total - head));
        }
    } else if let Some(tail) = filter.tail_lines {
        if total > tail {
            let omitted = total - tail;
            lines = lines[omitted..].to_vec();
            lines.insert(0, format!("... ({} lines omitted)", omitted));
        }
    }

    // 7. max_lines — absolute cap applied after head/tail (includes omit messages)
    if let Some(max) = filter.max_lines {
        if lines.len() > max {
            let truncated = lines.len() - max;
            lines.truncate(max);
            lines.push(format!("... ({} lines truncated)", truncated));
        }
    }

    // 8. on_empty
    let result = lines.join("\n");
    if result.trim().is_empty() {
        if let Some(ref msg) = filter.on_empty {
            return msg.clone();
        }
    }

    result
}

/// Apply the built-in filter named `filter_name` to `stdout`.
/// Returns `None` if no filter with that name exists — caller should passthrough.
pub fn apply_filter(filter_name: &str, stdout: &str) -> Option<String> {
    let reg = registry();
    let filter = reg.iter().find(|f| f.name == filter_name)?;
    Some(apply_compiled_filter(filter, stdout))
}

// ---------------------------------------------------------------------------
// Utilities (ported from rtk/src/core/utils.rs)
// ---------------------------------------------------------------------------

/// Strip ANSI escape codes (colors, styles) from a string.
pub fn strip_ansi(text: &str) -> String {
    static ANSI_RE: OnceLock<Regex> = OnceLock::new();
    let re = ANSI_RE.get_or_init(|| Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap());
    re.replace_all(text, "").to_string()
}

/// Truncate a string to `max_len` characters, appending `...` if needed.
/// Unicode-safe: counts chars, not bytes.
pub fn truncate(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_len {
        s.to_string()
    } else if max_len < 3 {
        "...".to_string()
    } else {
        format!("{}...", s.chars().take(max_len - 3).collect::<String>())
    }
}

// ---------------------------------------------------------------------------
// Inline test runner (ported from rtk)
// ---------------------------------------------------------------------------

/// Outcome of running a single inline test.
#[derive(Debug)]
pub struct TestOutcome {
    pub filter_name: String,
    pub test_name: String,
    pub passed: bool,
    pub actual: String,
    pub expected: String,
}

/// Run all inline `[[tests.<filter>]]` test cases defined in the built-in TOML files.
pub fn run_all_inline_tests() -> Vec<TestOutcome> {
    let mut outcomes = Vec::new();
    for (source, content) in BUILTIN_SOURCES {
        collect_test_outcomes(content, source, &mut outcomes);
    }
    outcomes
}

fn collect_test_outcomes(content: &str, source: &str, outcomes: &mut Vec<TestOutcome>) {
    let file: TomlFilterFile = match toml::from_str(content) {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "[flowctl:compress] warning: TOML parse error in {} during verify: {}",
                source, e
            );
            return;
        }
    };

    // Compile all filters and index by name
    let mut compiled_filters: BTreeMap<String, CompiledFilter> = BTreeMap::new();
    for (name, def) in file.filters {
        match compile_filter(name.clone(), def) {
            Ok(f) => {
                compiled_filters.insert(name, f);
            }
            Err(e) => eprintln!(
                "[flowctl:compress] warning: filter '{}' compilation error: {}",
                name, e
            ),
        }
    }

    // Run tests
    for (filter_name, tests) in file.tests {
        let compiled = match compiled_filters.get(&filter_name) {
            Some(f) => f,
            None => {
                eprintln!(
                    "[flowctl:compress] warning: [[tests.{}]] references unknown filter",
                    filter_name
                );
                continue;
            }
        };

        for test in tests {
            let actual = apply_compiled_filter(compiled, &test.input);
            // Trim trailing newlines: TOML multiline strings end with a newline
            let actual_cmp = actual.trim_end_matches('\n').to_string();
            let expected_cmp = test.expected.trim_end_matches('\n').to_string();
            outcomes.push(TestOutcome {
                filter_name: filter_name.clone(),
                test_name: test.name,
                passed: actual_cmp == expected_cmp,
                actual: actual_cmp,
                expected: expected_cmp,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn first_filter(toml_src: &str) -> CompiledFilter {
        parse_and_compile(toml_src, "test")
            .expect("test TOML should be valid")
            .into_iter()
            .next()
            .expect("expected at least one filter")
    }

    // --- Utility tests ---

    #[test]
    fn test_strip_ansi_removes_codes() {
        assert_eq!(strip_ansi("\x1b[31mError\x1b[0m"), "Error");
        assert_eq!(strip_ansi("plain"), "plain");
        assert_eq!(strip_ansi("\x1b[1;32mBold green\x1b[0m"), "Bold green");
    }

    #[test]
    fn test_truncate_unicode_safe() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
        assert_eq!(truncate("hi", 2), "hi");
        assert_eq!(truncate("日本語xyz", 5), "日本...");
    }

    // --- Pipeline tests ---

    #[test]
    fn test_strip_lines_matching_basic() {
        let f = first_filter(
            r#"
schema_version = 1
[filters.f]
match_command = "^cmd"
strip_lines_matching = ["^noise", "^verbose"]
"#,
        );
        let input = "noise line\nkeep this\nverbose stuff\nalso keep";
        let out = apply_compiled_filter(&f, input);
        assert_eq!(out, "keep this\nalso keep");
    }

    #[test]
    fn test_keep_lines_matching_basic() {
        let f = first_filter(
            r#"
schema_version = 1
[filters.f]
match_command = "^cmd"
keep_lines_matching = ["^PASS", "^FAIL"]
"#,
        );
        let input = "PASS test_a\nsome noise\nFAIL test_b\nmore noise";
        let out = apply_compiled_filter(&f, input);
        assert_eq!(out, "PASS test_a\nFAIL test_b");
    }

    #[test]
    fn test_strip_ansi_stage() {
        let f = first_filter(
            r#"
schema_version = 1
[filters.f]
match_command = "^cmd"
strip_ansi = true
"#,
        );
        let out = apply_compiled_filter(&f, "\x1b[31mError\x1b[0m\nnormal");
        assert_eq!(out, "Error\nnormal");
    }

    #[test]
    fn test_replace_chained() {
        let f = first_filter(
            r#"
schema_version = 1
[filters.f]
match_command = "^cmd"
replace = [
  { pattern = "foo", replacement = "bar" },
  { pattern = "bar", replacement = "baz" },
]
"#,
        );
        let out = apply_compiled_filter(&f, "foo\nqux");
        assert_eq!(out, "baz\nqux");
    }

    #[test]
    fn test_replace_with_backrefs() {
        let f = first_filter(
            r#"
schema_version = 1
[filters.f]
match_command = "^cmd"
replace = [
  { pattern = "^(\\w+):\\s+(.+)$", replacement = "$1=$2" },
]
"#,
        );
        let out = apply_compiled_filter(&f, "key: value\nfoo: bar");
        assert_eq!(out, "key=value\nfoo=bar");
    }

    #[test]
    fn test_max_lines_truncates() {
        let f = first_filter(
            r#"
schema_version = 1
[filters.f]
match_command = "^cmd"
max_lines = 2
"#,
        );
        let out = apply_compiled_filter(&f, "a\nb\nc\nd");
        assert!(out.contains("2 lines truncated"));
    }

    #[test]
    fn test_on_empty_message() {
        let f = first_filter(
            r#"
schema_version = 1
[filters.f]
match_command = "^cmd"
strip_lines_matching = ["."]
on_empty = "all clean"
"#,
        );
        let out = apply_compiled_filter(&f, "a\nb");
        assert_eq!(out, "all clean");
    }

    #[test]
    fn test_apply_filter_unknown_returns_none() {
        assert!(apply_filter("nonexistent_filter_name", "input").is_none());
    }

    // --- Inline test runner: runs every [[tests.X]] case in embedded TOML files ---

    #[test]
    fn test_builtin_inline_tests_all_pass() {
        let outcomes = run_all_inline_tests();
        assert!(
            !outcomes.is_empty(),
            "expected at least one inline test case in builtin filters"
        );
        let mut failed = 0;
        for outcome in &outcomes {
            if !outcome.passed {
                eprintln!(
                    "FAIL [{}::{}]\n  expected:\n{}\n  actual:\n{}",
                    outcome.filter_name, outcome.test_name, outcome.expected, outcome.actual
                );
                failed += 1;
            }
        }
        assert_eq!(
            failed, 0,
            "{}/{} inline tests failed",
            failed,
            outcomes.len()
        );
    }

    #[test]
    fn test_builtin_filters_load() {
        let reg = registry();
        assert!(!reg.is_empty(), "builtin registry should load at least one filter");
        // POC filters + fn-21 extensions
        for expected in &[
            "epics",
            "tasks",
            "status",
            "gap",
            "memory",
            "dag",
            "files",
            "ready",
            "hook_precompact",
            "hook_subagent",
        ] {
            assert!(
                reg.iter().any(|f| f.name == *expected),
                "expected builtin filter '{}' to be present",
                expected
            );
        }
    }
}
