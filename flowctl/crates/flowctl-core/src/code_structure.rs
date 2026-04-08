//! Code structure extraction: extract symbol definitions from source files.
//!
//! Uses regex-based extraction for function/struct/class/trait/type/enum/const
//! definitions across multiple languages. Designed to be upgraded to tree-sitter
//! parsing in the future without changing the public API.

#![forbid(unsafe_code)]

use std::fmt;
use std::path::Path;

use ignore::WalkBuilder;
use regex::Regex;
use serde::Serialize;
use thiserror::Error;

// ── Error type ──────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum StructureError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("unsupported language for file: {0}")]
    UnsupportedLanguage(String),

    #[error("regex error: {0}")]
    Regex(#[from] regex::Error),
}

// ── Types ───────────────────────────────────────────────────────────

/// Kind of extracted code symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Struct,
    Trait,
    Type,
    Const,
    Impl,
    Class,
    Method,
    Interface,
    Enum,
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Function => "fn",
            Self::Struct => "struct",
            Self::Trait => "trait",
            Self::Type => "type",
            Self::Const => "const",
            Self::Impl => "impl",
            Self::Class => "class",
            Self::Method => "method",
            Self::Interface => "interface",
            Self::Enum => "enum",
        };
        write!(f, "{s}")
    }
}

/// A code symbol extracted from a source file.
#[derive(Debug, Clone, Serialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file: String,
    pub line: usize,
    pub signature: String,
}

// ── Language detection ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    C,
    Cpp,
    Ruby,
}

fn detect_language(path: &Path) -> Option<Language> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "rs" => Some(Language::Rust),
        "py" => Some(Language::Python),
        "js" | "mjs" | "cjs" | "jsx" => Some(Language::JavaScript),
        "ts" | "tsx" | "mts" | "cts" => Some(Language::TypeScript),
        "go" => Some(Language::Go),
        "java" => Some(Language::Java),
        "c" | "h" => Some(Language::C),
        "cpp" | "hpp" | "cc" | "cxx" | "hh" | "hxx" => Some(Language::Cpp),
        "rb" => Some(Language::Ruby),
        _ => None,
    }
}

// ── Regex patterns per language ─────────────────────────────────────

struct LangPatterns {
    patterns: Vec<(SymbolKind, Regex)>,
}

fn rust_patterns() -> LangPatterns {
    LangPatterns {
        patterns: vec![
            (SymbolKind::Function, Regex::new(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)\s*(?:<[^>]*>)?\s*\([^)]*\)(?:\s*->\s*[^\{;]+)?").unwrap()),
            (SymbolKind::Struct, Regex::new(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+(\w+)(?:<[^>]*>)?(?:\s*\{|\s*\(|\s*;)").unwrap()),
            (SymbolKind::Enum, Regex::new(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?enum\s+(\w+)(?:<[^>]*>)?").unwrap()),
            (SymbolKind::Trait, Regex::new(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?trait\s+(\w+)(?:<[^>]*>)?").unwrap()),
            (SymbolKind::Type, Regex::new(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?type\s+(\w+)(?:<[^>]*>)?\s*=").unwrap()),
            (SymbolKind::Const, Regex::new(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?const\s+(\w+)\s*:").unwrap()),
            (SymbolKind::Impl, Regex::new(r"(?m)^impl(?:<[^>]*>)?\s+(?:(\w+)\s+for\s+)?(\w+)").unwrap()),
        ],
    }
}

fn python_patterns() -> LangPatterns {
    LangPatterns {
        patterns: vec![
            (SymbolKind::Function, Regex::new(r"(?m)^(?:\s*)(?:async\s+)?def\s+(\w+)\s*\([^)]*\)(?:\s*->\s*[^:]+)?").unwrap()),
            (SymbolKind::Class, Regex::new(r"(?m)^class\s+(\w+)(?:\([^)]*\))?").unwrap()),
        ],
    }
}

fn js_ts_patterns(is_typescript: bool) -> LangPatterns {
    let mut patterns = vec![
        (SymbolKind::Function, Regex::new(r"(?m)^(?:export\s+)?(?:async\s+)?function\s+(\w+)\s*(?:<[^>]*>)?\s*\([^)]*\)").unwrap()),
        (SymbolKind::Class, Regex::new(r"(?m)^(?:export\s+)?class\s+(\w+)").unwrap()),
        (SymbolKind::Const, Regex::new(r"(?m)^(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s+)?\([^)]*\)\s*=>").unwrap()),
    ];
    if is_typescript {
        patterns.push((SymbolKind::Interface, Regex::new(r"(?m)^(?:export\s+)?interface\s+(\w+)(?:<[^>]*>)?").unwrap()));
        patterns.push((SymbolKind::Type, Regex::new(r"(?m)^(?:export\s+)?type\s+(\w+)(?:<[^>]*>)?\s*=").unwrap()));
        patterns.push((SymbolKind::Enum, Regex::new(r"(?m)^(?:export\s+)?enum\s+(\w+)").unwrap()));
    }
    LangPatterns { patterns }
}

fn go_patterns() -> LangPatterns {
    LangPatterns {
        patterns: vec![
            (SymbolKind::Function, Regex::new(r"(?m)^func\s+(?:\([^)]+\)\s+)?(\w+)\s*\([^)]*\)").unwrap()),
            (SymbolKind::Struct, Regex::new(r"(?m)^type\s+(\w+)\s+struct\s*\{").unwrap()),
            (SymbolKind::Interface, Regex::new(r"(?m)^type\s+(\w+)\s+interface\s*\{").unwrap()),
            (SymbolKind::Type, Regex::new(r"(?m)^type\s+(\w+)\s+(?!struct|interface)\w").unwrap()),
        ],
    }
}

fn java_patterns() -> LangPatterns {
    LangPatterns {
        patterns: vec![
            (SymbolKind::Class, Regex::new(r"(?m)^(?:public|private|protected)?\s*(?:abstract\s+)?(?:final\s+)?class\s+(\w+)").unwrap()),
            (SymbolKind::Interface, Regex::new(r"(?m)^(?:public\s+)?interface\s+(\w+)").unwrap()),
            (SymbolKind::Enum, Regex::new(r"(?m)^(?:public\s+)?enum\s+(\w+)").unwrap()),
            (SymbolKind::Method, Regex::new(r"(?m)^\s+(?:public|private|protected)\s+(?:static\s+)?(?:final\s+)?(?:\w+(?:<[^>]*>)?)\s+(\w+)\s*\(").unwrap()),
        ],
    }
}

fn c_cpp_patterns(is_cpp: bool) -> LangPatterns {
    let mut patterns = vec![
        (SymbolKind::Function, Regex::new(r"(?m)^(?:\w[\w\s\*]*?)\s+(\w+)\s*\([^)]*\)\s*\{").unwrap()),
        (SymbolKind::Struct, Regex::new(r"(?m)^(?:typedef\s+)?struct\s+(\w+)").unwrap()),
        (SymbolKind::Enum, Regex::new(r"(?m)^(?:typedef\s+)?enum\s+(\w+)").unwrap()),
    ];
    if is_cpp {
        patterns.push((SymbolKind::Class, Regex::new(r"(?m)^class\s+(\w+)").unwrap()));
    }
    LangPatterns { patterns }
}

fn ruby_patterns() -> LangPatterns {
    LangPatterns {
        patterns: vec![
            (SymbolKind::Function, Regex::new(r"(?m)^\s*def\s+(?:self\.)?(\w+[?!]?)").unwrap()),
            (SymbolKind::Class, Regex::new(r"(?m)^class\s+(\w+)").unwrap()),
            (SymbolKind::Function, Regex::new(r"(?m)^module\s+(\w+)").unwrap()),
        ],
    }
}

fn patterns_for(lang: Language) -> LangPatterns {
    match lang {
        Language::Rust => rust_patterns(),
        Language::Python => python_patterns(),
        Language::JavaScript => js_ts_patterns(false),
        Language::TypeScript => js_ts_patterns(true),
        Language::Go => go_patterns(),
        Language::Java => java_patterns(),
        Language::C => c_cpp_patterns(false),
        Language::Cpp => c_cpp_patterns(true),
        Language::Ruby => ruby_patterns(),
    }
}

// ── Extraction ──────────────────────────────────────────────────────

/// Extract the line number (1-based) for a byte offset in content.
fn line_for_offset(content: &str, offset: usize) -> usize {
    content[..offset].matches('\n').count() + 1
}

/// Clean up a matched signature: take only the first line, trim trailing `{`.
fn clean_signature(matched: &str) -> String {
    let first_line = matched.lines().next().unwrap_or(matched);
    first_line.trim().trim_end_matches('{').trim().to_string()
}

/// Extract all symbol definitions from a single file.
pub fn extract_symbols(path: &Path) -> Result<Vec<Symbol>, StructureError> {
    let lang = detect_language(path).ok_or_else(|| {
        StructureError::UnsupportedLanguage(path.display().to_string())
    })?;
    let content = std::fs::read_to_string(path)?;
    let file_str = path.display().to_string();
    let lang_patterns = patterns_for(lang);

    let mut symbols = Vec::new();

    for (kind, regex) in &lang_patterns.patterns {
        for cap in regex.captures_iter(&content) {
            let full_match = cap.get(0).unwrap();
            let line = line_for_offset(&content, full_match.start());
            let signature = clean_signature(full_match.as_str());

            // For impl blocks in Rust, extract the type name
            let name = if *kind == SymbolKind::Impl && lang == Language::Rust {
                // Group 2 is the target type, group 1 is the trait (if `impl Trait for Type`)
                cap.get(2)
                    .or_else(|| cap.get(1))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default()
            } else {
                cap.get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default()
            };

            if name.is_empty() {
                continue;
            }

            symbols.push(Symbol {
                name,
                kind: *kind,
                file: file_str.clone(),
                line,
                signature: signature.clone(),
            });
        }
    }

    // Sort by line number for stable output.
    symbols.sort_by_key(|s| s.line);
    Ok(symbols)
}

/// Extract symbols from all supported files under a directory.
pub fn extract_all_symbols(root: &Path) -> Result<Vec<Symbol>, StructureError> {
    let mut all_symbols = Vec::new();

    let walker = WalkBuilder::new(root)
        .hidden(true) // respect hidden dirs
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if detect_language(path).is_none() {
            continue;
        }
        match extract_symbols(path) {
            Ok(syms) => all_symbols.extend(syms),
            Err(StructureError::UnsupportedLanguage(_)) => continue,
            Err(StructureError::Io(_)) => continue, // skip unreadable files
            Err(e) => return Err(e),
        }
    }

    Ok(all_symbols)
}

/// Return the list of supported file extensions.
pub fn supported_extensions() -> &'static [&'static str] {
    &[
        "rs", "py", "js", "mjs", "cjs", "jsx", "ts", "tsx", "mts", "cts",
        "go", "java", "c", "h", "cpp", "hpp", "cc", "cxx", "hh", "hxx", "rb",
    ]
}

/// Check if a path has a supported extension.
pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| supported_extensions().contains(&ext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_rust_extraction() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        let mut f = std::fs::File::create(&file).unwrap();
        write!(f, r#"
pub fn authenticate(token: &str) -> Result<User> {{
    todo!()
}}

pub struct User {{
    pub id: u64,
    pub email: String,
}}

enum Role {{
    Admin,
    User,
}}

pub trait Auth {{
    fn check(&self) -> bool;
}}
"#).unwrap();

        let symbols = extract_symbols(&file).unwrap();
        assert!(symbols.iter().any(|s| s.name == "authenticate" && s.kind == SymbolKind::Function));
        assert!(symbols.iter().any(|s| s.name == "User" && s.kind == SymbolKind::Struct));
        assert!(symbols.iter().any(|s| s.name == "Role" && s.kind == SymbolKind::Enum));
        assert!(symbols.iter().any(|s| s.name == "Auth" && s.kind == SymbolKind::Trait));
    }

    #[test]
    fn test_python_extraction() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.py");
        let mut f = std::fs::File::create(&file).unwrap();
        write!(f, r#"
def greet(name: str) -> str:
    return f"Hello, {{name}}"

class UserService:
    def get_user(self, id: int) -> User:
        pass

async def fetch_data(url: str) -> dict:
    pass
"#).unwrap();

        let symbols = extract_symbols(&file).unwrap();
        assert!(symbols.iter().any(|s| s.name == "greet" && s.kind == SymbolKind::Function));
        assert!(symbols.iter().any(|s| s.name == "UserService" && s.kind == SymbolKind::Class));
        assert!(symbols.iter().any(|s| s.name == "fetch_data" && s.kind == SymbolKind::Function));
    }

    #[test]
    fn test_typescript_extraction() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        let mut f = std::fs::File::create(&file).unwrap();
        write!(f, r#"
export function createUser(name: string): User {{
    return {{ name }};
}}

export interface UserConfig {{
    name: string;
}}

export type UserId = string;

export class UserService {{
}}

export enum Status {{
    Active,
    Inactive,
}}
"#).unwrap();

        let symbols = extract_symbols(&file).unwrap();
        assert!(symbols.iter().any(|s| s.name == "createUser" && s.kind == SymbolKind::Function));
        assert!(symbols.iter().any(|s| s.name == "UserConfig" && s.kind == SymbolKind::Interface));
        assert!(symbols.iter().any(|s| s.name == "UserId" && s.kind == SymbolKind::Type));
        assert!(symbols.iter().any(|s| s.name == "UserService" && s.kind == SymbolKind::Class));
        assert!(symbols.iter().any(|s| s.name == "Status" && s.kind == SymbolKind::Enum));
    }

    #[test]
    fn test_unsupported_extension() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("readme.md");
        std::fs::write(&file, "# Heading").unwrap();
        assert!(matches!(
            extract_symbols(&file),
            Err(StructureError::UnsupportedLanguage(_))
        ));
    }

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language(Path::new("foo.rs")), Some(Language::Rust));
        assert_eq!(detect_language(Path::new("bar.py")), Some(Language::Python));
        assert_eq!(detect_language(Path::new("baz.ts")), Some(Language::TypeScript));
        assert_eq!(detect_language(Path::new("qux.go")), Some(Language::Go));
        assert_eq!(detect_language(Path::new("readme.md")), None);
    }
}
