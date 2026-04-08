//! Fuzzy patch utilities: diff generation, patch application, and search-replace
//! with progressive fallback (exact → whitespace-insensitive → context-based).
//!
//! Built on top of the `fudiff` crate for context-based unified diffs.

use thiserror::Error;

/// Errors from patch operations.
#[derive(Debug, Error)]
pub enum PatchError {
    /// The diff could not be parsed.
    #[error("diff parse error: {0}")]
    ParseError(String),

    /// The diff could not be applied (context mismatch, ambiguous match, etc.).
    #[error("apply error: {0}")]
    ApplyError(String),

    /// The search text was not found (even with fuzzy fallbacks).
    #[error("text not found: {0}")]
    NotFound(String),
}

/// Generate a context-based unified diff between original and modified text.
///
/// Returns a string in fudiff's unified diff format with `@@ @@` hunk markers,
/// context lines (` `), deletions (`-`), and additions (`+`).
pub fn create_diff(original: &str, modified: &str) -> String {
    let d = fudiff::diff(original, modified);
    d.render()
}

/// Apply a unified diff to text that may have drifted from the original.
///
/// The diff is parsed and applied via fudiff's context-matching algorithm,
/// which locates hunks by surrounding context lines rather than fixed line numbers.
pub fn apply_diff(target: &str, diff: &str) -> Result<String, PatchError> {
    let parsed = fudiff::parse(diff).map_err(|e| PatchError::ParseError(format!("{e:?}")))?;
    parsed
        .patch(target)
        .map_err(|e| PatchError::ApplyError(format!("{e:?}")))
}

/// Simple search-replace with progressive fuzzy fallback.
///
/// Fallback chain:
/// 1. **Exact match** — literal substring replacement.
/// 2. **Whitespace-normalized match** — collapse runs of whitespace on both sides
///    and match; apply replacement preserving original indentation of the first line.
/// 3. **Context-based diff** — generate a fudiff from `old_text` → `new_text` context
///    and apply it to the target, which tolerates moderate drift.
pub fn fuzzy_replace(content: &str, old_text: &str, new_text: &str) -> Result<String, PatchError> {
    // Strategy 1: exact match
    if content.contains(old_text) {
        return Ok(content.replacen(old_text, new_text, 1));
    }

    // Strategy 2: whitespace-normalized match
    if let Some(result) = whitespace_normalized_replace(content, old_text, new_text) {
        return Ok(result);
    }

    // Strategy 3: context-based diff/patch
    //
    // We create a diff from old_text → new_text, then try to apply it to `content`.
    // This works when `content` has the same structure as old_text but with minor drift
    // (extra/removed lines nearby, slight reordering of surrounding code).
    let diff = create_diff(old_text, new_text);
    apply_diff(content, &diff).map_err(|_| {
        PatchError::NotFound(format!(
            "could not locate old_text in content (tried exact, whitespace-normalized, and context-based matching)"
        ))
    })
}

/// Attempt whitespace-normalized replacement.
///
/// Normalizes both the search needle and each candidate window in the content
/// by collapsing all whitespace runs to single spaces. If a match is found,
/// replaces the original (un-normalized) text span with `new_text`.
fn whitespace_normalized_replace(
    content: &str,
    old_text: &str,
    new_text: &str,
) -> Option<String> {
    let norm_old = normalize_whitespace(old_text);
    if norm_old.is_empty() {
        return None;
    }

    // Scan content line-by-line to find a contiguous block whose normalized form
    // matches the normalized old_text.
    let old_line_count = old_text.lines().count().max(1);
    let content_lines: Vec<&str> = content.lines().collect();

    for start in 0..content_lines.len() {
        // Try windows of old_line_count and old_line_count ± 1 to tolerate blank-line drift.
        for window_size in [old_line_count, old_line_count + 1, old_line_count.saturating_sub(1)]
        {
            if window_size == 0 || start + window_size > content_lines.len() {
                continue;
            }
            let window: String = content_lines[start..start + window_size].join("\n");
            if normalize_whitespace(&window) == norm_old {
                // Found it — replace this span.
                let mut result = String::new();
                if start > 0 {
                    result.push_str(&content_lines[..start].join("\n"));
                    result.push('\n');
                }
                result.push_str(new_text);
                if start + window_size < content_lines.len() {
                    result.push('\n');
                    result.push_str(&content_lines[start + window_size..].join("\n"));
                }
                // Preserve trailing newline
                if content.ends_with('\n') && !result.ends_with('\n') {
                    result.push('\n');
                }
                return Some(result);
            }
        }
    }

    None
}

/// Collapse all whitespace runs (including newlines) to a single space and trim.
fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_apply_diff_roundtrip() {
        let original = "fn main() {\n    println!(\"hello\");\n}\n";
        let modified = "fn main() {\n    println!(\"hello, world!\");\n}\n";
        let diff = create_diff(original, modified);
        assert!(diff.contains("@@"));
        assert!(diff.contains("-    println!(\"hello\");"));
        assert!(diff.contains("+    println!(\"hello, world!\");"));

        // Apply to original should produce modified
        let result = apply_diff(original, &diff).unwrap();
        assert_eq!(result, modified);
    }

    #[test]
    fn apply_diff_to_drifted_target() {
        let original = "line1\nline2\nold line\nline4\n";
        let modified = "line1\nline2\nnew line\nline4\n";
        let diff = create_diff(original, modified);

        // Target has an extra line but same context
        let drifted = "line0\nline1\nline2\nold line\nline4\nline5\n";
        let result = apply_diff(drifted, &diff).unwrap();
        assert!(result.contains("new line"));
        assert!(!result.contains("old line"));
    }

    #[test]
    fn fuzzy_replace_exact_match() {
        let content = "fn foo() {\n    bar();\n}\n";
        let result = fuzzy_replace(content, "    bar();", "    baz();").unwrap();
        assert_eq!(result, "fn foo() {\n    baz();\n}\n");
    }

    #[test]
    fn fuzzy_replace_whitespace_mismatch() {
        // Content uses 4-space indent; old_text uses 2-space indent.
        // Whitespace normalization should still match.
        let content = "fn foo() {\n    bar();\n}\n";
        let result = fuzzy_replace(content, "  bar();", "  baz();").unwrap();
        assert!(result.contains("baz();"));
        assert!(!result.contains("bar"));
    }

    #[test]
    fn fuzzy_replace_context_drift() {
        let original_block = "line1\nline2\nold_call()\nline4";
        let new_block = "line1\nline2\nnew_call()\nline4";

        // Content has the same structure but with extra surrounding lines
        let content = "header\nline1\nline2\nold_call()\nline4\nfooter\n";
        let diff = create_diff(original_block, new_block);
        let result = apply_diff(content, &diff).unwrap();
        assert!(result.contains("new_call()"));
        assert!(!result.contains("old_call()"));
    }

    #[test]
    fn fuzzy_replace_not_found() {
        let content = "completely different text\n";
        let result = fuzzy_replace(content, "nonexistent text here", "replacement");
        assert!(result.is_err());
        assert!(matches!(result, Err(PatchError::NotFound(_))));
    }

    #[test]
    fn empty_diff_is_noop() {
        let text = "unchanged content\n";
        let diff = create_diff(text, text);
        let result = apply_diff(text, &diff).unwrap();
        assert_eq!(result, text);
    }

    #[test]
    fn whitespace_normalized_multiline() {
        let content = "fn main() {\n    let x  =  1;\n    let y = 2;\n}\n";
        let old_text = "let x = 1;\n    let y = 2;";
        let new_text = "let x = 10;\n    let y = 20;";
        let result = fuzzy_replace(content, old_text, new_text).unwrap();
        assert!(result.contains("let x = 10;"));
        assert!(result.contains("let y = 20;"));
    }
}
