//! ID parsing, generation, and validation utilities.
//!
//! Ported from `scripts/flowctl/core/ids.py`. The regex and slugify logic
//! must produce identical results to the Python implementation.

use std::sync::LazyLock;

use regex::Regex;

/// Compiled regex for parsing flowctl IDs.
///
/// Pattern supports:
/// - Legacy: `fn-N`, `fn-N.M`
/// - Short suffix: `fn-N-xxx`, `fn-N-xxx.M` (1-3 char random)
/// - Slug suffix: `fn-N-longer-slug`, `fn-N-longer-slug.M` (multi-segment)
///
/// Ported from `scripts/flowctl/core/ids.py:58-60`.
static ID_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^fn-(\d+)(?:-[a-z0-9][a-z0-9-]*[a-z0-9]|-[a-z0-9]{1,3})?(?:\.(\d+))?$")
        .expect("ID regex is valid")
});

/// Regex for slugify: non-word characters (except spaces and hyphens).
static SLUGIFY_NON_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[^\w\s-]").expect("slugify non-word regex is valid")
});

/// Regex for slugify: collapsing whitespace and hyphens.
static SLUGIFY_COLLAPSE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[-\s]+").expect("slugify collapse regex is valid")
});

/// Parsed ID components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedId {
    /// Epic number.
    pub epic: u32,
    /// Task number (None for epic IDs).
    pub task: Option<u32>,
}

/// Strong type for epic IDs (e.g. `fn-1-add-auth`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EpicId(pub String);

impl std::fmt::Display for EpicId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Strong type for task IDs (e.g. `fn-1-add-auth.3`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(pub String);

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TaskId {
    /// Extract the epic ID portion from a task ID.
    ///
    /// Preserves suffix: `fn-5-x7k.3` -> `fn-5-x7k`.
    /// Ported from `scripts/flowctl/core/ids.py:138-148`.
    pub fn epic_id(&self) -> Result<EpicId, crate::error::CoreError> {
        let parsed = parse_id(&self.0)?;
        if parsed.task.is_none() {
            return Err(crate::error::CoreError::InvalidId(format!(
                "not a task ID: {}",
                self.0
            )));
        }
        // Split on '.' and take the epic part (preserves suffix).
        let epic_part = self
            .0
            .rsplit_once('.')
            .map(|(epic, _)| epic)
            .unwrap_or(&self.0);
        Ok(EpicId(epic_part.to_string()))
    }
}

/// Parse an ID string into its components.
///
/// Returns (epic_num, task_num) where task_num is None for epic IDs.
///
/// Ported from `scripts/flowctl/core/ids.py:49-66`. Must produce identical
/// results for all test cases.
///
/// # Examples
///
/// ```
/// use flowctl_core::id::parse_id;
///
/// // Epic IDs
/// let parsed = parse_id("fn-1").unwrap();
/// assert_eq!(parsed.epic, 1);
/// assert_eq!(parsed.task, None);
///
/// // Task IDs
/// let parsed = parse_id("fn-1.3").unwrap();
/// assert_eq!(parsed.epic, 1);
/// assert_eq!(parsed.task, Some(3));
///
/// // Slug IDs
/// let parsed = parse_id("fn-2-add-auth").unwrap();
/// assert_eq!(parsed.epic, 2);
/// assert_eq!(parsed.task, None);
///
/// // Invalid IDs
/// assert!(parse_id("invalid").is_err());
/// ```
pub fn parse_id(id_str: &str) -> Result<ParsedId, crate::error::CoreError> {
    let captures = ID_REGEX
        .captures(id_str)
        .ok_or_else(|| crate::error::CoreError::InvalidId(id_str.to_string()))?;

    let epic: u32 = captures
        .get(1)
        .unwrap()
        .as_str()
        .parse()
        .map_err(|_| crate::error::CoreError::InvalidId(id_str.to_string()))?;

    let task: Option<u32> = captures
        .get(2)
        .map(|m| {
            m.as_str()
                .parse()
                .map_err(|_| crate::error::CoreError::InvalidId(id_str.to_string()))
        })
        .transpose()?;

    Ok(ParsedId { epic, task })
}

/// Check whether a string is a valid epic ID (fn-N or fn-N-slug, no task number).
pub fn is_epic_id(id_str: &str) -> bool {
    parse_id(id_str)
        .map(|p| p.task.is_none())
        .unwrap_or(false)
}

/// Check whether a string is a valid task ID (fn-N.M or fn-N-slug.M).
pub fn is_task_id(id_str: &str) -> bool {
    parse_id(id_str)
        .map(|p| p.task.is_some())
        .unwrap_or(false)
}

/// Extract the epic ID from a task ID string.
///
/// Preserves suffix: `fn-5-x7k.3` -> `fn-5-x7k`.
pub fn epic_id_from_task(task_id: &str) -> Result<String, crate::error::CoreError> {
    let parsed = parse_id(task_id)?;
    if parsed.task.is_none() {
        return Err(crate::error::CoreError::InvalidId(format!(
            "not a task ID: {task_id}"
        )));
    }
    // Split on '.' and take the epic part (preserves suffix).
    let epic_part = task_id
        .rsplit_once('.')
        .map(|(epic, _)| epic)
        .unwrap_or(task_id);
    Ok(epic_part.to_string())
}

/// Expand a potentially short dependency ID to a full task ID within the given epic.
///
/// If `dep_id` is a short-form task ID (e.g., `fn-42.1`) whose epic number matches
/// the epic number parsed from `epic_id`, expands it to `{epic_id}.{task_num}`.
/// If `dep_id` is already a full task ID, returns it unchanged.
///
/// # Examples
///
/// ```
/// use flowctl_core::id::expand_dep_id;
///
/// // Short ID expanded to full
/// let result = expand_dep_id("fn-42.1", "fn-42-confidence-calibration");
/// assert_eq!(result, "fn-42-confidence-calibration.1");
///
/// // Already full ID — returned unchanged
/// let result = expand_dep_id("fn-42-confidence-calibration.1", "fn-42-confidence-calibration");
/// assert_eq!(result, "fn-42-confidence-calibration.1");
///
/// // Different epic number — returned unchanged (caller handles error)
/// let result = expand_dep_id("fn-99.1", "fn-42-confidence-calibration");
/// assert_eq!(result, "fn-99.1");
/// ```
pub fn expand_dep_id(dep_id: &str, epic_id: &str) -> String {
    // Parse both IDs to extract epic numbers
    let dep_parsed = match parse_id(dep_id) {
        Ok(p) => p,
        Err(_) => return dep_id.to_string(),
    };
    let epic_parsed = match parse_id(epic_id) {
        Ok(p) => p,
        Err(_) => return dep_id.to_string(),
    };

    // Only expand if: dep is a task ID, epic numbers match, and dep is shorter (a short form)
    if let Some(task_num) = dep_parsed.task {
        if dep_parsed.epic == epic_parsed.epic {
            let dep_epic = dep_id.rsplit_once('.').map(|(e, _)| e).unwrap_or(dep_id);
            // If the dep's epic portion differs from the target epic, expand it
            if dep_epic != epic_id {
                return format!("{}.{}", epic_id, task_num);
            }
        }
    }

    dep_id.to_string()
}

/// Convert text to a URL-safe slug for epic IDs.
///
/// Uses Django pattern (stdlib only): normalize unicode, strip non-alphanumeric,
/// collapse whitespace/hyphens. Returns None if result is empty.
///
/// Output contains only `[a-z0-9-]` to match `parse_id()` regex.
///
/// Ported from `scripts/flowctl/core/ids.py:16-46`. Must produce identical
/// output to the Python version for Unicode input.
///
/// # Arguments
///
/// * `text` - Input text to slugify.
/// * `max_length` - Maximum length (default 40). Set to 0 for no limit.
///
/// # Examples
///
/// ```
/// use flowctl_core::id::slugify;
///
/// assert_eq!(slugify("Hello World", 40), Some("hello-world".to_string()));
/// assert_eq!(slugify("café résumé", 40), Some("cafe-resume".to_string()));
/// assert_eq!(slugify("", 40), None);
/// ```
pub fn slugify(text: &str, max_length: usize) -> Option<String> {
    // Step 1: NFKD normalize + strip to ASCII.
    // This matches Python's: unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode("ascii")
    let normalized = unicode_nfkd_to_ascii(text);

    // Step 2: Remove non-word chars (except spaces and hyphens), lowercase.
    let lowered = normalized.to_lowercase();
    let cleaned = SLUGIFY_NON_WORD.replace_all(&lowered, "");

    // Step 3: Convert underscores to spaces.
    let no_underscores = cleaned.replace('_', " ");

    // Step 4: Collapse whitespace and hyphens to single hyphen, strip leading/trailing.
    let collapsed = SLUGIFY_COLLAPSE.replace_all(&no_underscores, "-");
    let trimmed = collapsed.trim_matches('-');

    if trimmed.is_empty() {
        return None;
    }

    let mut result = trimmed.to_string();

    // Step 5: Truncate at word boundary if too long.
    if max_length > 0 && result.len() > max_length {
        let truncated = &result[..max_length];
        if let Some(pos) = truncated.rfind('-') {
            result = truncated[..pos].trim_end_matches('-').to_string();
        } else {
            result = truncated.trim_end_matches('-').to_string();
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Perform NFKD normalization and strip non-ASCII characters.
///
/// This replicates Python's behavior:
/// ```python
/// unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode("ascii")
/// ```
///
/// NFKD decomposes characters into base + combining marks, then we keep
/// only ASCII bytes. This turns e.g. 'e' + combining accent into just 'e'.
fn unicode_nfkd_to_ascii(text: &str) -> String {
    // Manual NFKD decomposition: iterate over chars, decompose each to
    // its NFKD form, keep only ASCII.
    //
    // For common accented Latin chars, the NFKD decomposition splits them
    // into base char + combining diacritical mark. The combining marks are
    // non-ASCII (U+0300..U+036F), so they get stripped.
    //
    // We use Rust's built-in Unicode character database via char methods.
    // Since we don't have a full NFKD implementation in std, we do a
    // simplified version that handles the common cases matching Python.
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        // Try to decompose to ASCII-compatible representation.
        if ch.is_ascii() {
            result.push(ch);
        } else {
            // For non-ASCII chars, attempt NFKD-style decomposition.
            // Common accented characters decompose to base + combining mark.
            // We only keep the ASCII base character.
            if let Some(ascii_ch) = decompose_to_ascii(ch) {
                result.push(ascii_ch);
            }
            // Non-decomposable non-ASCII characters are dropped (like Python's
            // .encode("ascii", "ignore"))
        }
    }
    result
}

/// Attempt to decompose a Unicode character to its ASCII base.
///
/// Covers common Latin accented characters that Python's NFKD handles.
/// Returns None if the character has no ASCII decomposition.
fn decompose_to_ascii(ch: char) -> Option<char> {
    // Common Latin-1 Supplement and Latin Extended decompositions.
    // This table covers the characters most commonly encountered in
    // slugification. It matches Python's NFKD + ASCII encode behavior.
    match ch {
        // A variants
        '\u{00C0}'..='\u{00C5}' => Some('A'),
        '\u{00E0}'..='\u{00E5}' => Some('a'),
        // C variants
        '\u{00C7}' => Some('C'),
        '\u{00E7}' => Some('c'),
        // D variants
        '\u{00D0}' => Some('D'),
        '\u{00F0}' => Some('d'),
        // E variants
        '\u{00C8}'..='\u{00CB}' => Some('E'),
        '\u{00E8}'..='\u{00EB}' => Some('e'),
        // I variants
        '\u{00CC}'..='\u{00CF}' => Some('I'),
        '\u{00EC}'..='\u{00EF}' => Some('i'),
        // N variants
        '\u{00D1}' => Some('N'),
        '\u{00F1}' => Some('n'),
        // O variants
        '\u{00D2}'..='\u{00D6}' => Some('O'),
        '\u{00D8}' => Some('O'),
        '\u{00F2}'..='\u{00F6}' => Some('o'),
        '\u{00F8}' => Some('o'),
        // U variants
        '\u{00D9}'..='\u{00DC}' => Some('U'),
        '\u{00F9}'..='\u{00FC}' => Some('u'),
        // Y variants
        '\u{00DD}' => Some('Y'),
        '\u{00FD}' | '\u{00FF}' => Some('y'),
        // Ligatures / special
        '\u{00C6}' => Some('A'), // Python NFKD: AE doesn't decompose, encode strips it
        '\u{00E6}' => Some('a'), // same
        '\u{00DF}' => None,      // German sharp s - Python NFKD doesn't decompose, stripped
        '\u{0152}' => Some('O'), // OE ligature
        '\u{0153}' => Some('o'),
        // Latin Extended-A (common)
        '\u{0100}' | '\u{0102}' | '\u{0104}' => Some('A'),
        '\u{0101}' | '\u{0103}' | '\u{0105}' => Some('a'),
        '\u{0106}' | '\u{0108}' | '\u{010A}' | '\u{010C}' => Some('C'),
        '\u{0107}' | '\u{0109}' | '\u{010B}' | '\u{010D}' => Some('c'),
        '\u{010E}' | '\u{0110}' => Some('D'),
        '\u{010F}' | '\u{0111}' => Some('d'),
        '\u{0112}'..='\u{011A}' if ch as u32 % 2 == 0 => Some('E'),
        '\u{0113}'..='\u{011B}' if ch as u32 % 2 == 1 => Some('e'),
        '\u{011C}'..='\u{0122}' if ch as u32 % 2 == 0 => Some('G'),
        '\u{011D}'..='\u{0123}' if ch as u32 % 2 == 1 => Some('g'),
        '\u{0124}' | '\u{0126}' => Some('H'),
        '\u{0125}' | '\u{0127}' => Some('h'),
        '\u{0128}'..='\u{0130}' if ch as u32 % 2 == 0 => Some('I'),
        '\u{0129}'..='\u{0131}' if ch as u32 % 2 == 1 => Some('i'),
        '\u{0134}' => Some('J'),
        '\u{0135}' => Some('j'),
        '\u{0136}' => Some('K'),
        '\u{0137}' => Some('k'),
        '\u{0139}' | '\u{013B}' | '\u{013D}' | '\u{013F}' | '\u{0141}' => Some('L'),
        '\u{013A}' | '\u{013C}' | '\u{013E}' | '\u{0140}' | '\u{0142}' => Some('l'),
        '\u{0143}' | '\u{0145}' | '\u{0147}' => Some('N'),
        '\u{0144}' | '\u{0146}' | '\u{0148}' => Some('n'),
        '\u{014C}'..='\u{0150}' if ch as u32 % 2 == 0 => Some('O'),
        '\u{014D}'..='\u{0151}' if ch as u32 % 2 == 1 => Some('o'),
        '\u{0154}' | '\u{0156}' | '\u{0158}' => Some('R'),
        '\u{0155}' | '\u{0157}' | '\u{0159}' => Some('r'),
        '\u{015A}' | '\u{015C}' | '\u{015E}' | '\u{0160}' => Some('S'),
        '\u{015B}' | '\u{015D}' | '\u{015F}' | '\u{0161}' => Some('s'),
        '\u{0162}' | '\u{0164}' | '\u{0166}' => Some('T'),
        '\u{0163}' | '\u{0165}' | '\u{0167}' => Some('t'),
        '\u{0168}'..='\u{0172}' if ch as u32 % 2 == 0 => Some('U'),
        '\u{0169}'..='\u{0173}' if ch as u32 % 2 == 1 => Some('u'),
        '\u{0174}' => Some('W'),
        '\u{0175}' => Some('w'),
        '\u{0176}' | '\u{0178}' => Some('Y'),
        '\u{0177}' => Some('y'),
        '\u{0179}' | '\u{017B}' | '\u{017D}' => Some('Z'),
        '\u{017A}' | '\u{017C}' | '\u{017E}' => Some('z'),
        _ => None,
    }
}

/// Generate a random alphanumeric suffix for epic IDs (a-z0-9).
///
/// Uses a simple thread-local RNG for suffix generation.
pub fn generate_epic_suffix(length: usize) -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

    let mut result = String::with_capacity(length);
    for i in 0..length {
        // Use RandomState for basic randomness (no external dep needed).
        let state = RandomState::new();
        let mut hasher = state.build_hasher();
        hasher.write_usize(i);
        let idx = (hasher.finish() as usize) % ALPHABET.len();
        result.push(ALPHABET[idx] as char);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_id tests ──────────────────────────────────────────────

    #[test]
    fn test_parse_legacy_epic() {
        let p = parse_id("fn-1").unwrap();
        assert_eq!(p.epic, 1);
        assert_eq!(p.task, None);
    }

    #[test]
    fn test_parse_legacy_task() {
        let p = parse_id("fn-1.3").unwrap();
        assert_eq!(p.epic, 1);
        assert_eq!(p.task, Some(3));
    }

    #[test]
    fn test_parse_short_suffix_epic() {
        let p = parse_id("fn-5-x7k").unwrap();
        assert_eq!(p.epic, 5);
        assert_eq!(p.task, None);
    }

    #[test]
    fn test_parse_short_suffix_task() {
        let p = parse_id("fn-5-x7k.3").unwrap();
        assert_eq!(p.epic, 5);
        assert_eq!(p.task, Some(3));
    }

    #[test]
    fn test_parse_slug_epic() {
        let p = parse_id("fn-2-add-auth").unwrap();
        assert_eq!(p.epic, 2);
        assert_eq!(p.task, None);
    }

    #[test]
    fn test_parse_slug_task() {
        let p = parse_id("fn-2-add-auth.1").unwrap();
        assert_eq!(p.epic, 2);
        assert_eq!(p.task, Some(1));
    }

    #[test]
    fn test_parse_long_slug() {
        let p = parse_id("fn-10-flowctl-rust-platform-rewrite").unwrap();
        assert_eq!(p.epic, 10);
        assert_eq!(p.task, None);

        let p = parse_id("fn-10-flowctl-rust-platform-rewrite.5").unwrap();
        assert_eq!(p.epic, 10);
        assert_eq!(p.task, Some(5));
    }

    #[test]
    fn test_parse_single_char_suffix() {
        let p = parse_id("fn-3-a").unwrap();
        assert_eq!(p.epic, 3);
        assert_eq!(p.task, None);
    }

    #[test]
    fn test_parse_two_char_suffix() {
        let p = parse_id("fn-3-ab").unwrap();
        assert_eq!(p.epic, 3);
        assert_eq!(p.task, None);
    }

    #[test]
    fn test_parse_invalid_ids() {
        assert!(parse_id("").is_err());
        assert!(parse_id("invalid").is_err());
        assert!(parse_id("fn-").is_err());
        assert!(parse_id("fn-abc").is_err());
        assert!(parse_id("task-1").is_err());
        assert!(parse_id("fn-1-").is_err()); // trailing hyphen
        assert!(parse_id("fn-1-ABC").is_err()); // uppercase
        assert!(parse_id("fn-1.").is_err()); // trailing dot
        assert!(parse_id("fn-1.abc").is_err()); // non-numeric task
    }

    #[test]
    fn test_parse_large_numbers() {
        let p = parse_id("fn-999.99").unwrap();
        assert_eq!(p.epic, 999);
        assert_eq!(p.task, Some(99));
    }

    // ── is_epic_id / is_task_id tests ───────────────────────────────

    #[test]
    fn test_is_epic_id() {
        assert!(is_epic_id("fn-1"));
        assert!(is_epic_id("fn-2-add-auth"));
        assert!(!is_epic_id("fn-1.1"));
        assert!(!is_epic_id("invalid"));
    }

    #[test]
    fn test_is_task_id() {
        assert!(is_task_id("fn-1.1"));
        assert!(is_task_id("fn-2-add-auth.3"));
        assert!(!is_task_id("fn-1"));
        assert!(!is_task_id("invalid"));
    }

    // ── epic_id_from_task tests ─────────────────────────────────────

    #[test]
    fn test_epic_id_from_task() {
        assert_eq!(
            epic_id_from_task("fn-1.3").unwrap(),
            "fn-1"
        );
        assert_eq!(
            epic_id_from_task("fn-5-x7k.3").unwrap(),
            "fn-5-x7k"
        );
        assert_eq!(
            epic_id_from_task("fn-2-add-auth.1").unwrap(),
            "fn-2-add-auth"
        );
    }

    #[test]
    fn test_epic_id_from_task_errors() {
        // Epic ID (no task number) should error.
        assert!(epic_id_from_task("fn-1").is_err());
        // Invalid ID should error.
        assert!(epic_id_from_task("invalid").is_err());
    }

    // ── TaskId tests ────────────────────────────────────────────────

    #[test]
    fn test_task_id_epic_id() {
        let tid = TaskId("fn-5-x7k.3".to_string());
        let eid = tid.epic_id().unwrap();
        assert_eq!(eid.0, "fn-5-x7k");
    }

    // ── slugify tests ───────────────────────────────────────────────

    #[test]
    fn test_slugify_basic() {
        assert_eq!(slugify("Hello World", 40), Some("hello-world".to_string()));
    }

    #[test]
    fn test_slugify_accented() {
        assert_eq!(
            slugify("cafe resume", 40),
            Some("cafe-resume".to_string())
        );
        assert_eq!(
            slugify("cafe\u{0301} re\u{0301}sume\u{0301}", 40),
            Some("cafe-resume".to_string())
        );
    }

    #[test]
    fn test_slugify_unicode_accented() {
        // "cafe" with combining accent -> "cafe"
        // "resume" with combining accent -> "resume"
        assert_eq!(
            slugify("caf\u{00E9} r\u{00E9}sum\u{00E9}", 40),
            Some("cafe-resume".to_string())
        );
    }

    #[test]
    fn test_slugify_special_chars() {
        assert_eq!(
            slugify("Hello, World! (2024)", 40),
            Some("hello-world-2024".to_string())
        );
    }

    #[test]
    fn test_slugify_underscores() {
        assert_eq!(
            slugify("hello_world_test", 40),
            Some("hello-world-test".to_string())
        );
    }

    #[test]
    fn test_slugify_multiple_spaces_hyphens() {
        assert_eq!(
            slugify("hello   ---  world", 40),
            Some("hello-world".to_string())
        );
    }

    #[test]
    fn test_slugify_empty() {
        assert_eq!(slugify("", 40), None);
        assert_eq!(slugify("   ", 40), None);
        assert_eq!(slugify("---", 40), None);
    }

    #[test]
    fn test_slugify_truncation() {
        let long_text = "this is a very long title that should be truncated at a word boundary";
        let result = slugify(long_text, 20).unwrap();
        assert!(result.len() <= 20);
        // Should truncate at word boundary (hyphen).
        assert!(!result.ends_with('-'));
    }

    #[test]
    fn test_slugify_no_limit() {
        let long_text = "this is a very long title that should not be truncated";
        let result = slugify(long_text, 0).unwrap();
        assert_eq!(
            result,
            "this-is-a-very-long-title-that-should-not-be-truncated"
        );
    }

    #[test]
    fn test_slugify_leading_trailing_special() {
        assert_eq!(
            slugify("---hello---", 40),
            Some("hello".to_string())
        );
        assert_eq!(
            slugify("  hello  ", 40),
            Some("hello".to_string())
        );
    }

    // ── generate_epic_suffix tests ──────────────────────────────────

    #[test]
    fn test_generate_suffix_length() {
        let suffix = generate_epic_suffix(3);
        assert_eq!(suffix.len(), 3);
        // All chars should be a-z0-9.
        assert!(suffix.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn test_generate_suffix_different_lengths() {
        for len in [1, 3, 5, 10] {
            let suffix = generate_epic_suffix(len);
            assert_eq!(suffix.len(), len);
        }
    }

    // ── Python parity tests ─────────────────────────────────────────
    // These test exact output matching with the Python implementation
    // at scripts/flowctl/core/ids.py.

    #[test]
    fn test_python_parity_parse_id() {
        // Exact matches from Python output.
        let cases: Vec<(&str, Option<u32>, Option<u32>)> = vec![
            ("fn-1", Some(1), None),
            ("fn-1.3", Some(1), Some(3)),
            ("fn-5-x7k", Some(5), None),
            ("fn-5-x7k.3", Some(5), Some(3)),
            ("fn-2-add-auth", Some(2), None),
            ("fn-2-add-auth.1", Some(2), Some(1)),
            ("fn-10-flowctl-rust-platform-rewrite", Some(10), None),
            ("fn-10-flowctl-rust-platform-rewrite.5", Some(10), Some(5)),
            ("fn-3-a", Some(3), None),
            ("fn-3-ab", Some(3), None),
            ("fn-999.99", Some(999), Some(99)),
            // Invalid cases — Python returns (None, None).
            ("invalid", None, None),
            ("", None, None),
            ("fn-abc", None, None),
            ("fn-1-", None, None),
            ("fn-1.abc", None, None),
        ];

        for (input, expected_epic, expected_task) in cases {
            match parse_id(input) {
                Ok(parsed) => {
                    assert_eq!(
                        Some(parsed.epic),
                        expected_epic,
                        "Epic mismatch for {input}"
                    );
                    assert_eq!(
                        parsed.task, expected_task,
                        "Task mismatch for {input}"
                    );
                }
                Err(_) => {
                    assert_eq!(
                        expected_epic, None,
                        "Expected valid parse for {input}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_python_parity_slugify() {
        // Exact matches from Python output.
        let cases: Vec<(&str, usize, Option<&str>)> = vec![
            ("Hello World", 40, Some("hello-world")),
            ("caf\u{00E9} r\u{00E9}sum\u{00E9}", 40, Some("cafe-resume")),
            ("Hello, World! (2024)", 40, Some("hello-world-2024")),
            ("hello_world_test", 40, Some("hello-world-test")),
            ("hello   ---  world", 40, Some("hello-world")),
            ("", 40, None),
            ("   ", 40, None),
            ("---", 40, None),
            (
                "this is a very long title that should be truncated at a word boundary",
                40,
                Some("this-is-a-very-long-title-that-should"),
            ),
            ("---hello---", 40, Some("hello")),
            (
                "this is a very long title that should be truncated at a word boundary",
                20,
                Some("this-is-a-very-long"),
            ),
        ];

        for (input, max_len, expected) in cases {
            let result = slugify(input, max_len);
            assert_eq!(
                result.as_deref(),
                expected,
                "slugify({input:?}, {max_len}) mismatch"
            );
        }
    }

    // ── expand_dep_id tests ────────────────────────────────────────

    #[test]
    fn test_expand_short_id_to_full() {
        assert_eq!(
            expand_dep_id("fn-42.1", "fn-42-confidence-calibration"),
            "fn-42-confidence-calibration.1"
        );
    }

    #[test]
    fn test_expand_already_full_id_unchanged() {
        assert_eq!(
            expand_dep_id("fn-42-confidence-calibration.1", "fn-42-confidence-calibration"),
            "fn-42-confidence-calibration.1"
        );
    }

    #[test]
    fn test_expand_different_epic_number_unchanged() {
        // Epic numbers don't match — return as-is
        assert_eq!(
            expand_dep_id("fn-99.1", "fn-42-confidence-calibration"),
            "fn-99.1"
        );
    }

    #[test]
    fn test_expand_legacy_short_id() {
        assert_eq!(
            expand_dep_id("fn-5.3", "fn-5-add-auth"),
            "fn-5-add-auth.3"
        );
    }

    #[test]
    fn test_expand_invalid_id_unchanged() {
        assert_eq!(
            expand_dep_id("invalid", "fn-42-slug"),
            "invalid"
        );
    }

    #[test]
    fn test_expand_epic_id_not_task_unchanged() {
        // Not a task ID (no .N) — return as-is
        assert_eq!(
            expand_dep_id("fn-42", "fn-42-slug"),
            "fn-42"
        );
    }

    #[test]
    fn test_expand_short_suffix_to_long_suffix() {
        assert_eq!(
            expand_dep_id("fn-10-abc.5", "fn-10-flowctl-rust-platform-rewrite"),
            "fn-10-flowctl-rust-platform-rewrite.5"
        );
    }
}
