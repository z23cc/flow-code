//! Init and detect commands.

use std::fs;

use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::types::{
    CONFIG_FILE, EPICS_DIR, MEMORY_DIR, META_FILE, REVIEWS_DIR, SCHEMA_VERSION,
    SPECS_DIR, SUPPORTED_SCHEMA_VERSIONS, TASKS_DIR,
};

use super::{deep_merge, get_default_config, get_flow_dir, write_json_file};

// ── Init command ────────────────────────────────────────────────────

pub fn cmd_init(json: bool) {
    let cwd = std::env::current_dir()
        .unwrap_or_else(|e| error_exit(&format!("Cannot get current dir: {e}")));
    let mut actions: Vec<String> = Vec::new();

    // Ensure .flow/ symlink → .git/flow-state/flow/ (or plain dir outside git)
    match crate::commands::helpers::ensure_flow_symlink(&cwd) {
        Ok(shared_dir) => {
            let flow_dir = get_flow_dir();
            if flow_dir.is_symlink() {
                actions.push(format!(".flow/ → {}", shared_dir.display()));
            }
        }
        Err(e) => error_exit(&format!("Failed to setup .flow/: {e}")),
    }

    let flow_dir = get_flow_dir();

    // Create directories if missing (idempotent, never destroys existing)
    for subdir in &[EPICS_DIR, SPECS_DIR, TASKS_DIR, MEMORY_DIR, REVIEWS_DIR, "checklists", "index"] {
        let dir_path = flow_dir.join(subdir);
        if !dir_path.exists() {
            if let Err(e) = fs::create_dir_all(&dir_path) {
                error_exit(&format!("Failed to create {}: {}", dir_path.display(), e));
            }
            actions.push(format!("created {}/", subdir));
        }
    }

    // Create meta.json if missing (never overwrite existing)
    let meta_path = flow_dir.join(META_FILE);
    if !meta_path.exists() {
        let meta = json!({
            "schema_version": SCHEMA_VERSION,
            "next_epic": 1
        });
        write_json_file(&meta_path, &meta);
        actions.push("created meta.json".to_string());
    }

    // Config: create or upgrade (merge missing defaults)
    let config_path = flow_dir.join(CONFIG_FILE);
    if !config_path.exists() {
        write_json_file(&config_path, &get_default_config());
        actions.push("created config.json".to_string());
    } else {
        // Load raw config, compare with merged (which includes new defaults)
        let raw = match fs::read_to_string(&config_path) {
            Ok(content) => serde_json::from_str::<serde_json::Value>(&content)
                .unwrap_or(json!({})),
            Err(_) => json!({}),
        };
        let merged = deep_merge(&get_default_config(), &raw);
        if merged != raw {
            write_json_file(&config_path, &merged);
            actions.push("upgraded config.json (added missing keys)".to_string());
        }
    }

    // Ensure .state directory exists for runtime state
    let state_dir = flow_dir.join(".state");
    if !state_dir.exists() {
        if let Err(e) = fs::create_dir_all(&state_dir) {
            eprintln!("warning: failed to create .state/: {e}");
        } else {
            actions.push("created .state/".to_string());
        }
    }

    // Ensure FlowStore dirs are ready
    if let Err(e) = flowctl_core::json_store::ensure_dirs(&flow_dir) {
        eprintln!("warning: failed to ensure store dirs: {e}");
    }

    // Create project-context.md with auto-detected stack info if missing
    let project_context_path = flow_dir.join("project-context.md");
    if !project_context_path.exists() {
        let content = generate_project_context(&cwd);
        if let Err(e) = fs::write(&project_context_path, content) {
            eprintln!("warning: failed to create project-context.md: {e}");
        } else {
            actions.push("created project-context.md (auto-detected stack)".to_string());
        }
    }
    let has_project_context = project_context_path.exists();

    // Build output
    let message = if actions.is_empty() {
        ".flow/ already up to date".to_string()
    } else {
        format!(".flow/ updated: {}", actions.join(", "))
    };

    if json {
        json_output(json!({
            "message": message,
            "path": flow_dir.to_string_lossy(),
            "actions": actions,
            "hint": if has_project_context { serde_json::Value::Null } else {
                serde_json::Value::String("Tip: copy templates/project-context.md to .flow/project-context.md to share technical standards with all worker agents".to_string())
            },
        }));
    } else {
        println!("{}", message);
        if !has_project_context {
            println!("Tip: copy templates/project-context.md to .flow/project-context.md to share technical standards with all worker agents");
        }
    }
}

// ── Project context generation ──────────────────────────────────────

/// Detected project stack info used to generate all sections.
struct DetectedStack {
    stack_lines: Vec<String>,
    has_rust: bool,
    has_node: bool,
    has_typescript: bool,
    has_vitest: bool,
    has_jest: bool,
    has_eslint: bool,
    has_python: bool,
    has_pytest: bool,
    has_ruff: bool,
    has_go: bool,
    has_ruby: bool,
    has_rails: bool,
}

fn detect_stack(root: &std::path::Path) -> DetectedStack {
    let mut s = DetectedStack {
        stack_lines: Vec::new(),
        has_rust: false,
        has_node: false,
        has_typescript: false,
        has_vitest: false,
        has_jest: false,
        has_eslint: false,
        has_python: false,
        has_pytest: false,
        has_ruff: false,
        has_go: false,
        has_ruby: false,
        has_rails: false,
    };

    // Rust
    if root.join("Cargo.toml").exists() {
        s.has_rust = true;
        s.stack_lines.push("- Language: Rust".to_string());
        if root.join("Cargo.lock").exists() {
            s.stack_lines.push("- Package Manager: Cargo".to_string());
        }
    }

    // Node / JS / TS
    if root.join("package.json").exists() {
        s.has_node = true;
        let pj = fs::read_to_string(root.join("package.json")).unwrap_or_default();
        if pj.contains("\"react\"") || pj.contains("\"next\"") {
            s.stack_lines.push("- Framework: React / Next.js".to_string());
        } else if pj.contains("\"vue\"") {
            s.stack_lines.push("- Framework: Vue".to_string());
        } else if pj.contains("\"svelte\"") {
            s.stack_lines.push("- Framework: Svelte".to_string());
        }
        if pj.contains("\"typescript\"") {
            s.has_typescript = true;
            s.stack_lines.push("- Language: TypeScript".to_string());
        } else {
            s.stack_lines.push("- Language: JavaScript".to_string());
        }
        if pj.contains("\"vitest\"") {
            s.has_vitest = true;
            s.stack_lines.push("- Testing: Vitest".to_string());
        } else if pj.contains("\"jest\"") {
            s.has_jest = true;
            s.stack_lines.push("- Testing: Jest".to_string());
        }
        if pj.contains("\"eslint\"") {
            s.has_eslint = true;
            s.stack_lines.push("- Linting: ESLint".to_string());
        }
        if pj.contains("\"prettier\"") {
            s.stack_lines.push("- Formatting: Prettier".to_string());
        }
    }

    // Python
    if root.join("pyproject.toml").exists() || root.join("setup.py").exists() || root.join("requirements.txt").exists() {
        s.has_python = true;
        s.stack_lines.push("- Language: Python".to_string());
        let pyp = fs::read_to_string(root.join("pyproject.toml")).unwrap_or_default();
        if pyp.contains("django") { s.stack_lines.push("- Framework: Django".to_string()); }
        if pyp.contains("fastapi") { s.stack_lines.push("- Framework: FastAPI".to_string()); }
        if pyp.contains("flask") { s.stack_lines.push("- Framework: Flask".to_string()); }
        if pyp.contains("pytest") {
            s.has_pytest = true;
            s.stack_lines.push("- Testing: pytest".to_string());
        }
        if pyp.contains("ruff") {
            s.has_ruff = true;
            s.stack_lines.push("- Linting: Ruff".to_string());
        }
    }

    // Go
    if root.join("go.mod").exists() {
        s.has_go = true;
        s.stack_lines.push("- Language: Go".to_string());
    }

    // Ruby
    if root.join("Gemfile").exists() {
        s.has_ruby = true;
        s.stack_lines.push("- Language: Ruby".to_string());
        let gf = fs::read_to_string(root.join("Gemfile")).unwrap_or_default();
        if gf.contains("rails") {
            s.has_rails = true;
            s.stack_lines.push("- Framework: Rails".to_string());
        }
    }

    // Database detection
    if root.join("prisma").exists() || root.join("prisma/schema.prisma").exists() {
        s.stack_lines.push("- ORM: Prisma".to_string());
    }
    let compose = fs::read_to_string(root.join("docker-compose.yml"))
        .or_else(|_| fs::read_to_string(root.join("docker-compose.yaml")))
        .unwrap_or_default();
    if compose.contains("postgres") { s.stack_lines.push("- Database: PostgreSQL".to_string()); }
    if compose.contains("mysql") { s.stack_lines.push("- Database: MySQL".to_string()); }
    if compose.contains("mongo") { s.stack_lines.push("- Database: MongoDB".to_string()); }
    if compose.contains("redis") { s.stack_lines.push("- Database: Redis".to_string()); }

    // Rust workspace testing/linting
    if root.join("flowctl").exists() && s.has_rust {
        s.stack_lines.push("- Testing: cargo test".to_string());
        s.stack_lines.push("- Linting: clippy".to_string());
    }

    s
}

fn generate_guard_commands(s: &DetectedStack) -> String {
    let mut test = String::new();
    let mut lint = String::new();
    let mut typecheck = String::new();
    let mut format_check = String::new();

    if s.has_rust {
        test = "cargo test --all".to_string();
        lint = "cargo clippy --all -- -D warnings".to_string();
        format_check = "cargo fmt --all -- --check".to_string();
    } else if s.has_python {
        if s.has_pytest { test = "pytest".to_string(); }
        lint = if s.has_ruff { "ruff check .".to_string() } else { "flake8".to_string() };
    } else if s.has_node {
        if s.has_vitest {
            test = "npx vitest run".to_string();
        } else if s.has_jest {
            test = "npx jest".to_string();
        }
        if s.has_eslint { lint = "npx eslint .".to_string(); }
        if s.has_typescript { typecheck = "npx tsc --noEmit".to_string(); }
    } else if s.has_go {
        test = "go test ./...".to_string();
        lint = "golangci-lint run".to_string();
    } else if s.has_ruby {
        if s.has_rails {
            test = "bundle exec rspec".to_string();
            lint = "bundle exec rubocop".to_string();
        }
    }

    format!(
        "```yaml\ntest: \"{test}\"\nlint: \"{lint}\"\ntypecheck: \"{typecheck}\"\nformat_check: \"{format_check}\"\n```"
    )
}

fn generate_file_conventions(root: &std::path::Path) -> String {
    let mut frontend: Vec<&str> = Vec::new();
    let mut backend: Vec<&str> = Vec::new();
    let mut testing: Vec<&str> = Vec::new();
    let mut docs: Vec<&str> = Vec::new();

    // Frontend directories
    if root.join("src/components").exists() { frontend.push("src/components/"); }
    if root.join("src/pages").exists() { frontend.push("src/pages/"); }
    if root.join("src/app").exists() { frontend.push("src/app/"); }
    if root.join("src/views").exists() { frontend.push("src/views/"); }
    if root.join("frontend").exists() { frontend.push("frontend/"); }
    if root.join("app/javascript").exists() { frontend.push("app/javascript/"); }

    // Backend directories
    if root.join("src/api").exists() { backend.push("src/api/"); }
    if root.join("src/server").exists() { backend.push("src/server/"); }
    if root.join("src/lib").exists() { backend.push("src/lib/"); }
    if root.join("crates").exists() { backend.push("crates/"); }
    if root.join("flowctl/crates").exists() { backend.push("flowctl/crates/"); }
    if root.join("app/models").exists() { backend.push("app/models/"); }
    if root.join("app/controllers").exists() { backend.push("app/controllers/"); }
    if root.join("cmd").exists() { backend.push("cmd/"); }
    if root.join("internal").exists() { backend.push("internal/"); }
    if root.join("pkg").exists() { backend.push("pkg/"); }

    // Testing directories
    if root.join("tests").exists() { testing.push("tests/"); }
    if root.join("test").exists() { testing.push("test/"); }
    if root.join("spec").exists() { testing.push("spec/"); }
    if root.join("scripts").exists() { testing.push("scripts/"); }
    if root.join("__tests__").exists() { testing.push("__tests__/"); }

    // Docs directories
    if root.join("docs").exists() { docs.push("docs/"); }
    if root.join("doc").exists() { docs.push("doc/"); }

    let fmt_list = |items: &[&str]| -> String {
        if items.is_empty() {
            "[]".to_string()
        } else {
            let quoted: Vec<String> = items.iter().map(|s| format!("\"{}\"", s)).collect();
            format!("[{}]", quoted.join(", "))
        }
    };

    format!(
        "```yaml\nfrontend: {}\nbackend: {}\ntesting: {}\ndocs: {}\n```",
        fmt_list(&frontend),
        fmt_list(&backend),
        fmt_list(&testing),
        fmt_list(&docs),
    )
}

fn detect_rules(root: &std::path::Path) -> Vec<String> {
    let mut rules = Vec::new();

    // CI detection
    if root.join(".github/workflows").exists() { rules.push("- CI: GitHub Actions".to_string()); }
    if root.join(".gitlab-ci.yml").exists() { rules.push("- CI: GitLab CI".to_string()); }

    // Cargo.toml: unsafe_code = "forbid"
    if root.join("Cargo.toml").exists() {
        let cargo = fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
        if cargo.contains("unsafe_code") && cargo.contains("forbid") {
            rules.push("- unsafe_code = forbid (no unsafe Rust allowed)".to_string());
        }
    }

    // tsconfig.json: strict mode
    if root.join("tsconfig.json").exists() {
        let ts = fs::read_to_string(root.join("tsconfig.json")).unwrap_or_default();
        if ts.contains("\"strict\"") && ts.contains("true") {
            rules.push("- TypeScript strict mode enabled".to_string());
        }
    }

    // ESLint config
    if root.join(".eslintrc").exists()
        || root.join(".eslintrc.js").exists()
        || root.join(".eslintrc.json").exists()
        || root.join(".eslintrc.yml").exists()
        || root.join("eslint.config.js").exists()
        || root.join("eslint.config.mjs").exists()
    {
        rules.push("- ESLint enforced".to_string());
    }

    rules
}

fn generate_project_context(root: &std::path::Path) -> String {
    let detected = detect_stack(root);

    let stack_section = if detected.stack_lines.is_empty() {
        "<!-- Could not auto-detect. Fill in your project's stack -->\n- Framework: \n- Language: \n- Database: \n- Testing: \n- Linting: ".to_string()
    } else {
        detected.stack_lines.join("\n")
    };

    let guard_section = generate_guard_commands(&detected);
    let file_conventions = generate_file_conventions(root);

    let rules = detect_rules(root);
    let rules_section = if rules.is_empty() {
        "<!-- Add project-specific rules that agents must follow -->".to_string()
    } else {
        rules.join("\n")
    };

    format!(
        r#"# Project Context

> Shared technical standards for all agents. Auto-loaded by workers during re-anchoring.
> Focus on what's **unobvious** — things agents can't infer from code alone.

## Technology Stack
<!-- Auto-detected by flowctl init. Edit as needed. -->
{stack_section}

## Guard Commands
<!-- Commands that flowctl guard will execute. Leave empty to skip a check. -->
{guard_section}

## Critical Implementation Rules
{rules_section}

## File Conventions
<!-- Maps domains to file patterns for auto domain assignment -->
{file_conventions}

## Architecture Decisions
<!-- Key decisions and their rationale -->

## Non-Goals
<!-- Things agents should NOT do -->
"#
    )
}

// ── Detect command ──────────────────────────────────────────────────

pub fn cmd_detect(json: bool) {
    let flow_dir = get_flow_dir();
    let exists = flow_dir.exists();
    let mut issues: Vec<String> = Vec::new();

    if exists {
        let meta_path = flow_dir.join(META_FILE);
        if !meta_path.exists() {
            issues.push("meta.json missing".to_string());
        } else {
            match fs::read_to_string(&meta_path) {
                Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(meta) => {
                        let version = meta.get("schema_version").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
                        if !SUPPORTED_SCHEMA_VERSIONS.contains(&version) {
                            issues.push(format!(
                                "schema_version unsupported (supported {:?}, got {})",
                                SUPPORTED_SCHEMA_VERSIONS, version
                            ));
                        }
                    }
                    Err(e) => issues.push(format!("meta.json parse error: {}", e)),
                },
                Err(e) => issues.push(format!("meta.json unreadable: {}", e)),
            }
        }

        for subdir in &[EPICS_DIR, SPECS_DIR, TASKS_DIR, MEMORY_DIR, REVIEWS_DIR] {
            if !flow_dir.join(subdir).exists() {
                issues.push(format!("{}/ missing", subdir));
            }
        }
    }

    let valid = exists && issues.is_empty();

    if json {
        json_output(json!({
            "exists": exists,
            "valid": valid,
            "issues": issues,
            "path": flow_dir.to_string_lossy(),
        }));
    } else if exists {
        if valid {
            println!(".flow/ exists and is valid");
        } else {
            println!(".flow/ exists but has issues:");
            for issue in &issues {
                println!("  - {}", issue);
            }
        }
    } else {
        println!(".flow/ not found");
    }
}
