//! Shared helpers used across hook sub-modules.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

/// Find flow directory — delegates to shared resolver (git-common-dir aware).
pub fn get_flow_dir() -> PathBuf {
    crate::commands::helpers::get_flow_dir()
}

/// Read JSON from stdin (hook protocol input).
pub fn read_stdin_json() -> Value {
    use std::io::Read as _;
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    serde_json::from_str(&input).unwrap_or(json!({}))
}

/// Block with reason message and exit code 2.
pub fn output_block(reason: &str) -> ! {
    eprintln!("{reason}");
    std::process::exit(2);
}

/// Output JSON response and exit code 0.
pub fn output_json_and_exit(data: &Value) -> ! {
    println!("{}", serde_json::to_string(data).unwrap_or_default());
    std::process::exit(0);
}

/// Get the repository root via git.
pub fn get_repo_root() -> PathBuf {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            PathBuf::from(String::from_utf8_lossy(&o.stdout).trim().to_string())
        }
        _ => env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}

/// Find flowctl binary for guard hooks (checks FLOWCTL env, then plugin root).
pub fn find_flowctl_for_guard() -> Option<PathBuf> {
    if let Ok(f) = env::var("FLOWCTL") {
        let p = PathBuf::from(&f);
        if p.exists() {
            return Some(p);
        }
    }
    let plugin_root = env::var("DROID_PLUGIN_ROOT")
        .or_else(|_| env::var("CLAUDE_PLUGIN_ROOT"))
        .unwrap_or_default();
    if !plugin_root.is_empty() {
        let p = PathBuf::from(&plugin_root).join("bin").join("flowctl");
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Get current executable path for subprocess calls.
pub fn self_exe() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| {
        if p.exists() {
            Some(p)
        } else {
            find_flowctl_for_guard()
        }
    })
}

/// Run flowctl with args and parse JSON output.
pub fn run_flowctl(flowctl: &Path, args: &[&str]) -> Option<Value> {
    let result = Command::new(flowctl).args(args).output();
    match result {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            serde_json::from_str(stdout.trim()).ok()
        }
        _ => None,
    }
}

/// Normalize a command string (collapse whitespace, remove empty quotes).
pub fn normalize_command(cmd: &str) -> String {
    let mut s = cmd.replace('\t', " ");
    s = s.replace("\"\"", "").replace("''", "");
    let ws_re = regex::Regex::new(r" {2,}").unwrap();
    ws_re.replace_all(&s, " ").trim().to_string()
}

/// Check if memory is enabled in .flow/config.json.
pub fn is_memory_enabled() -> bool {
    let config_path = get_repo_root().join(".flow").join("config.json");
    if !config_path.exists() {
        return false;
    }
    let Ok(content) = std::fs::read_to_string(&config_path) else {
        return false;
    };
    let Ok(config) = serde_json::from_str::<Value>(&content) else {
        return false;
    };
    config
        .get("memory")
        .and_then(|m| m.get("enabled"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// Make a path relative to a base directory.
pub fn pathdiff_relative(file_path: &str, base: &Path) -> String {
    let fp = Path::new(file_path);
    if let Ok(stripped) = fp.strip_prefix(base) {
        stripped.to_string_lossy().to_string()
    } else {
        file_path.to_string()
    }
}

/// Simple UTC timestamp without pulling in chrono crate.
pub fn chrono_utc_now() -> String {
    let output = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }
        _ => "1970-01-01T00:00:00Z".into(),
    }
}
