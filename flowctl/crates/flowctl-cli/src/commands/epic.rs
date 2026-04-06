//! Epic management commands.
//!
//! All CRUD operations go through the DB (sole source of truth).
//! Markdown is only used for spec body content (`specs/*.md`) and reviews.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::id::{generate_epic_suffix, is_epic_id, parse_id, slugify};
use flowctl_core::types::{
    Document, Epic, EpicStatus, ReviewStatus, ARCHIVE_DIR, FLOW_DIR, META_FILE,
    REVIEWS_DIR, SPECS_DIR,
};

#[derive(Subcommand, Debug)]
pub enum EpicCmd {
    /// Create a new epic.
    Create {
        /// Epic title.
        #[arg(long)]
        title: String,
        /// Branch name.
        #[arg(long)]
        branch: Option<String>,
    },
    /// Set epic spec from file (use '-' for stdin).
    Plan {
        /// Epic ID.
        id: String,
        /// Markdown file (use '-' for stdin).
        #[arg(long)]
        file: String,
    },
    /// Set plan review status.
    Review {
        /// Epic ID.
        id: String,
        /// Review status: ship, needs_work, unknown.
        #[arg(value_parser = ["ship", "needs_work", "unknown"])]
        status: String,
    },
    /// Set completion review status.
    Completion {
        /// Epic ID.
        id: String,
        /// Review status: ship, needs_work, unknown.
        #[arg(value_parser = ["ship", "needs_work", "unknown"])]
        status: String,
    },
    /// Set epic branch name.
    Branch {
        /// Epic ID.
        id: String,
        /// Branch name.
        name: String,
    },
    /// Rename epic title.
    Title {
        /// Epic ID.
        id: String,
        /// New title.
        #[arg(long)]
        title: String,
    },
    /// Close an epic.
    Close {
        /// Epic ID.
        id: String,
        /// Bypass gap registry gate.
        #[arg(long)]
        skip_gap_check: bool,
    },
    /// Reopen a closed epic.
    Reopen {
        /// Epic ID.
        id: String,
    },
    /// Archive closed epic to .flow/.archive/.
    Archive {
        /// Epic ID.
        id: String,
        /// Archive even if not closed.
        #[arg(long)]
        force: bool,
    },
    /// Archive all closed epics at once.
    Clean,
    /// Audit epic task-coverage vs original spec (advisory only).
    ///
    /// Assembles the epic spec, task list, and prior audit context into a
    /// payload consumed by `agents/epic-auditor.md`. Writes the assembled
    /// payload to `.flow/reviews/epic-audit-<id>-<timestamp>.json`. Advisory
    /// only — never mutates epic/tasks/gaps.
    Audit {
        /// Epic ID.
        id: String,
        /// Force a new audit even if a recent (<24h) receipt exists.
        #[arg(long)]
        force: bool,
    },
    /// Add epic-level dependency.
    AddDep {
        /// Epic ID.
        epic: String,
        /// Epic ID to depend on.
        depends_on: String,
    },
    /// Remove epic-level dependency.
    RmDep {
        /// Epic ID.
        epic: String,
        /// Epic ID to remove from deps.
        depends_on: String,
    },
    /// Set default backend specs.
    SetBackend {
        /// Epic ID.
        id: String,
        /// Default impl backend spec.
        #[arg(long = "impl")]
        impl_spec: Option<String>,
        /// Default review backend spec.
        #[arg(long)]
        review: Option<String>,
        /// Default sync backend spec.
        #[arg(long)]
        sync: Option<String>,
    },
    /// Set or clear auto_execute_pending marker.
    AutoExec {
        /// Epic ID.
        id: String,
        /// Mark auto-execute as pending.
        #[arg(long)]
        pending: bool,
        /// Clear auto-execute pending marker.
        #[arg(long)]
        done: bool,
    },
}

use super::helpers::get_flow_dir;

// ── Helpers ─────────────────────────────────────────────────────────

/// Ensure .flow/ exists, error_exit if not.
fn ensure_flow_exists() -> PathBuf {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
    flow_dir
}

/// Validate an epic ID, error_exit if invalid.
fn validate_epic_id(id: &str) {
    if !is_epic_id(id) {
        error_exit(&format!(
            "Invalid epic ID: {id}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)"
        ));
    }
}

/// Load epic document from DB (sole source of truth).
fn load_epic(id: &str) -> Document<Epic> {
    let conn = require_db()
        .unwrap_or_else(|e| error_exit(&format!("DB required: {e}")));
    let repo = crate::commands::db_shim::EpicRepo::new(&conn);
    match repo.get_with_body(id) {
        Ok((epic, body)) => Document { frontmatter: epic, body },
        Err(_) => error_exit(&format!("Epic {id} not found")),
    }
}

/// Write an epic document to DB (sole source of truth, no MD export).
fn save_epic(doc: &Document<Epic>) {
    let conn = require_db()
        .unwrap_or_else(|e| error_exit(&format!("DB required: {e}")));
    let repo = crate::commands::db_shim::EpicRepo::new(&conn);
    if let Err(e) = repo.upsert_with_body(&doc.frontmatter, &doc.body) {
        error_exit(&format!("DB write failed for {}: {e}", doc.frontmatter.id));
    }
}

/// Open DB connection (hard error if unavailable).
fn require_db() -> Result<crate::commands::db_shim::Connection, crate::commands::db_shim::DbError> {
    crate::commands::db_shim::require_db()
}

/// Get max epic number from DB. Returns 0 if none exist.
fn find_max_epic_number() -> u32 {
    let conn = require_db()
        .unwrap_or_else(|e| error_exit(&format!("DB required: {e}")));
    match crate::commands::db_shim::max_epic_num(&conn) {
        Ok(n) => n as u32,
        Err(_) => 0,
    }
}

/// Create default epic spec Markdown body.
fn create_epic_spec_body(id: &str, title: &str) -> String {
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
fn read_file_or_stdin(file: &str) -> String {
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
const GAP_BLOCKING_PRIORITIES: &[&str] = &["required", "important"];

// load_epic_raw() removed — gap checks now use DB GapRepo

// ── Command implementations ─────────────────────────────────────────

fn cmd_create(title: &str, branch: &Option<String>, json_mode: bool) {
    let flow_dir = ensure_flow_exists();

    // Verify meta.json exists
    let meta_path = flow_dir.join(META_FILE);
    if !meta_path.exists() {
        error_exit("meta.json not found. Run 'flowctl init' first.");
    }

    // DB-based ID allocation
    let max_epic = find_max_epic_number();
    let epic_num = max_epic + 1;
    let slug = slugify(title, 40);
    let suffix = slug.unwrap_or_else(|| generate_epic_suffix(3));
    let epic_id = format!("fn-{epic_num}-{suffix}");

    // Collision check: only check spec file (no more epic MD)
    let spec_path = flow_dir.join(SPECS_DIR).join(format!("{epic_id}.md"));
    if spec_path.exists() {
        error_exit(&format!(
            "Refusing to overwrite existing epic {epic_id}. \
             This shouldn't happen - check for orphaned files."
        ));
    }

    let now = Utc::now();
    let branch_name = branch.clone().unwrap_or_else(|| epic_id.clone());

    let epic = Epic {
        schema_version: 1,
        id: epic_id.clone(),
        title: title.to_string(),
        status: EpicStatus::Open,
        branch_name: Some(branch_name),
        plan_review: ReviewStatus::Unknown,
        completion_review: ReviewStatus::Unknown,
        depends_on_epics: vec![],
        default_impl: None,
        default_review: None,
        default_sync: None,
        auto_execute_pending: None,
        auto_execute_set_at: None,
        archived: false,
        file_path: Some(format!("specs/{epic_id}.md")),
        created_at: now,
        updated_at: now,
    };

    // Write to DB (sole source of truth)
    let body = create_epic_spec_body(&epic_id, title);
    let doc = Document {
        frontmatter: epic,
        body: body.clone(),
    };
    save_epic(&doc);

    // Write spec file (body-only Markdown in specs/)
    if let Some(parent) = spec_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&spec_path, &body)
        .unwrap_or_else(|e| error_exit(&format!("Failed to write spec: {e}")));

    if json_mode {
        json_output(json!({
            "id": epic_id,
            "title": title,
            "spec_path": format!("{FLOW_DIR}/{SPECS_DIR}/{epic_id}.md"),
            "message": format!("Epic {epic_id} created"),
        }));
    } else {
        println!("Epic {epic_id} created: {title}");
    }
}

fn cmd_set_plan(id: &str, file: &str, json_mode: bool) {
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    let mut doc = load_epic(id);

    // Read content from file or stdin
    let content = read_file_or_stdin(file);

    // Validate: reject duplicate headings
    let heading_re = regex::Regex::new(r"(?m)^(##\s+.+?)\s*$").expect("valid regex");
    let mut seen = std::collections::HashMap::new();
    for cap in heading_re.captures_iter(&content) {
        let h = cap[1].to_string();
        *seen.entry(h).or_insert(0u32) += 1;
    }
    let duplicates: Vec<String> = seen
        .iter()
        .filter(|(_, &count)| count > 1)
        .map(|(h, count)| format!("Duplicate heading: {h} (found {count} times)"))
        .collect();
    if !duplicates.is_empty() {
        error_exit(&format!("Spec validation failed: {}", duplicates.join("; ")));
    }

    // Write spec file (body-only Markdown)
    let spec_path = flow_dir.join(SPECS_DIR).join(format!("{id}.md"));
    fs::write(&spec_path, &content)
        .unwrap_or_else(|e| error_exit(&format!("Failed to write spec: {e}")));

    // Update epic body + timestamp in DB
    doc.body = content;
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "spec_path": spec_path.to_string_lossy(),
            "message": format!("Epic {id} spec updated"),
        }));
    } else {
        println!("Epic {id} spec updated");
    }
}

fn cmd_set_plan_review_status(id: &str, status: &str, json_mode: bool) {
    ensure_flow_exists();
    validate_epic_id(id);

    let mut doc = load_epic(id);

    let review_status = match status {
        "ship" => ReviewStatus::Passed,
        "needs_work" => ReviewStatus::Failed,
        _ => ReviewStatus::Unknown,
    };

    doc.frontmatter.plan_review = review_status;
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "plan_review_status": status,
            "plan_reviewed_at": Utc::now().to_rfc3339(),
            "message": format!("Epic {id} plan review status set to {status}"),
        }));
    } else {
        println!("Epic {id} plan review status set to {status}");
    }
}

fn cmd_set_completion_review_status(id: &str, status: &str, json_mode: bool) {
    ensure_flow_exists();
    validate_epic_id(id);

    let mut doc = load_epic(id);

    let review_status = match status {
        "ship" => ReviewStatus::Passed,
        "needs_work" => ReviewStatus::Failed,
        _ => ReviewStatus::Unknown,
    };

    doc.frontmatter.completion_review = review_status;
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "completion_review_status": status,
            "completion_reviewed_at": Utc::now().to_rfc3339(),
            "message": format!("Epic {id} completion review status set to {status}"),
        }));
    } else {
        println!("Epic {id} completion review status set to {status}");
    }
}

fn cmd_set_branch(id: &str, branch: &str, json_mode: bool) {
    ensure_flow_exists();
    validate_epic_id(id);

    let mut doc = load_epic(id);

    doc.frontmatter.branch_name = Some(branch.to_string());
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "branch_name": branch,
            "message": format!("Epic {id} branch_name set to {branch}"),
        }));
    } else {
        println!("Epic {id} branch_name set to {branch}");
    }
}

fn cmd_set_title(id: &str, new_title: &str, json_mode: bool) {
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    let old_id = id;
    let doc = load_epic(old_id);

    // Extract epic number
    let parsed = parse_id(old_id)
        .unwrap_or_else(|_| error_exit(&format!("Could not parse epic number from {old_id}")));
    let epic_num = parsed.epic;

    // Generate new ID
    let new_slug = slugify(new_title, 40);
    let new_suffix = new_slug.unwrap_or_else(|| generate_epic_suffix(3));
    let new_id = format!("fn-{epic_num}-{new_suffix}");

    let specs_dir = flow_dir.join(SPECS_DIR);

    // Check collision (if ID changed) via DB
    if new_id != old_id {
        let conn = require_db()
            .unwrap_or_else(|e| error_exit(&format!("DB required: {e}")));
        let repo = crate::commands::db_shim::EpicRepo::new(&conn);
        if repo.get(&new_id).is_ok() {
            error_exit(&format!(
                "Epic {new_id} already exists. Choose a different title."
            ));
        }
    }

    // Rename spec file (only MD file we keep)
    let mut files_renamed = 0;
    let old_spec = specs_dir.join(format!("{old_id}.md"));
    if old_spec.exists() {
        let new_spec = specs_dir.join(format!("{new_id}.md"));
        if let Err(e) = fs::rename(&old_spec, &new_spec) {
            error_exit(&format!("Failed to rename spec file: {e}"));
        }
        files_renamed += 1;
    }

    // Rename checkpoint file
    let old_checkpoint = flow_dir.join(format!(".checkpoint-{old_id}.json"));
    if old_checkpoint.exists() {
        let _ = fs::rename(
            &old_checkpoint,
            flow_dir.join(format!(".checkpoint-{new_id}.json")),
        );
        files_renamed += 1;
    }

    // Update epic in DB
    let mut new_doc = doc;
    new_doc.frontmatter.id = new_id.clone();
    new_doc.frontmatter.title = new_title.to_string();
    new_doc.frontmatter.file_path = Some(format!("specs/{new_id}.md"));
    new_doc.frontmatter.updated_at = Utc::now();
    save_epic(&new_doc);

    // Update task records in DB
    let conn = require_db()
        .unwrap_or_else(|e| error_exit(&format!("DB required: {e}")));
    let task_repo = crate::commands::db_shim::TaskRepo::new(&conn);
    let tasks = task_repo.list_by_epic(old_id).unwrap_or_default();
    let mut task_renames: Vec<(String, String)> = Vec::new();
    for task in &tasks {
        if let Ok(p) = parse_id(&task.id) {
            if let Some(task_num) = p.task {
                let new_task_id = format!("{new_id}.{task_num}");
                task_renames.push((task.id.clone(), new_task_id));
            }
        }
    }

    let task_id_map: std::collections::HashMap<&str, &str> = task_renames
        .iter()
        .map(|(o, n)| (o.as_str(), n.as_str()))
        .collect();
    for task in &tasks {
        let new_task_id = task_id_map
            .get(task.id.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| task.id.clone());
        let mut updated_task = task.clone();
        updated_task.id = new_task_id.clone();
        updated_task.epic = new_id.clone();
        updated_task.file_path = Some(format!("tasks/{new_task_id}.md"));
        updated_task.depends_on = updated_task
            .depends_on
            .iter()
            .map(|dep| {
                task_id_map
                    .get(dep.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| dep.clone())
            })
            .collect();
        updated_task.updated_at = Utc::now();
        let _ = task_repo.upsert(&updated_task);
    }

    // Update depends_on_epics in other epics that reference old_id (via DB)
    let mut updated_deps_in: Vec<String> = Vec::new();
    let epic_repo = crate::commands::db_shim::EpicRepo::new(&conn);
    if let Ok(all_epics) = epic_repo.list(None) {
        let dep_repo = crate::commands::db_shim::DepRepo::new(&conn);
        for other_epic in &all_epics {
            if other_epic.id == new_id || other_epic.id == old_id {
                continue;
            }
            if other_epic.depends_on_epics.contains(&old_id.to_string()) {
                // Update: remove old dep, add new one
                let _ = dep_repo.remove_epic_dep(&other_epic.id, old_id);
                let _ = dep_repo.add_epic_dep(&other_epic.id, &new_id);
                // Also update the Epic struct's depends_on_epics
                let mut updated_other = other_epic.clone();
                updated_other.depends_on_epics = updated_other
                    .depends_on_epics
                    .iter()
                    .map(|d| {
                        if d == old_id {
                            new_id.clone()
                        } else {
                            d.clone()
                        }
                    })
                    .collect();
                updated_other.updated_at = Utc::now();
                let _ = epic_repo.upsert(&updated_other);
                updated_deps_in.push(other_epic.id.clone());
            }
        }
    }

    let mut result = json!({
        "old_id": old_id,
        "new_id": new_id,
        "title": new_title,
        "files_renamed": files_renamed,
        "tasks_updated": task_renames.len(),
        "message": format!("Epic renamed: {old_id} -> {new_id}"),
    });
    if !updated_deps_in.is_empty() {
        result["updated_deps_in"] = json!(updated_deps_in);
    }

    if json_mode {
        json_output(result);
    } else {
        println!("Epic renamed: {old_id} -> {new_id}");
        println!("  Title: {new_title}");
        println!("  Files renamed: {files_renamed}");
        println!("  Tasks updated: {}", task_renames.len());
        if !updated_deps_in.is_empty() {
            println!("  Updated deps in: {}", updated_deps_in.join(", "));
        }
    }
}

fn cmd_close(id: &str, skip_gap_check: bool, json_mode: bool) {
    ensure_flow_exists();
    validate_epic_id(id);

    let mut doc = load_epic(id);

    // Check all tasks are done/skipped via DB
    let conn = require_db()
        .unwrap_or_else(|e| error_exit(&format!("DB required: {e}")));
    let task_repo = crate::commands::db_shim::TaskRepo::new(&conn);
    let tasks = task_repo.list_by_epic(id).unwrap_or_default();

    let incomplete: Vec<String> = tasks
        .iter()
        .filter(|t| {
            let s = t.status.to_string();
            s != "done" && s != "skipped"
        })
        .map(|t| format!("{} ({})", t.id, t.status))
        .collect();

    if !incomplete.is_empty() {
        error_exit(&format!(
            "Cannot close epic: incomplete tasks - {}",
            incomplete.join(", ")
        ));
    }

    // Gap registry gate — check DB gaps table
    let gap_repo = crate::commands::db_shim::GapRepo::new(&conn);
    let mut open_blocking_count = 0;
    let mut gap_list_parts: Vec<String> = Vec::new();

    if let Ok(gaps) = gap_repo.list(id, Some("open")) {
        for gap in &gaps {
            if GAP_BLOCKING_PRIORITIES.contains(&gap.priority.as_str()) {
                open_blocking_count += 1;
                gap_list_parts.push(format!("[{}] {}", gap.priority, gap.capability));
            }
        }
    }

    if open_blocking_count > 0 && !skip_gap_check {
        error_exit(&format!(
            "Cannot close epic: {open_blocking_count} unresolved blocking gap(s): {}. \
             Use --skip-gap-check to bypass.",
            gap_list_parts.join(", ")
        ));
    }
    if open_blocking_count > 0 && skip_gap_check && !json_mode {
        eprintln!(
            "WARNING: Bypassing {open_blocking_count} unresolved blocking gap(s)"
        );
    }

    doc.frontmatter.status = EpicStatus::Done;
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "status": "done",
            "message": format!("Epic {id} closed"),
            "gaps_skipped": if skip_gap_check { open_blocking_count } else { 0 },
            "retro_suggested": true,
        }));
    } else {
        println!("Epic {id} closed");
        println!(
            "\n  Tip: Run /flow-code:retro to capture lessons learned before archiving."
        );
    }
}

fn cmd_reopen(id: &str, json_mode: bool) {
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    // Check if archived (check .archive/ dir for specs/reviews)
    let archive_path = flow_dir.join(ARCHIVE_DIR).join(id);
    if archive_path.exists() {
        // Check if epic is marked archived in DB
        let conn = require_db()
            .unwrap_or_else(|e| error_exit(&format!("DB required: {e}")));
        let repo = crate::commands::db_shim::EpicRepo::new(&conn);
        if let Ok(epic) = repo.get(id) {
            if epic.archived {
                error_exit(&format!(
                    "Epic {id} is archived. Unarchive it first before reopening."
                ));
            }
        }
    }

    let mut doc = load_epic(id);
    let previous_status = doc.frontmatter.status.to_string();

    if doc.frontmatter.status == EpicStatus::Open {
        error_exit(&format!(
            "Epic {id} is already open (no-op protection)"
        ));
    }

    doc.frontmatter.status = EpicStatus::Open;
    doc.frontmatter.completion_review = ReviewStatus::Unknown;
    doc.frontmatter.plan_review = ReviewStatus::Unknown;
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "previous_status": previous_status,
            "new_status": "open",
            "message": format!("Epic {id} reopened"),
        }));
    } else {
        println!("Epic {id} reopened (was: {previous_status})");
    }
}

fn cmd_archive(id: &str, force: bool, json_mode: bool) {
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    let mut doc = load_epic(id);

    if doc.frontmatter.status != EpicStatus::Done && !force {
        error_exit(&format!(
            "Cannot archive epic {id}: status is '{}', not 'done'. \
             Close it first or use --force.",
            doc.frontmatter.status
        ));
    }

    // Set archived=true in DB
    doc.frontmatter.archived = true;
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    // Build archive directory for specs and reviews
    let archive_dir = flow_dir.join(ARCHIVE_DIR).join(id);
    fs::create_dir_all(&archive_dir)
        .unwrap_or_else(|e| error_exit(&format!("Failed to create archive dir: {e}")));

    let mut moved: Vec<String> = Vec::new();

    // Move spec file
    let spec_path = flow_dir.join(SPECS_DIR).join(format!("{id}.md"));
    if spec_path.exists() {
        let dest = archive_dir.join(spec_path.file_name().unwrap());
        let _ = fs::rename(&spec_path, &dest);
        moved.push(format!("specs/{}", spec_path.file_name().unwrap().to_string_lossy()));
    }

    // Move review receipts
    let reviews_dir = flow_dir.join(REVIEWS_DIR);
    if reviews_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&reviews_dir) {
            let mut review_entries: Vec<_> = entries.flatten().collect();
            review_entries.sort_by_key(|e| e.file_name());
            for entry in review_entries {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if review_belongs_to_epic(&name_str, id) {
                    let dest = archive_dir.join(&*name);
                    let _ = fs::rename(entry.path(), &dest);
                    moved.push(format!("reviews/{name_str}"));
                }
            }
        }
    }

    if json_mode {
        json_output(json!({
            "epic": id,
            "archive_dir": archive_dir.to_string_lossy(),
            "moved": moved,
            "count": moved.len(),
        }));
    } else {
        println!(
            "Archived epic {id} ({} files) \u{2192} .flow/.archive/{id}/",
            moved.len()
        );
        for f in &moved {
            println!("  {f}");
        }
    }
}

fn cmd_clean(json_mode: bool) {
    let flow_dir = ensure_flow_exists();

    let conn = require_db()
        .unwrap_or_else(|e| error_exit(&format!("DB required: {e}")));
    let epic_repo = crate::commands::db_shim::EpicRepo::new(&conn);

    let mut archived: Vec<String> = Vec::new();

    if let Ok(epics) = epic_repo.list(Some("done")) {
        for epic in &epics {
            if !epic.archived {
                cmd_archive_silent(&epic.id, &flow_dir);
                archived.push(epic.id.clone());
            }
        }
    }

    if json_mode {
        json_output(json!({
            "archived": archived,
            "count": archived.len(),
        }));
    } else if archived.is_empty() {
        println!("No closed epics to archive.");
    } else {
        println!(
            "Archived {} closed epic(s): {}",
            archived.len(),
            archived.join(", ")
        );
    }
}

/// Silent archive helper for clean command (no output).
/// Sets archived=true in DB, moves only specs and reviews to .archive/.
fn cmd_archive_silent(id: &str, flow_dir: &Path) {
    // Set archived=true in DB
    if let Ok(conn) = require_db() {
        let repo = crate::commands::db_shim::EpicRepo::new(&conn);
        if let Ok(mut epic) = repo.get(id) {
            epic.archived = true;
            epic.updated_at = Utc::now();
            let _ = repo.upsert(&epic);
        }
    }

    let archive_dir = flow_dir.join(ARCHIVE_DIR).join(id);
    let _ = fs::create_dir_all(&archive_dir);

    // Move spec
    let spec_path = flow_dir.join(SPECS_DIR).join(format!("{id}.md"));
    if spec_path.exists() {
        let _ = fs::rename(&spec_path, archive_dir.join(format!("{id}.md")));
    }

    // Move reviews
    let reviews_dir = flow_dir.join(REVIEWS_DIR);
    if reviews_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&reviews_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if review_belongs_to_epic(&name_str, id) {
                    let _ = fs::rename(entry.path(), archive_dir.join(&name));
                }
            }
        }
    }
}

fn cmd_add_dep(epic_id: &str, dep_id: &str, json_mode: bool) {
    ensure_flow_exists();
    validate_epic_id(epic_id);
    validate_epic_id(dep_id);

    if epic_id == dep_id {
        error_exit("Epic cannot depend on itself");
    }

    // Verify dep epic exists in DB
    let conn = require_db()
        .unwrap_or_else(|e| error_exit(&format!("DB required: {e}")));
    let repo = crate::commands::db_shim::EpicRepo::new(&conn);
    if repo.get(dep_id).is_err() {
        error_exit(&format!("Epic {dep_id} not found"));
    }

    let mut doc = load_epic(epic_id);

    if doc.frontmatter.depends_on_epics.contains(&dep_id.to_string()) {
        if json_mode {
            json_output(json!({
                "id": epic_id,
                "depends_on_epics": doc.frontmatter.depends_on_epics,
                "message": format!("{dep_id} already in dependencies"),
            }));
        } else {
            println!("{dep_id} already in {epic_id} dependencies");
        }
        return;
    }

    // Update DB via DepRepo
    let dep_repo = crate::commands::db_shim::DepRepo::new(&conn);
    if let Err(e) = dep_repo.add_epic_dep(epic_id, dep_id) {
        error_exit(&format!("Failed to add epic dep: {e}"));
    }

    doc.frontmatter.depends_on_epics.push(dep_id.to_string());
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": epic_id,
            "depends_on_epics": doc.frontmatter.depends_on_epics,
            "message": format!("Added {dep_id} to {epic_id} dependencies"),
        }));
    } else {
        println!("Added {dep_id} to {epic_id} dependencies");
    }
}

fn cmd_rm_dep(epic_id: &str, dep_id: &str, json_mode: bool) {
    ensure_flow_exists();
    validate_epic_id(epic_id);

    let mut doc = load_epic(epic_id);

    if !doc.frontmatter.depends_on_epics.contains(&dep_id.to_string()) {
        if json_mode {
            json_output(json!({
                "id": epic_id,
                "depends_on_epics": doc.frontmatter.depends_on_epics,
                "message": format!("{dep_id} not in dependencies"),
            }));
        } else {
            println!("{dep_id} not in {epic_id} dependencies");
        }
        return;
    }

    // Update DB via DepRepo
    let conn = require_db()
        .unwrap_or_else(|e| error_exit(&format!("DB required: {e}")));
    let dep_repo = crate::commands::db_shim::DepRepo::new(&conn);
    if let Err(e) = dep_repo.remove_epic_dep(epic_id, dep_id) {
        error_exit(&format!("Failed to remove epic dep: {e}"));
    }

    doc.frontmatter
        .depends_on_epics
        .retain(|d| d != dep_id);
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": epic_id,
            "depends_on_epics": doc.frontmatter.depends_on_epics,
            "message": format!("Removed {dep_id} from {epic_id} dependencies"),
        }));
    } else {
        println!("Removed {dep_id} from {epic_id} dependencies");
    }
}

fn cmd_set_backend(
    id: &str,
    impl_spec: &Option<String>,
    review: &Option<String>,
    sync: &Option<String>,
    json_mode: bool,
) {
    ensure_flow_exists();
    validate_epic_id(id);

    if impl_spec.is_none() && review.is_none() && sync.is_none() {
        error_exit("At least one of --impl, --review, or --sync must be provided");
    }

    let mut doc = load_epic(id);

    let mut updated: Vec<String> = Vec::new();

    if let Some(val) = impl_spec {
        let v = if val.is_empty() { None } else { Some(val.clone()) };
        doc.frontmatter.default_impl = v;
        updated.push(format!(
            "default_impl={}",
            impl_spec.as_deref().unwrap_or("null")
        ));
    }
    if let Some(val) = review {
        let v = if val.is_empty() { None } else { Some(val.clone()) };
        doc.frontmatter.default_review = v;
        updated.push(format!(
            "default_review={}",
            review.as_deref().unwrap_or("null")
        ));
    }
    if let Some(val) = sync {
        let v = if val.is_empty() { None } else { Some(val.clone()) };
        doc.frontmatter.default_sync = v;
        updated.push(format!(
            "default_sync={}",
            sync.as_deref().unwrap_or("null")
        ));
    }

    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "default_impl": doc.frontmatter.default_impl,
            "default_review": doc.frontmatter.default_review,
            "default_sync": doc.frontmatter.default_sync,
            "message": format!("Epic {id} backend specs updated: {}", updated.join(", ")),
        }));
    } else {
        println!(
            "Epic {id} backend specs updated: {}",
            updated.join(", ")
        );
    }
}

fn cmd_set_auto_execute(id: &str, pending: bool, done: bool, json_mode: bool) {
    ensure_flow_exists();
    validate_epic_id(id);

    if !pending && !done {
        error_exit("Either --pending or --done must be specified");
    }

    let mut doc = load_epic(id);

    let action;
    if pending {
        doc.frontmatter.auto_execute_pending = Some(true);
        doc.frontmatter.auto_execute_set_at = Some(Utc::now().to_rfc3339());
        action = "pending";
    } else {
        doc.frontmatter.auto_execute_pending = Some(false);
        action = "done";
    }

    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "auto_execute_pending": doc.frontmatter.auto_execute_pending,
            "auto_execute_set_at": doc.frontmatter.auto_execute_set_at,
            "message": format!("Epic {id} auto_execute set to {action}"),
        }));
    } else {
        println!("Epic {id} auto_execute set to {action}");
    }
}

// ── Shared helpers ─────────────────────────────────────────────────

/// Returns true if a review filename belongs to the given epic.
///
/// Matches both naming schemes used in `.flow/reviews/`:
/// - Task-suffixed reviews (plan/impl/cross-model): `*-{epic_id}.<task-num>-*.json`
///   matched via the `-{id}.` infix
/// - Epic-level audit receipts: `epic-audit-{id}-<timestamp>.json`
///   matched via the `-{id}-` prefix
fn review_belongs_to_epic(name: &str, id: &str) -> bool {
    name.contains(&format!("-{id}.")) || name.starts_with(&format!("epic-audit-{id}-"))
}

// ── Audit command ───────────────────────────────────────────────────

/// Find the most recent `epic-audit-<id>-*.json` receipt in `.flow/reviews/`.
/// Returns `(path, age_hours)` or `None` if none exists.
fn find_recent_audit(flow_dir: &Path, id: &str) -> Option<(PathBuf, f64)> {
    let reviews_dir = flow_dir.join(REVIEWS_DIR);
    if !reviews_dir.is_dir() {
        return None;
    }
    let prefix = format!("epic-audit-{id}-");
    let entries = fs::read_dir(&reviews_dir).ok()?;
    let mut best: Option<(PathBuf, std::time::SystemTime)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with(&prefix) || !name_str.ends_with(".json") {
            continue;
        }
        let path = entry.path();
        let modified = entry.metadata().and_then(|m| m.modified()).ok();
        if let Some(mtime) = modified {
            match &best {
                None => best = Some((path, mtime)),
                Some((_, cur)) if mtime > *cur => best = Some((path, mtime)),
                _ => {}
            }
        }
    }
    let (path, mtime) = best?;
    let age = std::time::SystemTime::now()
        .duration_since(mtime)
        .ok()?
        .as_secs_f64()
        / 3600.0;
    Some((path, age))
}

fn cmd_audit(id: &str, force: bool, json_mode: bool) {
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    // Re-use recent audit unless --force.
    if !force {
        if let Some((existing, age_hours)) = find_recent_audit(&flow_dir, id) {
            if age_hours < 24.0 {
                if json_mode {
                    json_output(json!({
                        "id": id,
                        "reused": true,
                        "receipt_path": existing.to_string_lossy(),
                        "age_hours": age_hours,
                        "message": format!(
                            "Reusing audit receipt from {:.1}h ago. Pass --force to regenerate.",
                            age_hours
                        ),
                    }));
                } else {
                    println!(
                        "Reusing audit receipt ({:.1}h old): {}",
                        age_hours,
                        existing.display()
                    );
                    println!("Pass --force to regenerate.");
                }
                return;
            }
        }
    }

    // Load epic spec (frontmatter + body) from DB.
    let epic_doc = load_epic(id);
    let epic_body = epic_doc.body.clone();

    // Load tasks from DB.
    let conn = require_db().ok();
    let tasks: Vec<flowctl_core::types::Task> = load_epic_tasks(conn.as_ref(), &flow_dir, id);

    // Shape task summaries for the payload.
    let task_entries: Vec<serde_json::Value> = tasks
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "title": t.title,
                "status": format!("{:?}", t.status).to_lowercase(),
                "domain": format!("{:?}", t.domain).to_lowercase(),
                "depends_on": t.depends_on,
                "files": t.files,
            })
        })
        .collect();

    // Assemble payload receipt.
    let timestamp = Utc::now();
    let receipt = json!({
        "schema_version": 1,
        "kind": "epic-audit-payload",
        "epic_id": id,
        "generated_at": timestamp.to_rfc3339(),
        "epic": {
            "id": epic_doc.frontmatter.id,
            "title": epic_doc.frontmatter.title,
            "status": format!("{:?}", epic_doc.frontmatter.status).to_lowercase(),
            "spec_body": epic_body,
        },
        "tasks": task_entries,
        "task_count": tasks.len(),
        // Audit findings placeholder — populated by agents/epic-auditor.md.
        "audit": {
            "coverage_score": null,
            "gaps": [],
            "redundancies": [],
            "recommendations": [],
            "notes": "Pending auditor agent — run agents/epic-auditor.md against this payload."
        }
    });

    // Write receipt.
    let reviews_dir = flow_dir.join(REVIEWS_DIR);
    if let Err(e) = fs::create_dir_all(&reviews_dir) {
        error_exit(&format!("Failed to create reviews dir: {e}"));
    }
    let ts_slug = timestamp.format("%Y%m%dT%H%M%SZ").to_string();
    let receipt_path = reviews_dir.join(format!("epic-audit-{id}-{ts_slug}.json"));
    let serialized = serde_json::to_string_pretty(&receipt)
        .unwrap_or_else(|e| error_exit(&format!("Failed to serialize audit: {e}")));
    fs::write(&receipt_path, &serialized)
        .unwrap_or_else(|e| error_exit(&format!("Failed to write {}: {e}", receipt_path.display())));

    if json_mode {
        json_output(json!({
            "id": id,
            "reused": false,
            "receipt_path": receipt_path.to_string_lossy(),
            "task_count": tasks.len(),
            "message": format!(
                "Wrote audit payload to {}. Run agents/epic-auditor.md to populate findings.",
                receipt_path.display()
            ),
        }));
    } else {
        println!("Wrote audit payload: {}", receipt_path.display());
        println!("  Epic: {id} ({} tasks)", tasks.len());
        println!("  Next: run agents/epic-auditor.md with receipt path as input");
    }
}

// ── Dispatch ────────────────────────────────────────────────────────

pub fn dispatch(cmd: &EpicCmd, json: bool) {
    match cmd {
        EpicCmd::Create { title, branch } => cmd_create(title, branch, json),
        EpicCmd::Plan { id, file } => cmd_set_plan(id, file, json),
        EpicCmd::Review { id, status } => cmd_set_plan_review_status(id, status, json),
        EpicCmd::Completion { id, status } => cmd_set_completion_review_status(id, status, json),
        EpicCmd::Branch { id, name } => cmd_set_branch(id, name, json),
        EpicCmd::Title { id, title } => cmd_set_title(id, title, json),
        EpicCmd::Close {
            id,
            skip_gap_check,
        } => cmd_close(id, *skip_gap_check, json),
        EpicCmd::Reopen { id } => cmd_reopen(id, json),
        EpicCmd::Archive { id, force } => cmd_archive(id, *force, json),
        EpicCmd::Clean => cmd_clean(json),
        EpicCmd::Audit { id, force } => cmd_audit(id, *force, json),
        EpicCmd::AddDep { epic, depends_on } => cmd_add_dep(epic, depends_on, json),
        EpicCmd::RmDep { epic, depends_on } => cmd_rm_dep(epic, depends_on, json),
        EpicCmd::SetBackend {
            id,
            impl_spec,
            review,
            sync,
        } => cmd_set_backend(id, impl_spec, review, sync, json),
        EpicCmd::AutoExec {
            id,
            pending,
            done,
        } => cmd_set_auto_execute(id, *pending, *done, json),
    }
}

// ── Replay command ────────────────────���────────────────────────────

pub fn cmd_replay(json_mode: bool, epic_id: &str, dry_run: bool, force: bool) {
    ensure_flow_exists();
    validate_epic_id(epic_id);

    let conn = require_db()
        .unwrap_or_else(|e| error_exit(&format!("DB required: {e}")));

    // Load tasks from DB
    let task_repo = crate::commands::db_shim::TaskRepo::new(&conn);
    let tasks = task_repo.list_by_epic(epic_id).unwrap_or_default();
    if tasks.is_empty() {
        error_exit(&format!("No tasks found for epic {}", epic_id));
    }

    // Check for in_progress tasks unless force
    if !force {
        let in_progress: Vec<&str> = tasks
            .iter()
            .filter(|t| t.status == flowctl_core::state_machine::Status::InProgress)
            .map(|t| t.id.as_str())
            .collect();
        if !in_progress.is_empty() {
            error_exit(&format!(
                "Tasks in progress: {}. Use --force to override.",
                in_progress.join(", ")
            ));
        }
    }

    // Count what would be reset
    let to_reset: Vec<&flowctl_core::types::Task> = tasks
        .iter()
        .filter(|t| t.status != flowctl_core::state_machine::Status::Todo)
        .collect();

    if dry_run {
        if json_mode {
            let ids: Vec<&str> = to_reset.iter().map(|t| t.id.as_str()).collect();
            json_output(json!({
                "dry_run": true,
                "epic": epic_id,
                "would_reset": ids,
                "count": ids.len(),
            }));
        } else {
            println!("Dry run — would reset {} task(s) to todo:", to_reset.len());
            for t in &to_reset {
                println!("  {} ({}) -> todo", t.id, t.status);
            }
        }
        return;
    }

    // Reset all tasks to todo in DB only
    let mut reset_count = 0;
    for task in &to_reset {
        if let Err(e) = task_repo.update_status(&task.id, flowctl_core::state_machine::Status::Todo) {
            eprintln!("Warning: failed to reset {} in DB: {}", task.id, e);
        }
        reset_count += 1;
    }

    if json_mode {
        let ids: Vec<&str> = to_reset.iter().map(|t| t.id.as_str()).collect();
        json_output(json!({
            "epic": epic_id,
            "reset": ids,
            "count": reset_count,
            "message": format!("Run /flow-code:work {} to re-execute", epic_id),
        }));
    } else {
        println!("Reset {} task(s) to todo for epic {}", reset_count, epic_id);
        println!();
        println!("To re-execute, run:  /flow-code:work {}", epic_id);
    }
}

/// Load tasks for an epic from DB (sole source of truth).
fn load_epic_tasks(
    conn: Option<&crate::commands::db_shim::Connection>,
    _flow_dir: &Path,
    epic_id: &str,
) -> Vec<flowctl_core::types::Task> {
    if let Some(c) = conn {
        let task_repo = crate::commands::db_shim::TaskRepo::new(c);
        if let Ok(tasks) = task_repo.list_by_epic(epic_id) {
            return tasks;
        }
    }
    Vec::new()
}

// ── Diff command ────────────��───────────────────────────���──────────

pub fn cmd_diff(json_mode: bool, epic_id: &str) {
    ensure_flow_exists();
    validate_epic_id(epic_id);

    // Load epic to get branch name from DB
    let branch = load_epic_branch(epic_id);

    let branch = match branch {
        Some(b) => b,
        None => error_exit(&format!(
            "No branch found for epic {}. Set with: flowctl epic set-branch {} --branch <name>",
            epic_id, epic_id
        )),
    };

    // Find merge base with main
    let merge_base = std::process::Command::new("git")
        .args(["merge-base", "main", &branch])
        .output();

    let base_ref = match merge_base {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => {
            // Fallback: try to use the branch directly
            eprintln!("Warning: could not find merge-base with main, showing full branch history");
            String::new()
        }
    };

    // Git log
    let range_spec = format!("{}..{}", base_ref, branch);
    let log_output = if base_ref.is_empty() {
        std::process::Command::new("git")
            .args(["log", "--oneline", "-20", &branch])
            .output()
    } else {
        std::process::Command::new("git")
            .args(["log", "--oneline", &range_spec])
            .output()
    };

    let log_text = match log_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => String::new(),
    };

    // Git diff --stat
    let diff_output = if base_ref.is_empty() {
        std::process::Command::new("git")
            .args(["diff", "--stat", &branch])
            .output()
    } else {
        std::process::Command::new("git")
            .args(["diff", "--stat", &range_spec])
            .output()
    };

    let diff_text = match diff_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => String::new(),
    };

    if json_mode {
        json_output(json!({
            "epic": epic_id,
            "branch": branch,
            "base_ref": if base_ref.is_empty() { None } else { Some(&base_ref) },
            "log": log_text,
            "diff_stat": diff_text,
        }));
    } else {
        println!("Epic: {}  Branch: {}", epic_id, branch);
        if !base_ref.is_empty() {
            println!("Base: {}", &base_ref[..base_ref.len().min(12)]);
        }
        println!();

        if !log_text.is_empty() {
            println!("Commits:");
            for line in log_text.lines() {
                println!("  {}", line);
            }
            println!();
        } else {
            println!("No commits found.");
            println!();
        }

        if !diff_text.is_empty() {
            println!("Diff summary:");
            for line in diff_text.lines() {
                println!("  {}", line);
            }
        } else {
            println!("No diff.");
        }
    }
}

/// Load branch name for an epic from DB (sole source of truth).
fn load_epic_branch(epic_id: &str) -> Option<String> {
    let conn = require_db().ok()?;
    let epic_repo = crate::commands::db_shim::EpicRepo::new(&conn);
    let epic = epic_repo.get(epic_id).ok()?;
    epic.branch_name.filter(|b| !b.is_empty())
}
