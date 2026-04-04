//! Task management commands: create, skip, split, set-spec, set-description,
//! set-acceptance, set-deps, reset, set-backend, show-backend.

mod create;
mod mutate;
mod query;

use std::env;
use std::fs;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

use clap::Subcommand;
use regex::Regex;

use crate::output::error_exit;

use flowctl_core::frontmatter;
use flowctl_core::id::{epic_id_from_task, is_task_id};
use flowctl_core::types::{
    Domain, Epic, Task, EPICS_DIR, TASKS_DIR,
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
    /// Set task spec: full file or individual sections.
    Spec {
        /// Task ID.
        id: String,
        /// Full spec file.
        #[arg(long)]
        file: Option<String>,
        /// Description section file.
        #[arg(long, alias = "description")]
        desc: Option<String>,
        /// Acceptance section file.
        #[arg(long, alias = "acceptance")]
        accept: Option<String>,
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

use crate::commands::helpers::get_flow_dir;

// ── Helpers ─────────────────────────────────────────────────────────

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

/// Load a task: DB first, markdown fallback.
fn load_task_md(_flow_dir: &Path, task_id: &str) -> Task {
    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::TaskRepo::new(&conn);
        if let Ok(task) = repo.get(task_id) {
            return task;
        }
    }
    // Fallback to markdown.
    let flow_dir = _flow_dir;
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

/// Load an epic: DB first, markdown fallback.
fn load_epic_md(_flow_dir: &Path, epic_id: &str) -> Option<Epic> {
    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::EpicRepo::new(&conn);
        if let Ok(epic) = repo.get(epic_id) {
            return Some(epic);
        }
    }
    let flow_dir = _flow_dir;
    let spec_path = flow_dir.join(EPICS_DIR).join(format!("{}.md", epic_id));
    if !spec_path.exists() {
        return None;
    }
    let content = fs::read_to_string(&spec_path).ok()?;
    let doc: frontmatter::Document<Epic> = frontmatter::parse(&content).ok()?;
    Some(doc.frontmatter)
}

/// Load task's full document (frontmatter + body): DB first, markdown fallback.
fn load_task_doc(flow_dir: &Path, task_id: &str) -> frontmatter::Document<Task> {
    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::TaskRepo::new(&conn);
        if let Ok((task, body)) = repo.get_with_body(task_id) {
            return frontmatter::Document {
                frontmatter: task,
                body,
            };
        }
    }
    // Fallback to markdown.
    let spec_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", task_id));
    if !spec_path.exists() {
        error_exit(&format!("Task {} not found", task_id));
    }
    let content = fs::read_to_string(&spec_path)
        .unwrap_or_else(|e| error_exit(&format!("Failed to read task {}: {e}", task_id)));
    frontmatter::parse(&content)
        .unwrap_or_else(|e| error_exit(&format!("Failed to parse task {}: {e}", task_id)))
}

/// Write a task document: DB first, then export markdown.
fn write_task_doc(flow_dir: &Path, task_id: &str, doc: &frontmatter::Document<Task>) {
    // Write to DB.
    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::TaskRepo::new(&conn);
        if let Err(e) = repo.upsert_with_body(&doc.frontmatter, &doc.body) {
            eprintln!("warning: DB write failed for {task_id}: {e}");
        }
    }
    // Export to markdown.
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
        } => create::cmd_task_create(
            json,
            epic,
            title,
            deps.as_deref(),
            acceptance_file.as_deref(),
            *priority,
            domain.as_deref(),
            files.as_deref(),
        ),
        TaskCmd::Spec {
            id,
            file,
            desc,
            accept,
        } => query::cmd_task_set_spec(json, id, file.as_deref(), desc.as_deref(), accept.as_deref()),
        TaskCmd::Reset { task_id, cascade } => mutate::cmd_task_reset(json, task_id, *cascade),
        TaskCmd::Skip { task_id, reason } => mutate::cmd_task_skip(json, task_id, reason.as_deref()),
        TaskCmd::Split {
            task_id,
            titles,
            chain,
        } => mutate::cmd_task_split(json, task_id, titles, *chain),
        TaskCmd::SetBackend {
            id,
            impl_spec,
            review,
            sync,
        } => query::cmd_task_set_backend(json, id, impl_spec.as_deref(), review.as_deref(), sync.as_deref()),
        TaskCmd::ShowBackend { id } => query::cmd_task_show_backend(json, id),
        TaskCmd::SetDeps { task_id, deps } => mutate::cmd_task_set_deps(json, task_id, deps),
    }
}
