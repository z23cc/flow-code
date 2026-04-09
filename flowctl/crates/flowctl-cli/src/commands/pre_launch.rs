//! Pre-launch checks: automated verification of 6 dimensions before shipping.

use serde_json::json;
use std::process::Command;

use crate::output::{json_output, pretty_output};

pub fn cmd_pre_launch(json_mode: bool) {
    let mut checks = Vec::new();

    // Get changed files (graceful: empty if not in git repo)
    let changed = Command::new("git")
        .args(["diff", "--name-only", "main...HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let changed_files: Vec<&str> = changed.lines().filter(|l| !l.is_empty()).collect();

    // 1. Code quality: report changed file count
    checks.push(json!({
        "dimension": "code_quality",
        "status": "info",
        "detail": format!("{} files changed", changed_files.len()),
    }));

    // 2. Security: grep for secrets in changed files
    let has_secrets = if !changed_files.is_empty() {
        let result = Command::new("grep")
            .args(["-rn", r"password\|secret\|api_key\|private_key\|token"])
            .args(&changed_files)
            .output();
        result.map_or(false, |o| !o.stdout.is_empty())
    } else {
        false
    };
    checks.push(json!({
        "dimension": "security",
        "status": if has_secrets { "fail" } else { "pass" },
        "detail": if has_secrets { "potential secrets found in changed files" } else { "no secrets detected" },
    }));

    // 3. Performance: manual review advisory
    checks.push(json!({
        "dimension": "performance",
        "status": "info",
        "detail": "manual review recommended for hot paths",
    }));

    // 4. Accessibility: check if frontend files changed
    let has_frontend = changed_files.iter().any(|f| {
        f.ends_with(".tsx")
            || f.ends_with(".jsx")
            || f.ends_with(".vue")
            || f.ends_with(".svelte")
            || f.ends_with(".css")
            || f.ends_with(".html")
    });
    checks.push(json!({
        "dimension": "accessibility",
        "status": if has_frontend { "warn" } else { "pass" },
        "detail": if has_frontend {
            "frontend files changed — verify keyboard nav, contrast, screen reader"
        } else {
            "no frontend changes"
        },
    }));

    // 5. Infrastructure: check for .env or docker changes
    let has_env_change = changed_files
        .iter()
        .any(|f| f.contains(".env") || f.contains("docker"));
    checks.push(json!({
        "dimension": "infrastructure",
        "status": if has_env_change { "warn" } else { "pass" },
        "detail": if has_env_change {
            "env/docker files changed — verify documentation"
        } else {
            "no infra changes"
        },
    }));

    // 6. Documentation: check if docs updated when change is significant
    let has_doc = changed_files
        .iter()
        .any(|f| f.contains("README") || f.contains("CHANGELOG") || f.contains("docs/"));
    let needs_doc = changed_files.len() > 3; // heuristic: significant change needs docs
    checks.push(json!({
        "dimension": "documentation",
        "status": if needs_doc && !has_doc { "warn" } else { "pass" },
        "detail": if needs_doc && !has_doc {
            "significant changes but no doc updates"
        } else {
            "documentation adequate"
        },
    }));

    let fail_count = checks.iter().filter(|c| c["status"] == "fail").count();
    let warn_count = checks.iter().filter(|c| c["status"] == "warn").count();
    let pass_count = checks.iter().filter(|c| c["status"] == "pass").count();

    if json_mode {
        json_output(json!({
            "checks": checks,
            "summary": { "pass": pass_count, "warn": warn_count, "fail": fail_count },
            "ship_ready": fail_count == 0,
        }));
    } else {
        for check in &checks {
            let icon = match check["status"].as_str().unwrap_or("") {
                "pass" => "\u{2705}",
                "warn" => "\u{26a0}\u{fe0f}",
                "fail" => "\u{274c}",
                _ => "\u{2139}\u{fe0f}",
            };
            pretty_output(
                "pre_launch",
                &format!(
                    "{} {}: {}",
                    icon,
                    check["dimension"].as_str().unwrap_or(""),
                    check["detail"].as_str().unwrap_or("")
                ),
            );
        }
        if fail_count > 0 {
            std::process::exit(1);
        }
    }
}
