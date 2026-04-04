//! Task management commands: create, skip, split, set-spec, set-description,
//! set-acceptance, set-deps, reset, set-backend, show-backend.

use std::env;
use std::fs;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

use chrono::Utc;
use clap::Subcommand;
use regex::Regex;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::frontmatter;
use flowctl_core::id::{epic_id_from_task, is_epic_id, is_task_id};
use flowctl_core::state_machine::Status;
use flowctl_core::types::{
    Domain, Epic, Task, EPICS_DIR, FLOW_DIR, TASKS_DIR,
};

#[derive(Subcommand, Debug)]
pub enum TaskCmd {
    /// Create a new task.
    Create {
        /// Epic ID.
        #[arg(long)]
        epic: String,
        /// Task title.
        #[arg(long)]
        title: String,
        /// Comma-separated dependency IDs.
        #[arg(long)]
        deps: Option<String>,
        /// Markdown file with acceptance criteria.
        #[arg(long)]
        acceptance_file: Option<String>,
        /// Priority (lower = earlier).
        #[arg(long)]
        priority: Option<i32>,
        /// Task domain.
        #[arg(long, value_parser = ["frontend", "backend", "architecture", "testing", "docs", "ops", "general"])]
        domain: Option<String>,
        /// Comma-separated owned file paths.
        #[arg(long)]
        files: Option<String>,
    },
    /// Set task description.
    SetDescription {
        /// Task ID.
        id: String,
        /// Markdown file (use '-' for stdin).
        #[arg(long)]
        file: String,
    },
    /// Set task acceptance criteria.
    SetAcceptance {
        /// Task ID.
        id: String,
        /// Markdown file (use '-' for stdin).
        #[arg(long)]
        file: String,
    },
    /// Set task spec (full file or sections).
    SetSpec {
        /// Task ID.
        id: String,
        /// Full spec file.
        #[arg(long)]
        file: Option<String>,
        /// Description section file.
        #[arg(long)]
        description: Option<String>,
        /// Acceptance section file.
        #[arg(long)]
        acceptance: Option<String>,
    },
    /// Reset task to todo.
    Reset {
        /// Task ID.
        task_id: String,
        /// Also reset dependent tasks.
        #[arg(long)]
        cascade: bool,
    },
    /// Skip task (mark as permanently skipped).
    Skip {
        /// Task ID.
        task_id: String,
        /// Why the task is being skipped.
        #[arg(long)]
        reason: Option<String>,
    },
    /// Split task into sub-tasks (runtime DAG mutation).
    Split {
        /// Task ID to split.
        task_id: String,
        /// Sub-task titles separated by '|'.
        #[arg(long)]
        titles: String,
        /// Chain sub-tasks sequentially.
        #[arg(long)]
        chain: bool,
    },
    /// Set backend specs for impl/review/sync.
    SetBackend {
        /// Task ID.
        id: String,
        /// Impl backend spec.
        #[arg(long = "impl")]
        impl_spec: Option<String>,
        /// Review backend spec.
        #[arg(long)]
        review: Option<String>,
        /// Sync backend spec.
        #[arg(long)]
        sync: Option<String>,
    },
    /// Show effective backend specs.
    ShowBackend {
        /// Task ID.
        id: String,
    },
    /// Set task dependencies (comma-separated).
    SetDeps {
        /// Task ID.
        task_id: String,
        /// Comma-separated dependency IDs.
        #[arg(long)]
        deps: String,
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

/// Try to open a DB connection.
fn try_open_db() -> Option<rusqlite::Connection> {
    let cwd = env::current_dir().ok()?;
    flowctl_db::open(&cwd).ok()
}

/// Read file content, or read from stdin if path is "-".
fn read_file_or_stdin(path: &str) -> String {
    if path == "-" {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .unwrap_or_else(|e| error_exit(&format!("Failed to read stdin: {e}")));
        buf
    } else {
        fs::read_to_string(path)
            .unwrap_or_else(|e| error_exit(&format!("Failed to read file '{}': {e}", path)))
    }
}

/// Scan .flow/tasks/ to find max task number for an epic. Returns 0 if none exist.
fn scan_max_task_id(flow_dir: &Path, epic_id: &str) -> u32 {
    let tasks_dir = flow_dir.join(TASKS_DIR);
    if !tasks_dir.exists() {
        return 0;
    }

    let pattern = format!(r"^{}\.(\d+)\.md$", regex::escape(epic_id));
    let re = Regex::new(&pattern).expect("task ID regex is valid");

    let mut max_n: u32 = 0;
    if let Ok(entries) = fs::read_dir(&tasks_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if let Some(caps) = re.captures(&name_str) {
                if let Ok(n) = caps[1].parse::<u32>() {
                    max_n = max_n.max(n);
                }
            }
        }
    }
    max_n
}

/// Parse a domain string into a Domain enum.
fn parse_domain(s: &str) -> Domain {
    match s {
        "frontend" => Domain::Frontend,
        "backend" => Domain::Backend,
        "architecture" => Domain::Architecture,
        "testing" => Domain::Testing,
        "docs" => Domain::Docs,
        "ops" => Domain::Ops,
        _ => Domain::General,
    }
}

/// Create task spec markdown content.
fn create_task_spec(id: &str, title: &str, acceptance: Option<&str>) -> String {
    let acceptance_content = acceptance.unwrap_or("- [ ] TBD");
    format!(
        "# {} {}\n\n## Description\nTBD\n\n## Acceptance\n{}\n\n## Done summary\nTBD\n\n## Evidence\n- Commits:\n- Tests:\n- PRs:\n",
        id, title, acceptance_content
    )
}

/// Load a task from its Markdown frontmatter file.
fn load_task_md(flow_dir: &Path, task_id: &str) -> Task {
    let spec_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", task_id));
    if !spec_path.exists() {
        error_exit(&format!("Task {} not found", task_id));
    }
    let content = fs::read_to_string(&spec_path)
        .unwrap_or_else(|e| error_exit(&format!("Failed to read task {}: {e}", task_id)));
    let doc: frontmatter::Document<Task> = frontmatter::parse(&content)
        .unwrap_or_else(|e| error_exit(&format!("Failed to parse task {}: {e}", task_id)));
    doc.frontmatter
}

/// Load an epic from its Markdown frontmatter file.
fn load_epic_md(flow_dir: &Path, epic_id: &str) -> Option<Epic> {
    let spec_path = flow_dir.join(EPICS_DIR).join(format!("{}.md", epic_id));
    if !spec_path.exists() {
        return None;
    }
    let content = fs::read_to_string(&spec_path).ok()?;
    let doc: frontmatter::Document<Epic> = frontmatter::parse(&content).ok()?;
    Some(doc.frontmatter)
}

/// Load task's full Markdown document (frontmatter + body).
fn load_task_doc(flow_dir: &Path, task_id: &str) -> frontmatter::Document<Task> {
    let spec_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", task_id));
    if !spec_path.exists() {
        error_exit(&format!("Task {} not found", task_id));
    }
    let content = fs::read_to_string(&spec_path)
        .unwrap_or_else(|e| error_exit(&format!("Failed to read task {}: {e}", task_id)));
    frontmatter::parse(&content)
        .unwrap_or_else(|e| error_exit(&format!("Failed to parse task {}: {e}", task_id)))
}

/// Write a task document (frontmatter + body) to disk.
fn write_task_doc(flow_dir: &Path, task_id: &str, doc: &frontmatter::Document<Task>) {
    let spec_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", task_id));
    let content = frontmatter::write(doc)
        .unwrap_or_else(|e| error_exit(&format!("Failed to serialize task {}: {e}", task_id)));
    fs::write(&spec_path, content)
        .unwrap_or_else(|e| error_exit(&format!("Failed to write task {}: {e}", task_id)));
}

/// Patch a specific section in a Markdown body. Replaces content under `section`
/// heading (e.g. "## Description") until the next "## " heading.
fn patch_body_section(body: &str, section: &str, new_content: &str) -> String {
    // Strip leading section heading from new_content if present
    let trimmed_new = {
        let lines: Vec<&str> = new_content.trim_start().lines().collect();
        if !lines.is_empty() && lines[0].trim() == section {
            lines[1..].join("\n").trim_start().to_string()
        } else {
            new_content.to_string()
        }
    };

    let lines: Vec<&str> = body.lines().collect();
    let mut result = Vec::new();
    let mut in_target = false;
    let mut section_found = false;

    for line in &lines {
        if line.starts_with("## ") {
            if line.trim() == section {
                in_target = true;
                section_found = true;
                result.push(line.to_string());
                result.push(trimmed_new.trim_end().to_string());
                continue;
            } else {
                in_target = false;
            }
        }

        if !in_target {
            result.push(line.to_string());
        }
    }

    if !section_found {
        // Auto-append missing section
        result.push(String::new());
        result.push(section.to_string());
        result.push(trimmed_new.trim_end().to_string());
    }

    result.join("\n")
}

/// Find tasks that depend on a given task (recursive BFS within same epic).
fn find_dependents(flow_dir: &Path, task_id: &str) -> Vec<String> {
    let tasks_dir = flow_dir.join(TASKS_DIR);
    if !tasks_dir.exists() {
        return vec![];
    }

    let epic_id = match epic_id_from_task(task_id) {
        Ok(id) => id,
        Err(_) => return vec![],
    };

    // Load all tasks in the epic
    let mut all_tasks: Vec<(String, Vec<String>)> = Vec::new();
    if let Ok(entries) = fs::read_dir(&tasks_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with(&epic_id) || !name_str.ends_with(".md") {
                continue;
            }
            let tid = name_str.trim_end_matches(".md").to_string();
            if !is_task_id(&tid) {
                continue;
            }
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(doc) = frontmatter::parse::<Task>(&content) {
                    all_tasks.push((doc.frontmatter.id.clone(), doc.frontmatter.depends_on.clone()));
                }
            }
        }
    }

    // BFS
    let mut dependents: Vec<String> = Vec::new();
    let mut to_check = vec![task_id.to_string()];
    let mut checked = std::collections::HashSet::new();

    while let Some(checking) = to_check.pop() {
        if !checked.insert(checking.clone()) {
            continue;
        }
        for (tid, deps) in &all_tasks {
            if checked.contains(tid) || dependents.contains(tid) {
                continue;
            }
            if deps.contains(&checking) {
                dependents.push(tid.clone());
                to_check.push(tid.clone());
            }
        }
    }

    dependents.sort();
    dependents
}

/// Clear ## Evidence section in spec body back to template.
fn clear_evidence_in_body(body: &str) -> String {
    let re = Regex::new(r"(?s)(## Evidence\s*\n).*?(\n## |\z)").expect("evidence regex valid");
    let replacement = "${1}- Commits:\n- Tests:\n- PRs:\n${2}";
    re.replace(body, replacement).to_string()
}

// ── Dispatch ────────────────────────────────────────────────────────

pub fn dispatch(cmd: &TaskCmd, json: bool) {
    match cmd {
        TaskCmd::Create {
            epic,
            title,
            deps,
            acceptance_file,
            priority,
            domain,
            files,
        } => cmd_task_create(
            json,
            epic,
            title,
            deps.as_deref(),
            acceptance_file.as_deref(),
            *priority,
            domain.as_deref(),
            files.as_deref(),
        ),
        TaskCmd::SetDescription { id, file } => cmd_task_set_section(json, id, "## Description", file),
        TaskCmd::SetAcceptance { id, file } => cmd_task_set_section(json, id, "## Acceptance", file),
        TaskCmd::SetSpec {
            id,
            file,
            description,
            acceptance,
        } => cmd_task_set_spec(json, id, file.as_deref(), description.as_deref(), acceptance.as_deref()),
        TaskCmd::Reset { task_id, cascade } => cmd_task_reset(json, task_id, *cascade),
        TaskCmd::Skip { task_id, reason } => cmd_task_skip(json, task_id, reason.as_deref()),
        TaskCmd::Split {
            task_id,
            titles,
            chain,
        } => cmd_task_split(json, task_id, titles, *chain),
        TaskCmd::SetBackend {
            id,
            impl_spec,
            review,
            sync,
        } => cmd_task_set_backend(json, id, impl_spec.as_deref(), review.as_deref(), sync.as_deref()),
        TaskCmd::ShowBackend { id } => cmd_task_show_backend(json, id),
        TaskCmd::SetDeps { task_id, deps } => cmd_task_set_deps(json, task_id, deps),
    }
}

// ── Command implementations ─────────────────────────────────────────

fn cmd_task_create(
    json_mode: bool,
    epic_id: &str,
    title: &str,
    deps: Option<&str>,
    acceptance_file: Option<&str>,
    priority: Option<i32>,
    domain: Option<&str>,
    files: Option<&str>,
) {
    let flow_dir = ensure_flow_exists();

    if !is_epic_id(epic_id) {
        error_exit(&format!(
            "Invalid epic ID: {}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)",
            epic_id
        ));
    }

    // Verify epic exists
    let epic_spec = flow_dir.join(EPICS_DIR).join(format!("{}.md", epic_id));
    if !epic_spec.exists() {
        error_exit(&format!("Epic {} not found", epic_id));
    }

    // Scan-based ID allocation
    let task_num = scan_max_task_id(&flow_dir, epic_id) + 1;
    let task_id = format!("{}.{}", epic_id, task_num);

    // Check no collision
    let spec_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", task_id));
    if spec_path.exists() {
        error_exit(&format!(
            "Refusing to overwrite existing task {}. Check for orphaned files.",
            task_id
        ));
    }

    // Parse dependencies
    let dep_list: Vec<String> = match deps {
        Some(d) if !d.is_empty() => d
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => vec![],
    };

    // Validate deps
    for dep in &dep_list {
        if !is_task_id(dep) {
            error_exit(&format!(
                "Invalid dependency ID: {}. Expected format: fn-N.M or fn-N-slug.M",
                dep
            ));
        }
        if let Ok(dep_epic) = epic_id_from_task(dep) {
            if dep_epic != epic_id {
                error_exit(&format!(
                    "Dependency {} must be within the same epic ({})",
                    dep, epic_id
                ));
            }
        }
    }

    // Read acceptance from file if provided
    let acceptance = acceptance_file.map(|f| read_file_or_stdin(f));

    // Parse files
    let file_list: Vec<String> = match files {
        Some(f) if !f.is_empty() => f
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => vec![],
    };

    let domain_enum = domain.map(|d| parse_domain(d)).unwrap_or(Domain::General);
    let now = Utc::now();

    // Create Task struct
    let task = Task {
        schema_version: 1,
        id: task_id.clone(),
        epic: epic_id.to_string(),
        title: title.to_string(),
        status: Status::Todo,
        priority: priority.map(|p| p as u32),
        domain: domain_enum,
        depends_on: dep_list.clone(),
        files: file_list,
        r#impl: None,
        review: None,
        sync: None,
        file_path: Some(format!("{}/{}/{}.md", FLOW_DIR, TASKS_DIR, task_id)),
        created_at: now,
        updated_at: now,
    };

    // Create spec markdown body
    let body = create_task_spec(&task_id, title, acceptance.as_deref());

    // Write Markdown file with frontmatter
    let doc = frontmatter::Document {
        frontmatter: task.clone(),
        body,
    };
    write_task_doc(&flow_dir, &task_id, &doc);

    // Upsert into SQLite if DB available
    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::TaskRepo::new(&conn);
        let _ = repo.upsert(&task);
    }

    let spec_path_str = format!("{}/{}/{}.md", FLOW_DIR, TASKS_DIR, task_id);
    if json_mode {
        json_output(json!({
            "id": task_id,
            "epic": epic_id,
            "title": title,
            "depends_on": dep_list,
            "spec_path": spec_path_str,
            "message": format!("Task {} created", task_id),
        }));
    } else {
        println!("Task {} created: {}", task_id, title);
    }
}

fn cmd_task_set_section(json_mode: bool, task_id: &str, section: &str, file_path: &str) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    let mut doc = load_task_doc(&flow_dir, task_id);

    // Read new content
    let new_content = read_file_or_stdin(file_path);

    // Patch body section
    doc.body = patch_body_section(&doc.body, section, &new_content);
    doc.frontmatter.updated_at = Utc::now();

    write_task_doc(&flow_dir, task_id, &doc);

    // Update DB
    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::TaskRepo::new(&conn);
        let _ = repo.upsert(&doc.frontmatter);
    }

    if json_mode {
        json_output(json!({
            "id": task_id,
            "section": section,
            "message": format!("Task {} {} updated", task_id, section),
        }));
    } else {
        println!("Task {} {} updated", task_id, section);
    }
}

fn cmd_task_set_spec(
    json_mode: bool,
    task_id: &str,
    file: Option<&str>,
    description: Option<&str>,
    acceptance: Option<&str>,
) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    if file.is_none() && description.is_none() && acceptance.is_none() {
        error_exit("Requires --file, --description, or --acceptance");
    }

    let mut doc = load_task_doc(&flow_dir, task_id);

    if let Some(f) = file {
        // Full file replacement mode
        let content = read_file_or_stdin(f);
        doc.body = content;
        doc.frontmatter.updated_at = Utc::now();
        write_task_doc(&flow_dir, task_id, &doc);

        if let Some(conn) = try_open_db() {
            let repo = flowctl_db::TaskRepo::new(&conn);
            let _ = repo.upsert(&doc.frontmatter);
        }

        if json_mode {
            json_output(json!({
                "id": task_id,
                "message": format!("Task {} spec replaced", task_id),
            }));
        } else {
            println!("Task {} spec replaced", task_id);
        }
        return;
    }

    // Section patch mode
    let mut sections_updated = Vec::new();

    if let Some(desc_file) = description {
        let desc_content = read_file_or_stdin(desc_file);
        doc.body = patch_body_section(&doc.body, "## Description", &desc_content);
        sections_updated.push("## Description");
    }

    if let Some(acc_file) = acceptance {
        let acc_content = read_file_or_stdin(acc_file);
        doc.body = patch_body_section(&doc.body, "## Acceptance", &acc_content);
        sections_updated.push("## Acceptance");
    }

    doc.frontmatter.updated_at = Utc::now();
    write_task_doc(&flow_dir, task_id, &doc);

    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::TaskRepo::new(&conn);
        let _ = repo.upsert(&doc.frontmatter);
    }

    if json_mode {
        json_output(json!({
            "id": task_id,
            "sections": sections_updated,
            "message": format!("Task {} updated: {}", task_id, sections_updated.join(", ")),
        }));
    } else {
        println!("Task {} updated: {}", task_id, sections_updated.join(", "));
    }
}

fn cmd_task_reset(json_mode: bool, task_id: &str, cascade: bool) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    let mut doc = load_task_doc(&flow_dir, task_id);
    let current_status = doc.frontmatter.status;

    // Check if epic is closed
    if let Ok(eid) = epic_id_from_task(task_id) {
        if let Some(epic) = load_epic_md(&flow_dir, &eid) {
            if epic.status == flowctl_core::types::EpicStatus::Done {
                error_exit(&format!("Cannot reset task in closed epic {}", eid));
            }
        }
    }

    if current_status == Status::InProgress {
        error_exit(&format!(
            "Cannot reset in_progress task {}. Complete or block it first.",
            task_id
        ));
    }

    if current_status == Status::Todo {
        if json_mode {
            json_output(json!({
                "reset": [],
                "message": format!("{} already todo", task_id),
            }));
        } else {
            println!("{} already todo", task_id);
        }
        return;
    }

    // Reset the task
    doc.frontmatter.status = Status::Todo;
    doc.frontmatter.updated_at = Utc::now();
    doc.body = clear_evidence_in_body(&doc.body);
    write_task_doc(&flow_dir, task_id, &doc);

    // Update DB
    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::TaskRepo::new(&conn);
        let _ = repo.update_status(task_id, Status::Todo);
        // Clear runtime state by upserting a blank state
        let runtime_repo = flowctl_db::RuntimeRepo::new(&conn);
        let blank = flowctl_core::types::RuntimeState {
            task_id: task_id.to_string(),
            ..Default::default()
        };
        let _ = runtime_repo.upsert(&blank);
    }

    let mut reset_ids = vec![task_id.to_string()];

    // Handle cascade
    if cascade {
        let dependents = find_dependents(&flow_dir, task_id);
        for dep_id in &dependents {
            if let Ok(dep_doc_result) = (|| -> Result<frontmatter::Document<Task>, ()> {
                let p = flow_dir.join(TASKS_DIR).join(format!("{}.md", dep_id));
                let content = fs::read_to_string(&p).map_err(|_| ())?;
                frontmatter::parse::<Task>(&content).map_err(|_| ())
            })() {
                let mut dep_doc = dep_doc_result;
                let dep_status = dep_doc.frontmatter.status;
                if dep_status == Status::InProgress || dep_status == Status::Todo {
                    continue;
                }

                dep_doc.frontmatter.status = Status::Todo;
                dep_doc.frontmatter.updated_at = Utc::now();
                dep_doc.body = clear_evidence_in_body(&dep_doc.body);
                write_task_doc(&flow_dir, dep_id, &dep_doc);

                if let Some(conn) = try_open_db() {
                    let repo = flowctl_db::TaskRepo::new(&conn);
                    let _ = repo.update_status(dep_id, Status::Todo);
                    let runtime_repo = flowctl_db::RuntimeRepo::new(&conn);
                    let blank = flowctl_core::types::RuntimeState {
                        task_id: dep_id.to_string(),
                        ..Default::default()
                    };
                    let _ = runtime_repo.upsert(&blank);
                }
                reset_ids.push(dep_id.clone());
            }
        }
    }

    if json_mode {
        json_output(json!({
            "reset": reset_ids,
        }));
    } else {
        println!("Reset: {}", reset_ids.join(", "));
    }
}

fn cmd_task_skip(json_mode: bool, task_id: &str, reason: Option<&str>) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!("Invalid task ID: {}", task_id));
    }

    let mut doc = load_task_doc(&flow_dir, task_id);

    if doc.frontmatter.status == Status::Done {
        error_exit(&format!("Cannot skip already-done task {}", task_id));
    }

    doc.frontmatter.status = Status::Skipped;
    doc.frontmatter.updated_at = Utc::now();
    write_task_doc(&flow_dir, task_id, &doc);

    // Update DB
    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::TaskRepo::new(&conn);
        let _ = repo.update_status(task_id, Status::Skipped);
    }

    let reason_str = reason.unwrap_or("");
    if json_mode {
        json_output(json!({
            "id": task_id,
            "status": "skipped",
            "reason": reason_str,
            "message": format!("Task {} skipped", task_id),
        }));
    } else {
        let suffix = if !reason_str.is_empty() {
            format!(": {}", reason_str)
        } else {
            String::new()
        };
        println!("Task {} skipped{}", task_id, suffix);
    }
}

fn cmd_task_split(json_mode: bool, task_id: &str, titles: &str, chain: bool) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!("Invalid task ID: {}", task_id));
    }

    let doc = load_task_doc(&flow_dir, task_id);
    let status = doc.frontmatter.status;

    if status == Status::Done || status == Status::Skipped {
        error_exit(&format!(
            "Cannot split task {} with status '{}'",
            task_id, status
        ));
    }

    let epic_id = epic_id_from_task(task_id)
        .unwrap_or_else(|_| error_exit(&format!("Cannot extract epic from {}", task_id)));

    let title_list: Vec<String> = titles
        .split('|')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();

    if title_list.len() < 2 {
        error_exit("Need at least 2 sub-task titles separated by '|'");
    }

    let max_task = scan_max_task_id(&flow_dir, &epic_id);
    let original_deps = doc.frontmatter.depends_on.clone();
    let mut created: Vec<String> = Vec::new();
    let now = Utc::now();

    for (i, sub_title) in title_list.iter().enumerate() {
        let sub_num = max_task + 1 + i as u32;
        let sub_id = format!("{}.{}", epic_id, sub_num);

        // First sub-task inherits original deps; subsequent depend on previous if chained
        let sub_deps = if i == 0 {
            original_deps.clone()
        } else if chain {
            let prev_id = format!("{}.{}", epic_id, max_task + i as u32);
            vec![prev_id]
        } else {
            vec![]
        };

        let sub_task = Task {
            schema_version: 1,
            id: sub_id.clone(),
            epic: epic_id.clone(),
            title: sub_title.clone(),
            status: Status::Todo,
            priority: doc.frontmatter.priority,
            domain: doc.frontmatter.domain,
            depends_on: sub_deps,
            files: vec![],
            r#impl: None,
            review: None,
            sync: None,
            file_path: Some(format!("{}/{}/{}.md", FLOW_DIR, TASKS_DIR, sub_id)),
            created_at: now,
            updated_at: now,
        };

        let body = create_task_spec(&sub_id, sub_title, None);
        let sub_doc = frontmatter::Document {
            frontmatter: sub_task.clone(),
            body,
        };
        write_task_doc(&flow_dir, &sub_id, &sub_doc);

        if let Some(conn) = try_open_db() {
            let repo = flowctl_db::TaskRepo::new(&conn);
            let _ = repo.upsert(&sub_task);
        }

        created.push(sub_id);
    }

    // Mark original task as skipped
    let mut orig_doc = doc;
    orig_doc.frontmatter.status = Status::Skipped;
    orig_doc.frontmatter.updated_at = now;
    write_task_doc(&flow_dir, task_id, &orig_doc);

    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::TaskRepo::new(&conn);
        let _ = repo.update_status(task_id, Status::Skipped);
    }

    // Update tasks that depended on original to depend on last sub-task
    let last_sub = created.last().unwrap().clone();
    let tasks_dir = flow_dir.join(TASKS_DIR);
    if let Ok(entries) = fs::read_dir(&tasks_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with(&epic_id) || !name_str.ends_with(".md") {
                continue;
            }
            let other_id = name_str.trim_end_matches(".md").to_string();
            if other_id == task_id || created.contains(&other_id) {
                continue;
            }
            if !is_task_id(&other_id) {
                continue;
            }
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(mut other_doc) = frontmatter::parse::<Task>(&content) {
                    if other_doc.frontmatter.depends_on.contains(&task_id.to_string()) {
                        other_doc.frontmatter.depends_on = other_doc
                            .frontmatter
                            .depends_on
                            .iter()
                            .map(|d| {
                                if d == task_id {
                                    last_sub.clone()
                                } else {
                                    d.clone()
                                }
                            })
                            .collect();
                        other_doc.frontmatter.updated_at = now;

                        // Re-read full doc to preserve body
                        if let Ok(full_doc) = frontmatter::parse::<Task>(&content) {
                            let updated_doc = frontmatter::Document {
                                frontmatter: other_doc.frontmatter,
                                body: full_doc.body,
                            };
                            write_task_doc(&flow_dir, &updated_doc.frontmatter.id, &updated_doc);
                        }
                    }
                }
            }
        }
    }

    if json_mode {
        json_output(json!({
            "original": task_id,
            "split_into": created,
            "chain": chain,
            "message": format!("Task {} split into {} sub-tasks", task_id, created.len()),
        }));
    } else {
        println!("Task {} split into:", task_id);
        for sub_id in &created {
            println!("  {}", sub_id);
        }
        println!(
            "Original task marked as skipped. Downstream deps updated to {}.",
            last_sub
        );
    }
}

fn cmd_task_set_backend(
    json_mode: bool,
    task_id: &str,
    impl_spec: Option<&str>,
    review: Option<&str>,
    sync: Option<&str>,
) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    if impl_spec.is_none() && review.is_none() && sync.is_none() {
        error_exit("At least one of --impl, --review, or --sync must be provided");
    }

    let mut doc = load_task_doc(&flow_dir, task_id);
    let mut updated = Vec::new();

    if let Some(v) = impl_spec {
        let val = if v.is_empty() { None } else { Some(v.to_string()) };
        doc.frontmatter.r#impl = val;
        updated.push(format!("impl={}", if v.is_empty() { "null" } else { v }));
    }
    if let Some(v) = review {
        let val = if v.is_empty() { None } else { Some(v.to_string()) };
        doc.frontmatter.review = val;
        updated.push(format!("review={}", if v.is_empty() { "null" } else { v }));
    }
    if let Some(v) = sync {
        let val = if v.is_empty() { None } else { Some(v.to_string()) };
        doc.frontmatter.sync = val;
        updated.push(format!("sync={}", if v.is_empty() { "null" } else { v }));
    }

    doc.frontmatter.updated_at = Utc::now();
    write_task_doc(&flow_dir, task_id, &doc);

    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::TaskRepo::new(&conn);
        let _ = repo.upsert(&doc.frontmatter);
    }

    let msg = format!("Task {} backend specs updated: {}", task_id, updated.join(", "));
    if json_mode {
        json_output(json!({
            "id": task_id,
            "impl": doc.frontmatter.r#impl,
            "review": doc.frontmatter.review,
            "sync": doc.frontmatter.sync,
            "message": msg,
        }));
    } else {
        println!("{}", msg);
    }
}

fn cmd_task_show_backend(json_mode: bool, task_id: &str) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    let task = load_task_md(&flow_dir, task_id);
    let epic_id_str = &task.epic;
    let epic = load_epic_md(&flow_dir, epic_id_str);

    // Resolve effective specs with source tracking
    let resolve = |task_val: &Option<String>, epic_key: &str| -> (serde_json::Value, serde_json::Value) {
        if let Some(v) = task_val {
            if !v.is_empty() {
                return (json!(v), json!("task"));
            }
        }
        if let Some(ref e) = epic {
            let epic_val = match epic_key {
                "default_impl" => &e.default_impl,
                "default_review" => &e.default_review,
                "default_sync" => &e.default_sync,
                _ => &None,
            };
            if let Some(v) = epic_val {
                if !v.is_empty() {
                    return (json!(v), json!("epic"));
                }
            }
        }
        (json!(null), json!(null))
    };

    let (impl_spec, impl_source) = resolve(&task.r#impl, "default_impl");
    let (review_spec, review_source) = resolve(&task.review, "default_review");
    let (sync_spec, sync_source) = resolve(&task.sync, "default_sync");

    if json_mode {
        json_output(json!({
            "id": task_id,
            "epic": epic_id_str,
            "impl": {"spec": impl_spec, "source": impl_source},
            "review": {"spec": review_spec, "source": review_source},
            "sync": {"spec": sync_spec, "source": sync_source},
        }));
    } else {
        let fmt = |spec: &serde_json::Value, source: &serde_json::Value| -> String {
            if spec.is_null() {
                "null".to_string()
            } else {
                format!("{} ({})", spec.as_str().unwrap_or("null"), source.as_str().unwrap_or(""))
            }
        };
        println!("impl: {}", fmt(&impl_spec, &impl_source));
        println!("review: {}", fmt(&review_spec, &review_source));
        println!("sync: {}", fmt(&sync_spec, &sync_source));
    }
}

fn cmd_task_set_deps(json_mode: bool, task_id: &str, deps: &str) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    let dep_ids: Vec<String> = deps
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if dep_ids.is_empty() {
        error_exit("--deps cannot be empty");
    }

    let task_epic = epic_id_from_task(task_id)
        .unwrap_or_else(|_| error_exit(&format!("Invalid task ID: {}", task_id)));

    // Validate all dep IDs
    for dep_id in &dep_ids {
        if !is_task_id(dep_id) {
            error_exit(&format!(
                "Invalid dependency ID: {}. Expected format: fn-N.M or fn-N-slug.M",
                dep_id
            ));
        }
        if let Ok(dep_epic) = epic_id_from_task(dep_id) {
            if dep_epic != task_epic {
                error_exit(&format!(
                    "Dependencies must be within same epic. Task {} is in {}, dependency {} is in {}",
                    task_id, task_epic, dep_id, dep_epic
                ));
            }
        }
    }

    let mut doc = load_task_doc(&flow_dir, task_id);

    let mut added = Vec::new();
    for dep_id in &dep_ids {
        if !doc.frontmatter.depends_on.contains(dep_id) {
            doc.frontmatter.depends_on.push(dep_id.clone());
            added.push(dep_id.clone());
        }
    }

    if !added.is_empty() {
        doc.frontmatter.updated_at = Utc::now();
        write_task_doc(&flow_dir, task_id, &doc);

        if let Some(conn) = try_open_db() {
            let repo = flowctl_db::TaskRepo::new(&conn);
            let _ = repo.upsert(&doc.frontmatter);
        }
    }

    if json_mode {
        json_output(json!({
            "task": task_id,
            "depends_on": doc.frontmatter.depends_on,
            "added": added,
            "message": format!("Dependencies set for {}", task_id),
        }));
    } else if !added.is_empty() {
        println!("Added dependencies to {}: {}", task_id, added.join(", "));
    } else {
        println!("No new dependencies added (already set)");
    }
}
