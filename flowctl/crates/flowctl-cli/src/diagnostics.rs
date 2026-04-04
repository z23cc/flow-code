//! Rich error diagnostics via miette for CLI error reporting.
//!
//! Provides pretty-printed errors with source context for frontmatter
//! parse failures and other structured errors.

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

/// A rich diagnostic for frontmatter parse errors.
///
/// Shows the file path, the source content with the error location
/// highlighted, and a helpful suggestion.
#[derive(Debug, Error, Diagnostic)]
#[error("failed to parse frontmatter")]
#[diagnostic(
    code(flowctl::frontmatter::parse),
)]
pub struct FrontmatterDiagnostic {
    /// The source file content, named by path.
    #[source_code]
    pub src: NamedSource<String>,

    /// The span where the error occurred (byte offset + length).
    #[label("error here")]
    pub span: SourceSpan,

    /// The underlying parse error message.
    #[help]
    pub detail: String,
}

impl FrontmatterDiagnostic {
    /// Create a diagnostic from a file path, its content, and the error message.
    ///
    /// Attempts to locate the error within the source. If the error mentions
    /// a line number, uses that; otherwise highlights the frontmatter region.
    pub fn from_parse_error(path: &str, content: &str, error_msg: &str) -> Self {
        let (offset, length) = find_error_span(content, error_msg);

        FrontmatterDiagnostic {
            src: NamedSource::new(path, content.to_string()),
            span: SourceSpan::new(offset.into(), length),
            detail: error_msg.to_string(),
        }
    }
}

/// Try to find the byte offset + length for an error span.
///
/// Heuristics:
/// 1. If the error mentions "line N", highlight that line.
/// 2. If the error mentions "no closing ---", highlight end of content.
/// 3. If the error mentions "does not start with ---", highlight the first line.
/// 4. Otherwise, highlight the entire frontmatter region.
fn find_error_span(content: &str, error_msg: &str) -> (usize, usize) {
    let lower = error_msg.to_lowercase();

    // Try to extract a line number from the error message.
    if let Some(line_num) = extract_line_number(&lower) {
        if let Some((offset, len)) = line_span(content, line_num) {
            return (offset, len);
        }
    }

    // "does not start with ---" -> first line
    if lower.contains("does not start with") {
        let first_line_end = content.find('\n').unwrap_or(content.len());
        return (0, first_line_end.min(content.len()));
    }

    // "no closing ---" -> end of content
    if lower.contains("no closing") {
        let start = content.len().saturating_sub(20);
        return (start, content.len() - start);
    }

    // Default: highlight the frontmatter region (between first two ---)
    if content.starts_with("---") {
        let after_open = content.find('\n').map(|p| p + 1).unwrap_or(3);
        let close = content[after_open..].find("\n---");
        let end = close.map(|p| after_open + p + 4).unwrap_or(content.len().min(200));
        return (0, end);
    }

    // Fallback: first 80 chars
    (0, content.len().min(80))
}

/// Extract a line number from an error message (e.g., "at line 5").
fn extract_line_number(msg: &str) -> Option<usize> {
    let patterns = ["line ", "at line "];
    for pat in &patterns {
        if let Some(pos) = msg.find(pat) {
            let after = &msg[pos + pat.len()..];
            let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num_str.parse::<usize>() {
                return Some(n);
            }
        }
    }
    None
}

/// Get the byte offset and length of a 1-indexed line in content.
fn line_span(content: &str, line_num: usize) -> Option<(usize, usize)> {
    if line_num == 0 {
        return None;
    }
    let mut current_line = 1;
    let mut line_start = 0;
    for (i, ch) in content.char_indices() {
        if current_line == line_num {
            let line_end = content[i..].find('\n').map(|p| i + p).unwrap_or(content.len());
            return Some((line_start, line_end - line_start));
        }
        if ch == '\n' {
            current_line += 1;
            line_start = i + 1;
        }
    }
    if current_line == line_num {
        Some((line_start, content.len() - line_start))
    } else {
        None
    }
}

/// Report a frontmatter diagnostic to stderr using miette's handler.
pub fn report_frontmatter_error(path: &str, content: &str, error_msg: &str) {
    let diag = FrontmatterDiagnostic::from_parse_error(path, content, error_msg);
    eprintln!("{:?}", miette::Report::new(diag));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_line_number() {
        assert_eq!(extract_line_number("error at line 5"), Some(5));
        assert_eq!(extract_line_number("line 12 column 3"), Some(12));
        assert_eq!(extract_line_number("no line info"), None);
    }

    #[test]
    fn test_line_span() {
        let content = "line1\nline2\nline3\n";
        assert_eq!(line_span(content, 1), Some((0, 5)));
        assert_eq!(line_span(content, 2), Some((6, 5)));
        assert_eq!(line_span(content, 3), Some((12, 5)));
    }

    #[test]
    fn test_find_error_span_first_line() {
        let content = "not yaml\nstuff\n";
        let (offset, _len) = find_error_span(content, "does not start with ---");
        assert_eq!(offset, 0);
    }

    #[test]
    fn test_find_error_span_no_closing() {
        let content = "---\nid: test\ntitle: Test\n";
        let (offset, len) = find_error_span(content, "no closing --- delimiter");
        assert!(offset + len == content.len());
    }

    #[test]
    fn test_diagnostic_creation() {
        let path = "tasks/fn-1-test.1.md";
        let content = "---\nid: test\n: invalid\n---\n";
        let diag = FrontmatterDiagnostic::from_parse_error(
            path,
            content,
            "YAML parse error at line 3",
        );
        assert_eq!(diag.detail, "YAML parse error at line 3");
    }
}
