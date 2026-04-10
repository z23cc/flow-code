//! Stack commands: detect, set, show.
//! Invariants commands: init, show, check.
//! Guard command: run deterministic lint/test/typecheck.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::types::CONFIG_FILE;

use super::helpers::get_flow_dir;

// ── Stack ─────────────────────────���───────────────────────────��────

#[derive(Subcommand, Debug)]
pub enum StackCmd {
    /// Auto-detect project stack.
    Detect {
        /// Show detection without saving.
        #[arg(long)]
        dry_run: bool,
    },
    /// Set stack config from JSON file.
    Set {
        /// JSON file path (or - for stdin).
        #[arg(long)]
        file: String,
    },
    /// Show current stack config.
    Show,
}

pub fn dispatch(cmd: &StackCmd, json: bool) {
    match cmd {
        StackCmd::Detect { dry_run } => cmd_stack_detect(json, *dry_run),
        StackCmd::Set { file } => cmd_stack_set(json, file),
        StackCmd::Show => cmd_stack_show(json),
    }
}

// ── Invariants ─────────────────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum InvariantsCmd {
    /// Create invariants.md template.
    Init {
        /// Overwrite existing.
        #[arg(long)]
        force: bool,
    },
    /// Show invariants.
    Show,
    /// Run all verify commands.
    Check,
}

pub fn dispatch_invariants(cmd: &InvariantsCmd, json: bool) {
    match cmd {
        InvariantsCmd::Init { force } => cmd_invariants_init(json, *force),
        InvariantsCmd::Show => cmd_invariants_show(json),
        InvariantsCmd::Check => cmd_invariants_check(json),
    }
}

// ── Helpers ────────────────────────────────────────────────────────

fn get_repo_root() -> std::path::PathBuf {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            std::path::PathBuf::from(path)
        }
        _ => env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
    }
}

fn ensure_flow_exists() {
    if !get_flow_dir().exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
}

/// Load config.json, returning the parsed JSON object.
fn load_config() -> serde_json::Value {
    let config_path = get_flow_dir().join(CONFIG_FILE);
    if !config_path.exists() {
        return json!({});
    }
    match fs::read_to_string(&config_path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or(json!({})),
        Err(_) => json!({}),
    }
}

/// Save config.json.
fn save_config(config: &serde_json::Value) {
    let config_path = get_flow_dir().join(CONFIG_FILE);
    let content =
        serde_json::to_string_pretty(config).expect("config JSON serialization should not fail");
    if let Err(e) = fs::write(&config_path, &content) {
        error_exit(&format!("Failed to write config.json: {}", e));
    }
}

/// Get a top-level config key.
fn get_config_key(key: &str) -> serde_json::Value {
    let config = load_config();
    config.get(key).cloned().unwrap_or(json!({}))
}

/// Set a top-level config key.
fn set_config_key(key: &str, value: serde_json::Value) {
    let mut config = load_config();
    config[key] = value;
    save_config(&config);
}

// ── Stack detection ────────────────────────────────────────────────

/// Auto-detect project tech stack from files in the repo.
fn detect_stack() -> serde_json::Value {
    let repo = get_repo_root();
    let mut stack = json!({});

    // --- Backend detection ---
    let mut backend = json!({});

    // Rust detection — check repo root and one level of subdirectories
    let cargo_toml = repo.join("Cargo.toml");
    let rust_dir = if cargo_toml.exists() {
        Some((repo.clone(), cargo_toml.clone()))
    } else {
        // Check immediate subdirectories for Cargo.toml (e.g., flowctl/Cargo.toml)
        fs::read_dir(&repo).ok().and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
                .find_map(|e| {
                    let candidate = e.path().join("Cargo.toml");
                    if candidate.exists() {
                        Some((e.path(), candidate))
                    } else {
                        None
                    }
                })
        })
    };
    if let Some((rust_root, cargo_path)) = rust_dir {
        backend["language"] = json!("rust");
        // Use relative path prefix for subdirectory workspaces
        let prefix = if rust_root == repo {
            String::new()
        } else {
            format!(
                "cd {} && ",
                rust_root.file_name().unwrap_or_default().to_string_lossy()
            )
        };
        backend["test"] = json!(format!("{prefix}cargo test --all"));
        backend["lint"] = json!(format!("{prefix}cargo clippy --all-targets -- -D warnings"));
        backend["typecheck"] = json!(format!("{prefix}cargo check"));

        if let Ok(content) = fs::read_to_string(&cargo_path) {
            if content.contains("actix") {
                backend["framework"] = json!("actix");
            } else if content.contains("axum") {
                backend["framework"] = json!("axum");
            } else if content.contains("rocket") {
                backend["framework"] = json!("rocket");
            }
        }
    }

    // Python detection
    let pyproject = repo.join("pyproject.toml");
    let requirements = repo.join("requirements.txt");
    let setup_py = repo.join("setup.py");
    let manage_py = repo.join("manage.py");

    let has_python = pyproject.exists() || requirements.exists() || setup_py.exists();

    if has_python && !cargo_toml.exists() {
        let mut py_content = String::new();
        if pyproject.exists() {
            py_content += &fs::read_to_string(&pyproject).unwrap_or_default();
        }
        if requirements.exists() {
            py_content += &fs::read_to_string(&requirements).unwrap_or_default();
        }

        backend["language"] = json!("python");

        // Framework
        let py_lower = py_content.to_lowercase();
        if manage_py.exists() || py_lower.contains("django") {
            backend["framework"] = json!("django");
            let mut conventions = Vec::new();
            if py_content.contains("rest_framework") || py_content.contains("djangorestframework") {
                conventions.push("DRF");
            }
            if py_lower.contains("celery") {
                conventions.push("Celery");
            }
            if !conventions.is_empty() {
                backend["conventions"] = json!(conventions.join(", "));
            }
        } else if py_lower.contains("flask") {
            backend["framework"] = json!("flask");
        } else if py_lower.contains("fastapi") {
            backend["framework"] = json!("fastapi");
        }

        // Test
        if py_content.contains("pytest") {
            backend["test"] = json!("pytest");
        } else if manage_py.exists() {
            backend["test"] = json!("python manage.py test");
        }

        // Lint
        if py_content.contains("ruff") {
            backend["lint"] = json!("ruff check");
        } else if py_content.contains("flake8") {
            backend["lint"] = json!("flake8");
        }

        // Type check
        if py_content.contains("mypy") {
            backend["typecheck"] = json!("mypy");
        } else if py_content.contains("pyright") {
            backend["typecheck"] = json!("pyright");
        }
    }

    // Go detection
    let go_mod = repo.join("go.mod");
    if go_mod.exists() && !has_python && !cargo_toml.exists() {
        backend["language"] = json!("go");
        backend["test"] = json!("go test ./...");
        backend["lint"] = json!("golangci-lint run");
        if let Ok(go_content) = fs::read_to_string(&go_mod) {
            if go_content.contains("gin-gonic") {
                backend["framework"] = json!("gin");
            } else if go_content.contains("labstack/echo") {
                backend["framework"] = json!("echo");
            } else if go_content.contains("gofiber") {
                backend["framework"] = json!("fiber");
            }
        }
    }

    if backend != json!({}) {
        stack["backend"] = backend;
    }

    // --- Frontend detection ---
    let mut frontend = json!({});

    // Find package.json (root or common frontend dirs)
    let mut pkg_json: Option<serde_json::Value> = None;
    let mut pkg_path: Option<std::path::PathBuf> = None;
    let mut best_dep_count: i64 = -1;

    let pkg_candidates: Vec<std::path::PathBuf> = vec![
        repo.join("package.json"),
        repo.join("frontend/package.json"),
        repo.join("client/package.json"),
        repo.join("web/package.json"),
        repo.join("app/package.json"),
    ];

    for p in &pkg_candidates {
        if p.exists() {
            if let Ok(content) = fs::read_to_string(p) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                    let dep_count = parsed
                        .get("dependencies")
                        .and_then(|d| d.as_object())
                        .map(serde_json::Map::len)
                        .unwrap_or(0)
                        + parsed
                            .get("devDependencies")
                            .and_then(|d| d.as_object())
                            .map(serde_json::Map::len)
                            .unwrap_or(0);
                    if dep_count as i64 > best_dep_count {
                        best_dep_count = dep_count as i64;
                        pkg_json = Some(parsed);
                        pkg_path = Some(p.clone());
                    }
                }
            }
        }
    }

    if let (Some(pkg), Some(ppath)) = (pkg_json, pkg_path) {
        let mut all_deps = serde_json::Map::new();
        if let Some(deps) = pkg.get("dependencies").and_then(|d| d.as_object()) {
            all_deps.extend(deps.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        if let Some(deps) = pkg.get("devDependencies").and_then(|d| d.as_object()) {
            all_deps.extend(deps.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        let scripts = pkg.get("scripts").and_then(|s| s.as_object());

        // Language
        let pkg_parent = ppath.parent().unwrap_or(Path::new("."));
        if repo.join("tsconfig.json").exists()
            || pkg_parent.join("tsconfig.json").exists()
            || pkg_parent.join("tsconfig.app.json").exists()
        {
            frontend["language"] = json!("typescript");
        } else {
            frontend["language"] = json!("javascript");
        }

        // Framework
        if all_deps.contains_key("react") {
            frontend["framework"] = json!("react");
        } else if all_deps.contains_key("vue") {
            frontend["framework"] = json!("vue");
        } else if all_deps.contains_key("svelte") {
            frontend["framework"] = json!("svelte");
        } else if all_deps.contains_key("@angular/core") {
            frontend["framework"] = json!("angular");
        }

        // Meta-framework
        if all_deps.contains_key("next") {
            frontend["meta_framework"] = json!("nextjs");
        } else if all_deps.contains_key("nuxt") {
            frontend["meta_framework"] = json!("nuxt");
        } else if all_deps.contains_key("@remix-run/react") {
            frontend["meta_framework"] = json!("remix");
        }

        // Package manager
        let pkg_mgr = if pkg_parent.join("pnpm-lock.yaml").exists() {
            "pnpm"
        } else if pkg_parent.join("yarn.lock").exists() {
            "yarn"
        } else if pkg_parent.join("bun.lockb").exists() || pkg_parent.join("bun.lock").exists() {
            "bun"
        } else {
            "npm"
        };

        // Prefix for subdirectory projects
        let prefix = if pkg_parent != repo.as_path() {
            if let Ok(rel) = pkg_parent.strip_prefix(&repo) {
                format!("cd {} && ", rel.display())
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Commands from scripts
        if let Some(sc) = scripts {
            if sc.contains_key("test") {
                frontend["test"] = json!(format!("{}{} test", prefix, pkg_mgr));
            }
            if sc.contains_key("lint") {
                frontend["lint"] = json!(format!("{}{} run lint", prefix, pkg_mgr));
            }
            if sc.contains_key("typecheck") || sc.contains_key("type-check") {
                let tc_key = if sc.contains_key("typecheck") {
                    "typecheck"
                } else {
                    "type-check"
                };
                frontend["typecheck"] = json!(format!("{}{} run {}", prefix, pkg_mgr, tc_key));
            } else if frontend.get("language").and_then(|l| l.as_str()) == Some("typescript") {
                frontend["typecheck"] = json!(format!("{}npx tsc --noEmit", prefix));
            }
        }

        // Tailwind
        let has_tailwind = all_deps.contains_key("tailwindcss")
            || repo.join("tailwind.config.js").exists()
            || repo.join("tailwind.config.ts").exists();
        if has_tailwind {
            let existing = frontend
                .get("conventions")
                .and_then(|c| c.as_str())
                .unwrap_or("");
            if existing.is_empty() {
                frontend["conventions"] = json!("Tailwind");
            } else {
                frontend["conventions"] = json!(format!("Tailwind, {}", existing));
            }
        }
    }

    if frontend != json!({}) {
        stack["frontend"] = frontend;
    }

    // --- Infra detection ---
    let mut infra = json!({});

    if repo.join("Dockerfile").exists() {
        infra["runtime"] = json!("docker");
    }
    if repo.join("docker-compose.yml").exists()
        || repo.join("docker-compose.yaml").exists()
        || repo.join("compose.yml").exists()
        || repo.join("compose.yaml").exists()
    {
        infra["compose"] = json!(true);
    }
    if repo.join("terraform").is_dir() {
        infra["iac"] = json!("terraform");
    } else if repo.join("pulumi").is_dir() {
        infra["iac"] = json!("pulumi");
    }

    if infra != json!({}) {
        stack["infra"] = infra;
    }

    stack
}

// ── Stack commands ─────────────────────────────────────────────────

fn cmd_stack_detect(json_mode: bool, dry_run: bool) {
    ensure_flow_exists();

    let stack = detect_stack();

    if stack == json!({}) {
        if json_mode {
            json_output(json!({"stack": {}, "message": "no stack detected"}));
        } else {
            println!("No stack detected.");
        }
        return;
    }

    if !dry_run {
        set_config_key("stack", stack.clone());
    }

    if json_mode {
        let msg = if dry_run {
            "stack auto-detected (dry-run)"
        } else {
            "stack auto-detected"
        };
        json_output(json!({"stack": stack, "message": msg}));
    } else {
        if dry_run {
            println!("Detected stack (dry-run, not saved):");
        } else {
            println!("Stack detected and saved:");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&stack).unwrap_or_default()
        );
    }
}

fn cmd_stack_set(json_mode: bool, file: &str) {
    ensure_flow_exists();

    let raw = if file == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .unwrap_or_else(|e| {
                error_exit(&format!("Failed to read stdin: {}", e));
            });
        buf
    } else {
        fs::read_to_string(file).unwrap_or_else(|e| {
            error_exit(&format!("Failed to read {}: {}", file, e));
        })
    };

    let stack_data: serde_json::Value = serde_json::from_str(&raw).unwrap_or_else(|e| {
        error_exit(&format!("Invalid JSON: {}", e));
    });

    if !stack_data.is_object() {
        error_exit("Stack config must be a JSON object");
    }

    set_config_key("stack", stack_data.clone());

    if json_mode {
        json_output(json!({"stack": stack_data, "message": "stack config updated"}));
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&stack_data).unwrap_or_default()
        );
    }
}

fn cmd_stack_show(json_mode: bool) {
    ensure_flow_exists();

    let stack = get_config_key("stack");

    if json_mode {
        json_output(json!({"stack": stack}));
    } else if stack == json!({}) {
        println!("No stack configured. Use 'flowctl stack set --file <path>' to set.");
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&stack).unwrap_or_default()
        );
    }
}

// ── Invariants commands ────────────────────────────────────────────

const INVARIANTS_FILE: &str = "invariants.md";

fn invariants_path() -> std::path::PathBuf {
    get_flow_dir().join(INVARIANTS_FILE)
}

fn cmd_invariants_init(json_mode: bool, force: bool) {
    ensure_flow_exists();

    let inv_path = invariants_path();
    if inv_path.exists() && !force {
        if json_mode {
            json_output(json!({
                "created": false,
                "message": "invariants.md already exists. Use --force to overwrite.",
            }));
        } else {
            println!("invariants.md already exists. Use --force to overwrite.");
        }
        return;
    }

    let template = r#"# Architecture Invariants

Rules that must NEVER be violated, regardless of task or feature.
Workers check these during Phase 1. Planners check during Step 1.

<!-- Add your project's invariants below. Format:

## [Concept Name]
- **Rule:** [what must always hold]
- **Verify:** `shell command that exits 0 if invariant holds`
- **Fix:** [how to fix if violated]

-->
"#;

    if let Err(e) = fs::write(&inv_path, template) {
        error_exit(&format!("Failed to write invariants.md: {}", e));
    }

    if json_mode {
        json_output(json!({
            "created": true,
            "path": inv_path.to_string_lossy(),
            "message": "invariants.md created",
        }));
    } else {
        println!("Created: {}", inv_path.display());
    }
}

fn cmd_invariants_show(json_mode: bool) {
    ensure_flow_exists();

    let inv_path = invariants_path();
    if !inv_path.exists() {
        if json_mode {
            json_output(json!({
                "invariants": null,
                "message": "no invariants.md \u{2014} create with 'flowctl invariants init'",
            }));
        } else {
            println!("No invariants.md. Create with: flowctl invariants init");
        }
        return;
    }

    let content = fs::read_to_string(&inv_path).unwrap_or_else(|e| {
        error_exit(&format!("Failed to read invariants.md: {}", e));
    });

    if json_mode {
        json_output(json!({
            "invariants": content,
            "path": inv_path.to_string_lossy(),
        }));
    } else {
        println!("{}", content);
    }
}

fn cmd_invariants_check(json_mode: bool) {
    ensure_flow_exists();

    let inv_path = invariants_path();
    if !inv_path.exists() {
        if json_mode {
            json_output(json!({
                "all_passed": true,
                "results": [],
                "message": "no invariants.md",
            }));
        } else {
            println!("No invariants.md \u{2014} nothing to check.");
        }
        return;
    }

    let content = fs::read_to_string(&inv_path).unwrap_or_else(|e| {
        error_exit(&format!("Failed to read invariants.md: {}", e));
    });

    // Strip HTML comments before parsing
    let comment_re = regex::Regex::new(r"(?s)<!--.*?-->").expect("static regex must compile");
    let content_clean = comment_re.replace_all(&content, "");

    let verify_re = regex::Regex::new(r"`([^`]+)`").expect("static regex must compile");
    let repo_root = get_repo_root();

    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut all_passed = true;
    let mut current_name: Option<String> = None;

    for line in content_clean.lines() {
        if let Some(name) = line.strip_prefix("## ") {
            current_name = Some(name.trim().to_string());
        } else if line.contains("**Verify:**") {
            if let Some(ref name) = current_name {
                if let Some(captures) = verify_re.captures(line) {
                    let cmd = &captures[1];

                    if !json_mode {
                        println!("\u{25b8} [{}] {}", name, cmd);
                    }

                    let result = Command::new("sh")
                        .args(["-c", cmd])
                        .current_dir(&repo_root)
                        .stdout(if json_mode {
                            std::process::Stdio::piped()
                        } else {
                            std::process::Stdio::inherit()
                        })
                        .stderr(if json_mode {
                            std::process::Stdio::piped()
                        } else {
                            std::process::Stdio::inherit()
                        })
                        .status();

                    let rc = result.map(|s| s.code().unwrap_or(1)).unwrap_or(1);
                    let passed = rc == 0;
                    if !passed {
                        all_passed = false;
                    }

                    results.push(json!({
                        "name": name,
                        "command": cmd,
                        "passed": passed,
                        "exit_code": rc,
                    }));

                    if !json_mode {
                        let mark = if passed { "\u{2713}" } else { "\u{2717}" };
                        println!("  {} exit {}", mark, rc);
                    }
                }
                current_name = None;
            }
        }
    }

    if json_mode {
        json_output(json!({
            "all_passed": all_passed,
            "results": results,
        }));
    } else {
        let total = results.len();
        let passed_count = results
            .iter()
            .filter(|r| r["passed"] == json!(true))
            .count();
        if total == 0 {
            println!("No verify commands found in invariants.md.");
        } else {
            let suffix = if all_passed { "" } else { " \u{2014} VIOLATED" };
            println!("\n{}/{} invariants hold{}", passed_count, total, suffix);
        }
    }

    if !all_passed {
        std::process::exit(1);
    }
}
