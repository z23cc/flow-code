//! JSON output helpers for consistent CLI responses.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{json, Value};

/// Global compact mode flag, set once at startup.
static COMPACT: AtomicBool = AtomicBool::new(false);

/// Fields stripped in compact mode (LLM doesn't need these).
const COMPACT_STRIP: &[&str] = &[
    "success",
    "message",
    "created_at",
    "updated_at",
    "spec_path",
    "file_path",
    "schema_version",
];

/// Shared output options (flattened into every subcommand via clap).
#[derive(clap::Args, Debug, Clone)]
pub struct OutputOpts {
    /// Output as JSON.
    #[arg(long, global = true)]
    pub json: bool,

    /// Strip fields LLM doesn't need from JSON output.
    /// Auto-enabled when stdout is not a TTY.
    #[arg(long, global = true)]
    pub compact: bool,
}

/// Initialize compact mode from CLI flags. Call once at startup.
pub fn init_compact(explicit: bool) {
    let enabled = explicit || !std::io::stdout().is_terminal();
    COMPACT.store(enabled, Ordering::Relaxed);
}

/// Strip compact fields from a JSON value (recursive into arrays/objects).
fn strip_compact(val: &mut Value) {
    match val {
        Value::Object(map) => {
            for key in COMPACT_STRIP {
                map.remove(*key);
            }
            for v in map.values_mut() {
                strip_compact(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_compact(v);
            }
        }
        _ => {}
    }
}

/// Print a successful JSON response with additional data fields merged in.
pub fn json_output(data: Value) {
    let mut obj = match data {
        Value::Object(map) => map,
        _ => {
            let mut m = serde_json::Map::new();
            m.insert("data".to_string(), data);
            m
        }
    };
    obj.insert("success".to_string(), json!(true));
    let mut val = Value::Object(obj);
    if COMPACT.load(Ordering::Relaxed) {
        strip_compact(&mut val);
    }
    println!("{}", serde_json::to_string(&val).unwrap());
}

/// Print an error JSON response and exit with code 1.
#[allow(dead_code)]
pub fn error_exit(message: &str) -> ! {
    let out = json!({
        "success": false,
        "error": message,
    });
    eprintln!("{}", serde_json::to_string(&out).unwrap());
    std::process::exit(1);
}

/// Print a stub "not yet implemented" response for a command.
pub fn stub(command_name: &str, json_mode: bool) {
    if json_mode {
        json_output(json!({
            "status": "not_implemented",
            "command": command_name,
            "message": format!("{} is not yet implemented in Rust flowctl", command_name),
        }));
    } else {
        eprintln!(
            "flowctl {}: not yet implemented (Rust port in progress)",
            command_name
        );
        std::process::exit(1);
    }
}
