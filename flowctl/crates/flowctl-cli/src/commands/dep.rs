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
use flowctl_core::types::{Task, FLOW_DIR, TASKS_DIR};

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

fn get_flow_dir() -> PathBuf {
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(FLOW_DIR)
}

fn ensure_flow_exists() -> PathBuf {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
    flow_dir
}

/// Read a task's Markdown document (frontmatter + body).
fn read_task_doc(flow_dir: &Path, task_id: &str) -> (PathBuf, frontmatter::Document<Task>) {
    let task_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", task_id));
    if !task_path.exists() {
        error_exit(&format!("Task not found: {}", task_id));
    }
    let content = fs::read_to_string(&task_path)
        .unwrap_or_else(|e| error_exit(&format!("Cannot read {}: {}", task_path.display(), e)));
    let doc: frontmatter::Document<Task> = frontmatter::parse(&content)
        .unwrap_or_else(|e| error_exit(&format!("Cannot parse {}: {}", task_path.display(), e)));
    (task_path, doc)
}

/// Write a task's Markdown document back to disk.
fn write_task_doc(path: &Path, doc: &frontmatter::Document<Task>) {
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
    let conn = match flowctl_db::open(&cwd) {
        Ok(c) => c,
        Err(_) => return,
    };
    // Delete existing deps, re-insert
    let _ = conn.execute("DELETE FROM task_deps WHERE task_id = ?1", rusqlite::params![task_id]);
    for dep in deps {
        let _ = conn.execute(
            "INSERT INTO task_deps (task_id, depends_on) VALUES (?1, ?2)",
            rusqlite::params![task_id, dep],
        );
    }
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
    } else {
        if json {
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
}
