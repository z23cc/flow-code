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
    "branch_name",
    "plan_review_status",
    "plan_reviewed_at",
    "completion_review_status",
    "completion_reviewed_at",
    "default_impl",
    "default_review",
    "default_sync",
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

/// Is compact mode currently active?
pub fn is_compact() -> bool {
    COMPACT.load(Ordering::Relaxed)
}

/// Print pretty (non-JSON) output, routed through a built-in compress filter when
/// compact mode is active. Falls back to the raw text if the named filter does not
/// exist (backward-compat — passthrough on miss).
///
/// The `filter_name` must correspond to a filter embedded in `flowctl-core/src/filters/`.
pub fn pretty_output(filter_name: &str, text: &str) {
    if is_compact() {
        if let Some(filtered) = flowctl_core::compress::apply_filter(filter_name, text) {
            if filtered.is_empty() {
                // empty filter output — nothing to print
                return;
            }
            println!("{}", filtered);
            return;
        }
    }
    print!("{}", text);
    if !text.ends_with('\n') {
        println!();
    }
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
