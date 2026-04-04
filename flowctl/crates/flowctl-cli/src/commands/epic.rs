//! Epic management commands.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use clap::Subcommand;
use regex::Regex;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::frontmatter;
use flowctl_core::id::{generate_epic_suffix, is_epic_id, is_task_id, parse_id, slugify};
use flowctl_core::types::{
    Epic, EpicStatus, ReviewStatus, Task, ARCHIVE_DIR, EPICS_DIR, FLOW_DIR, META_FILE,
    REVIEWS_DIR, SPECS_DIR, TASKS_DIR,
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
    /// Set epic spec from file.
    SetPlan {
        /// Epic ID.
        id: String,
        /// Markdown file (use '-' for stdin).
        #[arg(long)]
        file: String,
    },
    /// Set plan review status.
    SetPlanReviewStatus {
        /// Epic ID.
        id: String,
        /// Review status.
        #[arg(long, value_parser = ["ship", "needs_work", "unknown"])]
        status: String,
    },
    /// Set completion review status.
    SetCompletionReviewStatus {
        /// Epic ID.
        id: String,
        /// Review status.
        #[arg(long, value_parser = ["ship", "needs_work", "unknown"])]
        status: String,
    },
    /// Set epic branch name.
    SetBranch {
        /// Epic ID.
        id: String,
        /// Branch name.
        #[arg(long)]
        branch: String,
    },
    /// Rename epic by setting a new title.
    SetTitle {
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
    SetAutoExecute {
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

// ── Helpers ─────────────────────────────────────────────────────────

/// Get the .flow/ directory path.
fn get_flow_dir() -> PathBuf {
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(FLOW_DIR)
}

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

/// Load epic from Markdown frontmatter file, error_exit if not found or parse fails.
fn load_epic(epic_path: &Path, id: &str) -> frontmatter::Document<Epic> {
    if !epic_path.exists() {
        error_exit(&format!("Epic {id} not found"));
    }
    let content = fs::read_to_string(epic_path)
        .unwrap_or_else(|e| error_exit(&format!("Failed to read {}: {e}", epic_path.display())));
    frontmatter::parse::<Epic>(&content)
        .unwrap_or_else(|e| error_exit(&format!("Failed to parse epic {id}: {e}")))
}

/// Write an epic document back to its Markdown file.
fn save_epic(epic_path: &Path, doc: &frontmatter::Document<Epic>) {
    let content = frontmatter::write(doc)
        .unwrap_or_else(|e| error_exit(&format!("Failed to serialize epic: {e}")));
    if let Some(parent) = epic_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(epic_path, &content)
        .unwrap_or_else(|e| error_exit(&format!("Failed to write {}: {e}", epic_path.display())));
}

/// Try to open DB connection for SQLite dual-write.
fn try_open_db() -> Option<rusqlite::Connection> {
    let cwd = env::current_dir().ok()?;
    flowctl_db::open(&cwd).ok()
}

/// Upsert epic into SQLite if DB is available.
fn db_upsert_epic(epic: &Epic) {
    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::EpicRepo::new(&conn);
        let _ = repo.upsert(epic);
    }
}

/// Scan .flow/epics/ and .flow/specs/ to find max epic number.
/// Returns 0 if none exist.
fn scan_max_epic_id(flow_dir: &Path) -> u32 {
    let pattern = Regex::new(r"^fn-(\d+)(?:-[a-z0-9][a-z0-9-]*[a-z0-9]|-[a-z0-9]{1,3})?\.(md|json)$")
        .expect("valid regex");

    let mut max_n: u32 = 0;

    // Scan epics/*.md
    let epics_dir = flow_dir.join(EPICS_DIR);
    if epics_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&epics_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if let Some(caps) = pattern.captures(&name_str) {
                    if let Ok(n) = caps[1].parse::<u32>() {
                        max_n = max_n.max(n);
                    }
                }
            }
        }
    }

    // Scan specs/*.md as safety net
    let specs_dir = flow_dir.join(SPECS_DIR);
    if specs_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&specs_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if let Some(caps) = pattern.captures(&name_str) {
                    if let Ok(n) = caps[1].parse::<u32>() {
                        max_n = max_n.max(n);
                    }
                }
            }
        }
    }

    max_n
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

/// Load epic as raw JSON Value from frontmatter (for gap checks and extra fields).
/// Falls back to trying JSON format for legacy compatibility.
fn load_epic_raw(epic_path: &Path, id: &str) -> serde_json::Value {
    if !epic_path.exists() {
        error_exit(&format!("Epic {id} not found"));
    }
    let content = fs::read_to_string(epic_path)
        .unwrap_or_else(|e| error_exit(&format!("Failed to read {}: {e}", epic_path.display())));

    // Try frontmatter parse first
    if content.trim_start().starts_with("---") {
        match frontmatter::parse::<serde_json::Value>(&content) {
            Ok(doc) => return doc.frontmatter,
            Err(_) => {}
        }
    }

    // Fall back to raw JSON
    serde_json::from_str(&content)
        .unwrap_or_else(|e| error_exit(&format!("Failed to parse epic {id}: {e}")))
}

// ── Command implementations ─────────────────────────────────────────

fn cmd_create(title: &str, branch: &Option<String>, json_mode: bool) {
    let flow_dir = ensure_flow_exists();

    // Verify meta.json exists
    let meta_path = flow_dir.join(META_FILE);
    if !meta_path.exists() {
        error_exit("meta.json not found. Run 'flowctl init' first.");
    }

    // Scan-based ID allocation
    let max_epic = scan_max_epic_id(&flow_dir);
    let epic_num = max_epic + 1;
    let slug = slugify(title, 40);
    let suffix = slug.unwrap_or_else(|| generate_epic_suffix(3));
    let epic_id = format!("fn-{epic_num}-{suffix}");

    // Collision check
    let epic_md_path = flow_dir.join(EPICS_DIR).join(format!("{epic_id}.md"));
    let spec_path = flow_dir.join(SPECS_DIR).join(format!("{epic_id}.md"));
    if epic_md_path.exists() || spec_path.exists() {
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
        file_path: Some(format!("epics/{epic_id}.md")),
        created_at: now,
        updated_at: now,
    };

    // Write epic Markdown
    let body = create_epic_spec_body(&epic_id, title);
    let doc = frontmatter::Document {
        frontmatter: epic.clone(),
        body: body.clone(),
    };
    save_epic(&epic_md_path, &doc);

    // Write spec file (separate body-only file in specs/)
    if let Some(parent) = spec_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&spec_path, &body)
        .unwrap_or_else(|e| error_exit(&format!("Failed to write spec: {e}")));

    // SQLite dual-write
    db_upsert_epic(&epic);

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

    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{id}.md"));
    let mut doc = load_epic(&epic_path, id);

    // Read content from file or stdin
    let content = read_file_or_stdin(file);

    // Validate: reject duplicate headings
    let heading_re = Regex::new(r"(?m)^(##\s+.+?)\s*$").expect("valid regex");
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

    // Write spec
    let spec_path = flow_dir.join(SPECS_DIR).join(format!("{id}.md"));
    fs::write(&spec_path, &content)
        .unwrap_or_else(|e| error_exit(&format!("Failed to write spec: {e}")));

    // Update epic timestamp
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&epic_path, &doc);
    db_upsert_epic(&doc.frontmatter);

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
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{id}.md"));
    let mut doc = load_epic(&epic_path, id);

    let review_status = match status {
        "ship" => ReviewStatus::Passed,
        "needs_work" => ReviewStatus::Failed,
        _ => ReviewStatus::Unknown,
    };

    doc.frontmatter.plan_review = review_status;
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&epic_path, &doc);
    db_upsert_epic(&doc.frontmatter);

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
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{id}.md"));
    let mut doc = load_epic(&epic_path, id);

    let review_status = match status {
        "ship" => ReviewStatus::Passed,
        "needs_work" => ReviewStatus::Failed,
        _ => ReviewStatus::Unknown,
    };

    doc.frontmatter.completion_review = review_status;
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&epic_path, &doc);
    db_upsert_epic(&doc.frontmatter);

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
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{id}.md"));
    let mut doc = load_epic(&epic_path, id);

    doc.frontmatter.branch_name = Some(branch.to_string());
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&epic_path, &doc);
    db_upsert_epic(&doc.frontmatter);

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
    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{old_id}.md"));
    let doc = load_epic(&epic_path, old_id);

    // Extract epic number
    let parsed = parse_id(old_id)
        .unwrap_or_else(|_| error_exit(&format!("Could not parse epic number from {old_id}")));
    let epic_num = parsed.epic;

    // Generate new ID
    let new_slug = slugify(new_title, 40);
    let new_suffix = new_slug.unwrap_or_else(|| generate_epic_suffix(3));
    let new_id = format!("fn-{epic_num}-{new_suffix}");

    let epics_dir = flow_dir.join(EPICS_DIR);
    let specs_dir = flow_dir.join(SPECS_DIR);
    let tasks_dir = flow_dir.join(TASKS_DIR);

    // Check collision (if ID changed)
    if new_id != old_id {
        let new_epic_path = epics_dir.join(format!("{new_id}.md"));
        if new_epic_path.exists() {
            error_exit(&format!(
                "Epic {new_id} already exists. Choose a different title."
            ));
        }
    }

    // Collect files to rename
    let mut renames: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut task_renames: Vec<(String, String)> = Vec::new();

    // Epic file
    renames.push((epic_path.clone(), epics_dir.join(format!("{new_id}.md"))));

    // Spec file
    let old_spec = specs_dir.join(format!("{old_id}.md"));
    if old_spec.exists() {
        renames.push((old_spec, specs_dir.join(format!("{new_id}.md"))));
    }

    // Task files
    if tasks_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&tasks_dir) {
            let mut task_entries: Vec<_> = entries
                .flatten()
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with(&format!("{old_id}."))
                })
                .collect();
            task_entries.sort_by_key(|e| e.file_name());

            for entry in task_entries {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                let path = entry.path();

                if name_str.ends_with(".md") {
                    let stem = name_str.trim_end_matches(".md");
                    if is_task_id(stem) {
                        if let Ok(p) = parse_id(stem) {
                            if let Some(task_num) = p.task {
                                let new_task_id = format!("{new_id}.{task_num}");
                                let new_path = tasks_dir.join(format!("{new_task_id}.md"));
                                renames.push((path, new_path));
                                // Track for content updates (avoid duplicates)
                                let old_task_id = stem.to_string();
                                if !task_renames.iter().any(|(o, _)| *o == old_task_id) {
                                    task_renames
                                        .push((old_task_id, new_task_id));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Checkpoint file
    let old_checkpoint = flow_dir.join(format!(".checkpoint-{old_id}.json"));
    if old_checkpoint.exists() {
        renames.push((
            old_checkpoint,
            flow_dir.join(format!(".checkpoint-{new_id}.json")),
        ));
    }

    // Perform renames
    let mut rename_errors: Vec<String> = Vec::new();
    for (old_path, new_path) in &renames {
        if let Err(e) = fs::rename(old_path, new_path) {
            rename_errors.push(format!(
                "{} -> {}: {e}",
                old_path.file_name().unwrap_or_default().to_string_lossy(),
                new_path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
    }

    if !rename_errors.is_empty() {
        error_exit(&format!(
            "Failed to rename some files: {}",
            rename_errors.join("; ")
        ));
    }

    // Update epic content
    let mut new_doc = doc;
    new_doc.frontmatter.id = new_id.clone();
    new_doc.frontmatter.title = new_title.to_string();
    new_doc.frontmatter.file_path = Some(format!("epics/{new_id}.md"));
    new_doc.frontmatter.updated_at = Utc::now();
    let new_epic_path = epics_dir.join(format!("{new_id}.md"));
    save_epic(&new_epic_path, &new_doc);
    db_upsert_epic(&new_doc.frontmatter);

    // Update task content
    let task_id_map: std::collections::HashMap<&str, &str> = task_renames
        .iter()
        .map(|(o, n)| (o.as_str(), n.as_str()))
        .collect();
    for (_old_task_id, new_task_id) in &task_renames {
        let task_path = tasks_dir.join(format!("{new_task_id}.md"));
        if task_path.exists() {
            if let Ok(content) = fs::read_to_string(&task_path) {
                if let Ok(mut task_doc) = frontmatter::parse::<Task>(&content) {
                    task_doc.frontmatter.id = new_task_id.clone();
                    task_doc.frontmatter.epic = new_id.clone();
                    task_doc.frontmatter.file_path =
                        Some(format!("tasks/{new_task_id}.md"));
                    // Update depends_on references within same epic
                    task_doc.frontmatter.depends_on = task_doc
                        .frontmatter
                        .depends_on
                        .iter()
                        .map(|dep| {
                            task_id_map
                                .get(dep.as_str())
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| dep.clone())
                        })
                        .collect();
                    task_doc.frontmatter.updated_at = Utc::now();
                    if let Ok(serialized) = frontmatter::write(&task_doc) {
                        let _ = fs::write(&task_path, serialized);
                    }
                    // SQLite update
                    if let Some(conn) = try_open_db() {
                        let repo = flowctl_db::TaskRepo::new(&conn);
                        let _ = repo.upsert(&task_doc.frontmatter);
                    }
                }
            }
        }
    }

    // Update depends_on_epics in other epics that reference old_id
    let mut updated_deps_in: Vec<String> = Vec::new();
    if epics_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&epics_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str == format!("{new_id}.md") {
                    continue;
                }
                if !name_str.ends_with(".md") {
                    continue;
                }
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(mut other_doc) = frontmatter::parse::<Epic>(&content) {
                        if other_doc.frontmatter.depends_on_epics.contains(&old_id.to_string()) {
                            other_doc.frontmatter.depends_on_epics = other_doc
                                .frontmatter
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
                            other_doc.frontmatter.updated_at = Utc::now();
                            if let Ok(serialized) = frontmatter::write(&other_doc) {
                                let _ = fs::write(&path, serialized);
                            }
                            updated_deps_in.push(other_doc.frontmatter.id.clone());
                        }
                    }
                }
            }
        }
    }

    let mut result = json!({
        "old_id": old_id,
        "new_id": new_id,
        "title": new_title,
        "files_renamed": renames.len(),
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
        println!("  Files renamed: {}", renames.len());
        println!("  Tasks updated: {}", task_renames.len());
        if !updated_deps_in.is_empty() {
            println!("  Updated deps in: {}", updated_deps_in.join(", "));
        }
    }
}

fn cmd_close(id: &str, skip_gap_check: bool, json_mode: bool) {
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{id}.md"));
    let mut doc = load_epic(&epic_path, id);

    // Check all tasks are done/skipped
    let tasks_dir = flow_dir.join(TASKS_DIR);
    if !tasks_dir.is_dir() {
        error_exit(&format!(
            "{TASKS_DIR}/ missing. Run 'flowctl init' or fix repo state."
        ));
    }

    let mut incomplete: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(&tasks_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with(&format!("{id}.")) || !name_str.ends_with(".md") {
                continue;
            }
            let stem = name_str.trim_end_matches(".md");
            if !is_task_id(stem) {
                continue;
            }
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(task) = frontmatter::parse_frontmatter::<Task>(&content) {
                    let status_str = task.status.to_string();
                    if status_str != "done" && status_str != "skipped" {
                        incomplete.push(format!("{} ({status_str})", task.id));
                    }
                }
            }
        }
    }

    if !incomplete.is_empty() {
        error_exit(&format!(
            "Cannot close epic: incomplete tasks - {}",
            incomplete.join(", ")
        ));
    }

    // Gap registry gate -- check raw frontmatter for gaps field
    let raw = load_epic_raw(&epic_path, id);
    let gaps = raw.get("gaps").and_then(|g| g.as_array());
    let mut open_blocking_count = 0;
    let mut gap_list_parts: Vec<String> = Vec::new();

    if let Some(gaps) = gaps {
        for gap in gaps {
            let status = gap.get("status").and_then(|s| s.as_str()).unwrap_or("");
            let priority = gap.get("priority").and_then(|s| s.as_str()).unwrap_or("");
            if status == "open" && GAP_BLOCKING_PRIORITIES.contains(&priority) {
                open_blocking_count += 1;
                let capability = gap
                    .get("capability")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                gap_list_parts.push(format!("[{priority}] {capability}"));
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
    save_epic(&epic_path, &doc);
    db_upsert_epic(&doc.frontmatter);

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

    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{id}.md"));

    if !epic_path.exists() {
        // Check archive
        let archive_path = flow_dir.join(ARCHIVE_DIR).join(id);
        if archive_path.exists() {
            error_exit(&format!(
                "Epic {id} is archived. Unarchive it first before reopening."
            ));
        }
        error_exit(&format!("Epic {id} not found"));
    }

    let mut doc = load_epic(&epic_path, id);
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
    save_epic(&epic_path, &doc);
    db_upsert_epic(&doc.frontmatter);

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

    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{id}.md"));
    let doc = load_epic(&epic_path, id);

    if doc.frontmatter.status != EpicStatus::Done && !force {
        error_exit(&format!(
            "Cannot archive epic {id}: status is '{}', not 'done'. \
             Close it first or use --force.",
            doc.frontmatter.status
        ));
    }

    // Build archive directory
    let archive_dir = flow_dir.join(ARCHIVE_DIR).join(id);
    fs::create_dir_all(&archive_dir)
        .unwrap_or_else(|e| error_exit(&format!("Failed to create archive dir: {e}")));

    let mut moved: Vec<String> = Vec::new();

    // Move epic file
    let dest = archive_dir.join(epic_path.file_name().unwrap());
    fs::rename(&epic_path, &dest)
        .unwrap_or_else(|e| error_exit(&format!("Failed to move epic file: {e}")));
    moved.push(format!("epics/{}", epic_path.file_name().unwrap().to_string_lossy()));

    // Move spec
    let spec_path = flow_dir.join(SPECS_DIR).join(format!("{id}.md"));
    if spec_path.exists() {
        let dest = archive_dir.join(spec_path.file_name().unwrap());
        let _ = fs::rename(&spec_path, &dest);
        moved.push(format!("specs/{}", spec_path.file_name().unwrap().to_string_lossy()));
    }

    // Move task files
    let tasks_dir = flow_dir.join(TASKS_DIR);
    if tasks_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&tasks_dir) {
            let mut task_entries: Vec<_> = entries.flatten().collect();
            task_entries.sort_by_key(|e| e.file_name());
            for entry in task_entries {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with(&format!("{id}.")) {
                    let dest = archive_dir.join(&*name);
                    let _ = fs::rename(entry.path(), &dest);
                    moved.push(format!("tasks/{name_str}"));
                }
            }
        }
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
                if name_str.contains(&format!("-{id}.")) {
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
    let epics_dir = flow_dir.join(EPICS_DIR);

    let mut archived: Vec<String> = Vec::new();

    if epics_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&epics_dir) {
            let mut epic_entries: Vec<_> = entries.flatten().collect();
            epic_entries.sort_by_key(|e| e.file_name());

            for entry in epic_entries {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if !name_str.ends_with(".md") {
                    continue;
                }
                let stem = name_str.trim_end_matches(".md");
                if !is_epic_id(stem) {
                    continue;
                }
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if let Ok(epic) = frontmatter::parse_frontmatter::<Epic>(&content) {
                        if epic.status == EpicStatus::Done {
                            let epic_id = epic.id.clone();
                            // Archive silently
                            cmd_archive_silent(&epic_id, &flow_dir);
                            archived.push(epic_id);
                        }
                    }
                }
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
fn cmd_archive_silent(id: &str, flow_dir: &Path) {
    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{id}.md"));
    if !epic_path.exists() {
        return;
    }

    let archive_dir = flow_dir.join(ARCHIVE_DIR).join(id);
    let _ = fs::create_dir_all(&archive_dir);

    // Move epic
    let _ = fs::rename(&epic_path, archive_dir.join(format!("{id}.md")));

    // Move spec
    let spec_path = flow_dir.join(SPECS_DIR).join(format!("{id}.md"));
    if spec_path.exists() {
        let _ = fs::rename(&spec_path, archive_dir.join(format!("{id}.md")));
    }

    // Move tasks
    let tasks_dir = flow_dir.join(TASKS_DIR);
    if tasks_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&tasks_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.to_string_lossy().starts_with(&format!("{id}.")) {
                    let _ = fs::rename(entry.path(), archive_dir.join(&name));
                }
            }
        }
    }

    // Move reviews
    let reviews_dir = flow_dir.join(REVIEWS_DIR);
    if reviews_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&reviews_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.to_string_lossy().contains(&format!("-{id}.")) {
                    let _ = fs::rename(entry.path(), archive_dir.join(&name));
                }
            }
        }
    }
}

fn cmd_add_dep(epic_id: &str, dep_id: &str, json_mode: bool) {
    let flow_dir = ensure_flow_exists();
    validate_epic_id(epic_id);
    validate_epic_id(dep_id);

    if epic_id == dep_id {
        error_exit("Epic cannot depend on itself");
    }

    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{epic_id}.md"));
    let dep_path = flow_dir.join(EPICS_DIR).join(format!("{dep_id}.md"));

    if !dep_path.exists() {
        error_exit(&format!("Epic {dep_id} not found"));
    }

    let mut doc = load_epic(&epic_path, epic_id);

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

    doc.frontmatter.depends_on_epics.push(dep_id.to_string());
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&epic_path, &doc);
    db_upsert_epic(&doc.frontmatter);

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
    let flow_dir = ensure_flow_exists();
    validate_epic_id(epic_id);

    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{epic_id}.md"));
    let mut doc = load_epic(&epic_path, epic_id);

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

    doc.frontmatter
        .depends_on_epics
        .retain(|d| d != dep_id);
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&epic_path, &doc);
    db_upsert_epic(&doc.frontmatter);

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
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    if impl_spec.is_none() && review.is_none() && sync.is_none() {
        error_exit("At least one of --impl, --review, or --sync must be provided");
    }

    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{id}.md"));
    let mut doc = load_epic(&epic_path, id);

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
    save_epic(&epic_path, &doc);
    db_upsert_epic(&doc.frontmatter);

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
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    if !pending && !done {
        error_exit("Either --pending or --done must be specified");
    }

    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{id}.md"));

    // For auto_execute fields, we work with raw frontmatter since Epic struct
    // doesn't have these fields. Read, patch, write back.
    if !epic_path.exists() {
        error_exit(&format!("Epic {id} not found"));
    }
    let content = fs::read_to_string(&epic_path)
        .unwrap_or_else(|e| error_exit(&format!("Failed to read epic: {e}")));

    let mut doc = frontmatter::parse::<serde_json::Value>(&content)
        .unwrap_or_else(|e| error_exit(&format!("Failed to parse epic {id}: {e}")));

    let action;
    if pending {
        doc.frontmatter["auto_execute_pending"] = json!(true);
        doc.frontmatter["auto_execute_set_at"] = json!(Utc::now().to_rfc3339());
        action = "pending";
    } else {
        doc.frontmatter["auto_execute_pending"] = json!(false);
        action = "done";
    }

    doc.frontmatter["updated_at"] = json!(Utc::now().to_rfc3339());

    let serialized = frontmatter::write(&doc)
        .unwrap_or_else(|e| error_exit(&format!("Failed to serialize epic: {e}")));
    fs::write(&epic_path, &serialized)
        .unwrap_or_else(|e| error_exit(&format!("Failed to write epic: {e}")));

    if json_mode {
        json_output(json!({
            "id": id,
            "auto_execute_pending": doc.frontmatter.get("auto_execute_pending"),
            "auto_execute_set_at": doc.frontmatter.get("auto_execute_set_at"),
            "message": format!("Epic {id} auto_execute set to {action}"),
        }));
    } else {
        println!("Epic {id} auto_execute set to {action}");
    }
}

// ── Dispatch ────────────────────────────────────────────────────────

pub fn dispatch(cmd: &EpicCmd, json: bool) {
    match cmd {
        EpicCmd::Create { title, branch } => cmd_create(title, branch, json),
        EpicCmd::SetPlan { id, file } => cmd_set_plan(id, file, json),
        EpicCmd::SetPlanReviewStatus { id, status } => {
            cmd_set_plan_review_status(id, status, json)
        }
        EpicCmd::SetCompletionReviewStatus { id, status } => {
            cmd_set_completion_review_status(id, status, json)
        }
        EpicCmd::SetBranch { id, branch } => cmd_set_branch(id, branch, json),
        EpicCmd::SetTitle { id, title } => cmd_set_title(id, title, json),
        EpicCmd::Close {
            id,
            skip_gap_check,
        } => cmd_close(id, *skip_gap_check, json),
        EpicCmd::Reopen { id } => cmd_reopen(id, json),
        EpicCmd::Archive { id, force } => cmd_archive(id, *force, json),
        EpicCmd::Clean => cmd_clean(json),
        EpicCmd::AddDep { epic, depends_on } => cmd_add_dep(epic, depends_on, json),
        EpicCmd::RmDep { epic, depends_on } => cmd_rm_dep(epic, depends_on, json),
        EpicCmd::SetBackend {
            id,
            impl_spec,
            review,
            sync,
        } => cmd_set_backend(id, impl_spec, review, sync, json),
        EpicCmd::SetAutoExecute {
            id,
            pending,
            done,
        } => cmd_set_auto_execute(id, *pending, *done, json),
    }
}
