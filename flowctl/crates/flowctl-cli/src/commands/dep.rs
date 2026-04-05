//! Dependency commands: dep add, dep rm.
//!
//! Updates both the Markdown frontmatter (canonical) and SQLite (cache).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::frontmatter;
use flowctl_core::id::{epic_id_from_task, is_task_id};
use flowctl_core::types::{Task, TASKS_DIR};

use super::helpers::get_flow_dir;

#[derive(Subcommand, Debug)]
pub enum DepCmd {
    /// Add a dependency.
    Add {
        /// Task ID.
        task: String,
        /// Dependency task ID.
        depends_on: String,
    },
    /// Remove a dependency.
    Rm {
        /// Task ID.
        task: String,
        /// Dependency to remove.
        depends_on: String,
    },
}

fn ensure_flow_exists() -> PathBuf {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
    flow_dir
}

/// Try to open a DB connection.
fn try_open_db() -> Option<crate::commands::db_shim::Connection> {
    let cwd = env::current_dir().ok()?;
    crate::commands::db_shim::open(&cwd).ok()
}

/// Read a task document: DB first, markdown fallback.
fn read_task_doc(flow_dir: &Path, task_id: &str) -> (PathBuf, frontmatter::Document<Task>) {
    let task_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", task_id));
    // Try DB first.
    if let Some(conn) = try_open_db() {
        let repo = crate::commands::db_shim::TaskRepo::new(&conn);
        if let Ok((task, body)) = repo.get_with_body(task_id) {
            return (task_path, frontmatter::Document { frontmatter: task, body });
        }
    }
    // Fallback to markdown.
    if !task_path.exists() {
        error_exit(&format!("Task not found: {}", task_id));
    }
    let content = fs::read_to_string(&task_path)
        .unwrap_or_else(|e| error_exit(&format!("Cannot read {}: {}", task_path.display(), e)));
    let doc: frontmatter::Document<Task> = frontmatter::parse(&content)
        .unwrap_or_else(|e| error_exit(&format!("Cannot parse {}: {}", task_path.display(), e)));
    (task_path, doc)
}

/// Write a task document: DB first, then export markdown.
fn write_task_doc(path: &Path, doc: &frontmatter::Document<Task>) {
    // Write to DB.
    if let Some(conn) = try_open_db() {
        let repo = crate::commands::db_shim::TaskRepo::new(&conn);
        if let Err(e) = repo.upsert_with_body(&doc.frontmatter, &doc.body) {
            eprintln!("warning: DB write failed for {}: {e}", doc.frontmatter.id);
        }
    }
    // Export to markdown.
    let content = frontmatter::write(doc)
        .unwrap_or_else(|e| error_exit(&format!("Cannot serialize task: {}", e)));
    fs::write(path, content)
        .unwrap_or_else(|e| error_exit(&format!("Cannot write {}: {}", path.display(), e)));
}

/// Update the SQLite cache for a task's dependencies (best-effort).
fn sync_deps_to_db(task_id: &str, deps: &[String]) {
    let cwd = match env::current_dir() {
        Ok(c) => c,
        Err(_) => return,
    };
    let conn = match crate::commands::db_shim::open(&cwd) {
        Ok(c) => c,
        Err(_) => return,
    };
    let dep_repo = crate::commands::db_shim::DepRepo::new(&conn);
    let _ = dep_repo.replace_task_deps(task_id, deps);
}

pub fn dispatch(cmd: &DepCmd, json: bool) {
    match cmd {
        DepCmd::Add { task, depends_on } => cmd_dep_add(json, task, depends_on),
        DepCmd::Rm { task, depends_on } => cmd_dep_rm(json, task, depends_on),
    }
}

fn cmd_dep_add(json: bool, task_id: &str, depends_on: &str) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }
    if !is_task_id(depends_on) {
        error_exit(&format!(
            "Invalid dependency ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            depends_on
        ));
    }

    // Validate same epic
    let task_epic = epic_id_from_task(task_id)
        .unwrap_or_else(|_| error_exit(&format!("Cannot parse epic from task ID: {}", task_id)));
    let dep_epic = epic_id_from_task(depends_on)
        .unwrap_or_else(|_| error_exit(&format!("Cannot parse epic from dep ID: {}", depends_on)));
    if task_epic != dep_epic {
        error_exit(&format!(
            "Dependencies must be within the same epic. Task {} is in {}, dependency {} is in {}",
            task_id, task_epic, depends_on, dep_epic
        ));
    }

    let (task_path, mut doc) = read_task_doc(&flow_dir, task_id);

    if !doc.frontmatter.depends_on.contains(&depends_on.to_string()) {
        doc.frontmatter.depends_on.push(depends_on.to_string());
        doc.frontmatter.updated_at = Utc::now();
        write_task_doc(&task_path, &doc);
        sync_deps_to_db(task_id, &doc.frontmatter.depends_on);
    }

    if json {
        json_output(json!({
            "task": task_id,
            "depends_on": doc.frontmatter.depends_on,
            "message": format!("Dependency {} added to {}", depends_on, task_id),
        }));
    } else {
        println!("Dependency {} added to {}", depends_on, task_id);
    }
}

fn cmd_dep_rm(json: bool, task_id: &str, depends_on: &str) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!("Invalid task ID: {}", task_id));
    }
    if !is_task_id(depends_on) {
        error_exit(&format!("Invalid dependency ID: {}", depends_on));
    }

    let (task_path, mut doc) = read_task_doc(&flow_dir, task_id);

    if let Some(pos) = doc.frontmatter.depends_on.iter().position(|d| d == depends_on) {
        doc.frontmatter.depends_on.remove(pos);
        doc.frontmatter.updated_at = Utc::now();
        write_task_doc(&task_path, &doc);
        sync_deps_to_db(task_id, &doc.frontmatter.depends_on);

        if json {
            json_output(json!({
                "task": task_id,
                "depends_on": doc.frontmatter.depends_on,
                "removed": true,
                "message": format!("Dependency {} removed from {}", depends_on, task_id),
            }));
        } else {
            println!("Dependency {} removed from {}", depends_on, task_id);
        }
    } else if json {
        json_output(json!({
            "task": task_id,
            "depends_on": doc.frontmatter.depends_on,
            "removed": false,
            "message": format!("{} not in dependencies", depends_on),
        }));
    } else {
        println!("{} is not a dependency of {}", depends_on, task_id);
    }
}
