//! Shared helpers for epic commands.

use std::fs;
use std::path::{Path, PathBuf};

use crate::output::error_exit;

use flowctl_core::id::is_epic_id;
use flowctl_core::types::{Document, Epic, META_FILE};

use super::super::helpers::get_flow_dir;

/// Ensure .flow/ exists, error_exit if not.
pub fn ensure_flow_exists() -> PathBuf {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
    flow_dir
}

/// Validate an epic ID, error_exit if invalid.
pub fn validate_epic_id(id: &str) {
    if !is_epic_id(id) {
        error_exit(&format!(
            "Invalid epic ID: {id}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)"
        ));
    }
}

/// Load epic document from JSON files.
pub fn load_epic(id: &str) -> Document<Epic> {
    let flow_dir = get_flow_dir();
    let epic = flowctl_core::json_store::epic_read(&flow_dir, id)
        .unwrap_or_else(|_| error_exit(&format!("Epic {id} not found")));
    let body = flowctl_core::json_store::epic_spec_read(&flow_dir, id).unwrap_or_default();
    Document {
        frontmatter: epic,
        body,
    }
}

/// Write an epic document to JSON files.
pub fn save_epic(doc: &Document<Epic>) {
    let flow_dir = get_flow_dir();
    if let Err(e) = flowctl_core::json_store::epic_write(&flow_dir, &doc.frontmatter) {
        error_exit(&format!("Failed to write epic {}: {e}", doc.frontmatter.id));
    }
    if let Err(e) =
        flowctl_core::json_store::epic_spec_write(&flow_dir, &doc.frontmatter.id, &doc.body)
    {
        error_exit(&format!(
            "Failed to write epic spec {}: {e}",
            doc.frontmatter.id
        ));
    }
}

/// Get max epic number from JSON files.
pub fn find_max_epic_number() -> u32 {
    let flow_dir = get_flow_dir();
    flowctl_core::json_store::epic_max_num(&flow_dir).unwrap_or(0)
}

/// Create default epic spec Markdown body.
pub fn create_epic_spec_body(id: &str, title: &str) -> String {
    format!(
        "# {id} {title}\n\n\
         ## Overview\nTBD\n\n\
         ## Scope\nTBD\n\n\
         ## Approach\nTBD\n\n\
         ## Quick commands\n\
         <!-- Required: at least one smoke command for the repo -->\n\
         - `# e.g., npm test, bun test, make test`\n\n\
         ## Acceptance\n\
         - [ ] TBD\n\n\
         ## References\n\
         - TBD\n"
    )
}

/// Read content from file path or stdin (if path is "-").
pub fn read_file_or_stdin(file: &str) -> String {
    if file == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .unwrap_or_else(|e| error_exit(&format!("Failed to read stdin: {e}")));
        buf
    } else {
        let path = Path::new(file);
        if !path.exists() {
            error_exit(&format!("Input file not found: {file}"));
        }
        fs::read_to_string(path)
            .unwrap_or_else(|e| error_exit(&format!("Failed to read {file}: {e}")))
    }
}

/// Gap-blocking priorities (matches Python's GAP_BLOCKING_PRIORITIES).
pub const GAP_BLOCKING_PRIORITIES: &[&str] = &["required", "important"];

/// Returns true if a review filename belongs to the given epic.
pub fn review_belongs_to_epic(name: &str, id: &str) -> bool {
    name.contains(&format!("-{id}.")) || name.starts_with(&format!("epic-audit-{id}-"))
}

/// Ensure meta.json exists in the flow directory.
pub fn ensure_meta_exists(flow_dir: &Path) {
    let meta_path = flow_dir.join(META_FILE);
    if !meta_path.exists() {
        error_exit("meta.json not found. Run 'flowctl init' first.");
    }
}

/// Load branch name for an epic from DB (sole source of truth).
pub fn load_epic_branch(epic_id: &str) -> Option<String> {
    let flow_dir = get_flow_dir();
    let epic = flowctl_core::json_store::epic_read(&flow_dir, epic_id).ok()?;
    epic.branch_name.filter(|b| !b.is_empty())
}
