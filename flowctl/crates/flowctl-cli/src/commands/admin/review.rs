//! Review-backend and parse-findings commands.

use std::fs;
use std::path::Path;

use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::types::{CONFIG_FILE, REVIEWS_DIR};

use super::{get_default_config, get_flow_dir};

// ── Review-backend command ─────────────────────────────────────────

pub fn cmd_review_backend(json_mode: bool, compare: Option<String>, epic: Option<String>) {
    // Priority: FLOW_REVIEW_BACKEND env > config (non-null) > default config
    let default_backend = get_default_config()
        .pointer("/review/backend")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "rp".to_string());

    let (backend, source) = if let Ok(env_val) = std::env::var("FLOW_REVIEW_BACKEND") {
        let trimmed = env_val.trim().to_string();
        if ["rp", "codex", "none"].contains(&trimmed.as_str()) {
            (trimmed, "env".to_string())
        } else {
            (default_backend.clone(), "default".to_string())
        }
    } else {
        let flow_dir = get_flow_dir();
        if flow_dir.exists() {
            let config_path = flow_dir.join(CONFIG_FILE);
            let config = if config_path.exists() {
                match fs::read_to_string(&config_path) {
                    Ok(content) => {
                        serde_json::from_str::<serde_json::Value>(&content)
                            .unwrap_or(json!({}))
                    }
                    Err(_) => json!({}),
                }
            } else {
                json!({})
            };

            // Read from user config; if null or missing, fall back to default
            let cfg_val = config
                .pointer("/review/backend")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if ["rp", "codex", "none"].contains(&cfg_val) {
                (cfg_val.to_string(), "config".to_string())
            } else {
                (default_backend, "default".to_string())
            }
        } else {
            (default_backend, "default".to_string())
        }
    };

    // --compare mode: compare review receipt files
    let receipt_files: Option<Vec<String>> = if let Some(epic_id) = &epic {
        if compare.is_none() {
            let flow_dir = get_flow_dir();
            let reviews_dir = flow_dir.join(REVIEWS_DIR);
            if !reviews_dir.exists() {
                if json_mode {
                    json_output(json!({"backend": backend, "source": source}));
                } else {
                    println!("{}", backend);
                }
                return;
            }
            let mut files: Vec<String> = Vec::new();
            if let Ok(entries) = fs::read_dir(&reviews_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.contains(&format!("-{}.", epic_id)) && name.ends_with(".json") {
                        files.push(entry.path().to_string_lossy().to_string());
                    }
                }
            }
            files.sort();
            if files.is_empty() {
                None
            } else {
                Some(files)
            }
        } else {
            None
        }
    } else {
        None
    };

    let receipt_files = receipt_files.or_else(|| {
        compare.map(|c| c.split(',').map(|f| f.trim().to_string()).collect())
    });

    if let Some(files) = receipt_files {
        let mut reviews: Vec<serde_json::Value> = Vec::new();
        for rf in &files {
            let rpath = Path::new(rf);
            if !rpath.exists() {
                error_exit(&format!("Receipt file not found: {}", rf));
            }
            match fs::read_to_string(rpath) {
                Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(rdata) => {
                        reviews.push(json!({
                            "file": rf,
                            "mode": rdata.get("mode").and_then(|v| v.as_str()).unwrap_or("unknown"),
                            "verdict": rdata.get("verdict").and_then(|v| v.as_str()).unwrap_or("unknown"),
                            "id": rdata.get("id").and_then(|v| v.as_str()).unwrap_or("unknown"),
                            "timestamp": rdata.get("timestamp").and_then(|v| v.as_str()).unwrap_or(""),
                            "review": rdata.get("review").and_then(|v| v.as_str()).unwrap_or(""),
                        }));
                    }
                    Err(e) => {
                        error_exit(&format!("Invalid receipt JSON: {}: {}", rf, e));
                    }
                },
                Err(e) => {
                    error_exit(&format!("Could not read receipt: {}: {}", rf, e));
                }
            }
        }

        // Analyze verdicts
        let mut verdicts: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for r in &reviews {
            let mode = r["mode"].as_str().unwrap_or("unknown").to_string();
            let verdict = r["verdict"].as_str().unwrap_or("unknown").to_string();
            verdicts.insert(mode, verdict);
        }
        let verdict_values: std::collections::HashSet<&String> = verdicts.values().collect();
        let all_same = verdict_values.len() <= 1;
        let consensus = if all_same {
            verdicts.values().next().cloned()
        } else {
            None
        };

        if json_mode {
            json_output(json!({
                "reviews": reviews.len(),
                "verdicts": verdicts,
                "consensus": consensus,
                "has_conflict": !all_same,
                "details": reviews,
            }));
        } else {
            println!("Review Comparison ({} reviews):\n", reviews.len());
            for r in &reviews {
                println!(
                    "  [{}] verdict: {}  ({})",
                    r["mode"].as_str().unwrap_or(""),
                    r["verdict"].as_str().unwrap_or(""),
                    r["file"].as_str().unwrap_or("")
                );
            }
            println!();
            if all_same {
                println!("Consensus: {}", consensus.unwrap_or_default());
            } else {
                println!("CONFLICT \u{2014} reviewers disagree:");
                for (mode, verdict) in &verdicts {
                    println!("  {}: {}", mode, verdict);
                }
            }
        }
        return;
    }

    if json_mode {
        json_output(json!({"backend": backend, "source": source}));
    } else {
        println!("{}", backend);
    }
}

// ── Parse-findings command ─────────────────────────────────────────

pub fn cmd_parse_findings(
    json_mode: bool,
    file: String,
    _epic: Option<String>,
    _register: bool,
    _source: String,
) {
    use flowctl_core::review_protocol::{
        filter_by_confidence, AutofixClass, FindingOwner, ReviewFinding, Severity,
    };

    // Read input from file or stdin
    let text = if file == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .unwrap_or_else(|e| {
                error_exit(&format!("Failed to read stdin: {}", e));
            });
        buf
    } else {
        match fs::read_to_string(&file) {
            Ok(content) => content,
            Err(e) => {
                error_exit(&format!("Failed to read file {}: {}", file, e));
            }
        }
    };

    let mut findings: Vec<ReviewFinding> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    // CE schema required keys
    let required_keys = ["severity", "file", "line", "confidence", "evidence"];

    // Tiered extraction:
    // 1. <findings>...</findings> tag
    // 2. Bare JSON array
    // 3. Markdown code block
    let raw_json = if let Some(start) = text.find("<findings>") {
        if let Some(end) = text.find("</findings>") {
            let inner = &text[start + 10..end];
            Some(inner.trim().to_string())
        } else {
            None
        }
    } else {
        None
    };

    let raw_json = raw_json.or_else(|| {
        // Tier 2: bare JSON array [{...}]
        if let Some(start) = text.find('[') {
            if let Some(end) = text.rfind(']') {
                let candidate = &text[start..=end];
                warnings.push("No <findings> tag found; extracted bare JSON array".to_string());
                Some(candidate.to_string())
            } else {
                None
            }
        } else {
            None
        }
    });

    if let Some(raw) = raw_json {
        // Remove trailing commas before ] or }
        let cleaned = raw.replace(",]", "]").replace(",}", "}");

        match serde_json::from_str::<serde_json::Value>(&cleaned) {
            Ok(serde_json::Value::Array(arr)) => {
                let mut raw_findings: Vec<ReviewFinding> = Vec::new();
                for (i, item) in arr.iter().enumerate() {
                    if !item.is_object() {
                        warnings.push(format!("Finding {} is not an object, skipping", i));
                        continue;
                    }
                    let missing: Vec<&&str> = required_keys
                        .iter()
                        .filter(|k| item.get(**k).is_none())
                        .collect();
                    if !missing.is_empty() {
                        let keys: Vec<&str> = missing.iter().map(|k| **k).collect();
                        warnings.push(format!(
                            "Finding {} missing keys: {}, skipping",
                            i,
                            keys.join(", ")
                        ));
                        continue;
                    }

                    let severity = match item.get("severity").and_then(|v| v.as_str()) {
                        Some("P0") | Some("critical") => Severity::P0,
                        Some("P1") | Some("warning") => Severity::P1,
                        Some("P2") => Severity::P2,
                        _ => Severity::P3,
                    };
                    let category = item
                        .get("category")
                        .and_then(|v| v.as_str())
                        .unwrap_or("general")
                        .to_string();
                    let description = item
                        .get("description")
                        .or_else(|| item.get("title"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let file_path = item.get("file").and_then(|v| v.as_str()).map(String::from);
                    let line = item.get("line").and_then(serde_json::Value::as_u64).map(|n| n as u32);
                    let confidence = item
                        .get("confidence")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.8);
                    let autofix_class =
                        match item.get("autofix_class").and_then(|v| v.as_str()) {
                            Some("safe_auto") => AutofixClass::SafeAuto,
                            Some("gated_auto") => AutofixClass::GatedAuto,
                            Some("advisory") => AutofixClass::Advisory,
                            _ => AutofixClass::Manual,
                        };
                    let owner = match item.get("owner").and_then(|v| v.as_str()) {
                        Some("review-fixer") => FindingOwner::ReviewFixer,
                        Some("downstream-resolver") => FindingOwner::DownstreamResolver,
                        Some("human") => FindingOwner::Human,
                        Some("release") => FindingOwner::Release,
                        _ => FindingOwner::ReviewFixer,
                    };
                    let evidence = item
                        .get("evidence")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let pre_existing = item
                        .get("pre_existing")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    let requires_verification = item
                        .get("requires_verification")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    let suggested_fix = item
                        .get("suggested_fix")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let why_it_matters = item
                        .get("why_it_matters")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    raw_findings.push(ReviewFinding {
                        severity,
                        category,
                        description,
                        file: file_path,
                        line,
                        confidence,
                        autofix_class,
                        owner,
                        evidence,
                        pre_existing,
                        requires_verification,
                        suggested_fix,
                        why_it_matters,
                    });
                }

                // Apply confidence filtering
                findings = filter_by_confidence(raw_findings);

                // Cap at 50
                if findings.len() > 50 {
                    warnings.push(format!(
                        "Found {} findings, capping at 50",
                        findings.len()
                    ));
                    findings.truncate(50);
                }
            }
            Ok(_) => {
                warnings.push("Findings JSON is not a list".to_string());
            }
            Err(e) => {
                warnings.push(format!("Failed to parse findings JSON: {}", e));
            }
        }
    } else {
        warnings.push("No findings found in review output".to_string());
    }

    if json_mode {
        let findings_json: Vec<serde_json::Value> = findings
            .iter()
            .map(|f| serde_json::to_value(f).unwrap_or(json!({})))
            .collect();
        json_output(json!({
            "findings": findings_json,
            "count": findings.len(),
            "registered": 0,
            "warnings": warnings,
        }));
    } else {
        println!("Found {} finding(s)", findings.len());
        for w in &warnings {
            eprintln!("  Warning: {}", w);
        }
        for f in &findings {
            let loc = f
                .file
                .as_deref()
                .map(|fp| {
                    if let Some(ln) = f.line {
                        format!("{}:{}", fp, ln)
                    } else {
                        fp.to_string()
                    }
                })
                .unwrap_or_default();
            println!(
                "  [{}] {} \u{2014} {} (confidence: {:.0}%)",
                f.severity, f.description, loc, f.confidence * 100.0
            );
        }
    }
}
