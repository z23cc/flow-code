//! Skill commands: register.
//!
//! `skill register` scans `skills/*/SKILL.md` files, extracts YAML
//! frontmatter (name + description), and logs them. With file-based storage,
//! skill registration is a scan-and-report operation (no DB upsert needed).

use clap::Subcommand;
use serde::Deserialize;
use serde_json::json;

use crate::output::{error_exit, json_output, pretty_output};

// ── CLI definition ─────────────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum SkillCmd {
    /// Scan skills/*/SKILL.md and register into DB.
    Register {
        /// Directory to scan (default: DROID_PLUGIN_ROOT or CLAUDE_PLUGIN_ROOT).
        #[arg(long)]
        dir: Option<String>,
    },
}

// ── Frontmatter struct ─────────────────────────────────────────────

#[derive(Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
}

// ── Dispatch ───────────────────────────────────────────────────────

pub fn dispatch(cmd: &SkillCmd, json: bool) {
    match cmd {
        SkillCmd::Register { dir } => cmd_skill_register(json, dir.as_deref()),
    }
}

// ── Register ───────────────────────────────────────────────────────

fn cmd_skill_register(json: bool, dir: Option<&str>) {
    // Resolve plugin root directory.
    let root = match dir {
        Some(d) => std::path::PathBuf::from(d),
        None => {
            if let Ok(d) = std::env::var("DROID_PLUGIN_ROOT") {
                std::path::PathBuf::from(d)
            } else if let Ok(d) = std::env::var("CLAUDE_PLUGIN_ROOT") {
                std::path::PathBuf::from(d)
            } else {
                error_exit("No --dir given and DROID_PLUGIN_ROOT / CLAUDE_PLUGIN_ROOT not set");
            }
        }
    };

    let skills_dir = root.join("skills");
    if !skills_dir.is_dir() {
        error_exit(&format!("Skills directory not found: {}", skills_dir.display()));
    }

    // Walk skills/*/SKILL.md
    let mut entries: Vec<(String, String, String)> = Vec::new(); // (name, description, path)
    let read_dir = std::fs::read_dir(&skills_dir).unwrap_or_else(|e| {
        error_exit(&format!("Cannot read {}: {e}", skills_dir.display()));
    });

    for entry in read_dir.flatten() {
        if !entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            continue;
        }
        let skill_md = entry.path().join("SKILL.md");
        if !skill_md.is_file() {
            continue;
        }
        let content = match std::fs::read_to_string(&skill_md) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("warn: cannot read {}: {e}", skill_md.display());
                continue;
            }
        };
        let fm: SkillFrontmatter =
            match flowctl_core::frontmatter::parse_frontmatter(&content) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!(
                        "warn: cannot parse frontmatter in {}: {e}",
                        skill_md.display()
                    );
                    continue;
                }
            };
        entries.push((
            fm.name,
            fm.description,
            skill_md.to_string_lossy().to_string(),
        ));
    }

    let skills_json: Vec<serde_json::Value> = entries
        .iter()
        .map(|(n, d, _)| json!({"name": n, "description": d}))
        .collect();

    if json {
        json_output(json!({
            "registered": entries.len(),
            "skills": skills_json,
        }));
    } else {
        pretty_output("skill_register", &format!("Registered {} skills", entries.len()));
        for (name, desc, _) in &entries {
            pretty_output("skill_register", &format!("  {} — {}", name, desc));
        }
    }
}
