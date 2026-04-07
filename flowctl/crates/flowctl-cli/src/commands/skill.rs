//! Skill commands: register, match.
//!
//! `skill register` scans `skills/*/SKILL.md` files, extracts YAML
//! frontmatter (name + description), and upserts each into the DB with
//! a BGE-small embedding for semantic matching.
//!
//! `skill match` performs semantic vector search against registered
//! skills and returns ranked results.

use clap::Subcommand;
use serde::Deserialize;
use serde_json::json;

use crate::output::{error_exit, json_output, pretty_output};

use super::db_shim;

// ── CLI definition ─────────────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum SkillCmd {
    /// Scan skills/*/SKILL.md and register into DB with embeddings.
    Register {
        /// Directory to scan (default: DROID_PLUGIN_ROOT or CLAUDE_PLUGIN_ROOT).
        #[arg(long)]
        dir: Option<String>,
    },
    /// Semantic search against registered skills.
    Match {
        /// Search query text.
        query: String,
        /// Maximum results to return.
        #[arg(long, default_value = "5")]
        limit: usize,
        /// Minimum cosine similarity threshold.
        #[arg(long, default_value = "0.75")]
        threshold: f64,
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
        SkillCmd::Match {
            query,
            limit,
            threshold,
        } => cmd_skill_match(json, query, *limit, *threshold),
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

    // Upsert each skill into DB.
    let conn = db_shim::require_db().unwrap_or_else(|e| {
        error_exit(&format!("Cannot open DB: {e}"));
    });

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    let repo = flowctl_db::skill::SkillRepo::new(conn.inner_conn());

    for (name, desc, path) in &entries {
        rt.block_on(async {
            repo.upsert(name, desc, Some(path.as_str()))
                .await
                .unwrap_or_else(|e| {
                    eprintln!("warn: failed to upsert skill '{}': {e}", name);
                });
        });
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

// ── Match ──────────────────────────────────────────────────────────

fn cmd_skill_match(json: bool, query: &str, limit: usize, threshold: f64) {
    let conn = db_shim::require_db().unwrap_or_else(|e| {
        error_exit(&format!("Cannot open DB: {e}"));
    });

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    let repo = flowctl_db::skill::SkillRepo::new(conn.inner_conn());
    let matches = rt.block_on(async {
        repo.match_skills(query, limit, threshold)
            .await
            .unwrap_or_else(|e| {
                error_exit(&format!("match_skills failed: {e}"));
            })
    });

    if json {
        let out: Vec<serde_json::Value> = matches
            .iter()
            .map(|m| {
                json!({
                    "name": m.name,
                    "description": m.description,
                    "score": (m.score * 100.0).round() / 100.0,
                })
            })
            .collect();
        json_output(json!(out));
    } else {
        if matches.is_empty() {
            pretty_output("skill_match", "No matching skills found.");
            return;
        }
        pretty_output(
            "skill_match",
            &format!("  {:<6} {:<28} {}", "Score", "Name", "Description"),
        );
        for m in &matches {
            pretty_output(
                "skill_match",
                &format!("  {:<6.2} {:<28} {}", m.score, m.name, m.description),
            );
        }
    }
}
