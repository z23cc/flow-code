//! JSON output helpers for consistent CLI responses.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

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

/// Convert an array of objects to columnar format: {"headers": [...], "rows": [[...], ...]}.
/// Saves ~80% tokens on lists with 10+ items by eliminating key repetition.
fn to_columnar(arr: &[Value]) -> Option<Value> {
    if arr.is_empty() {
        return None;
    }
    // Extract headers from the first object
    let first = arr.first()?.as_object()?;
    let headers: Vec<String> = first.keys().cloned().collect();
    let rows: Vec<Vec<Value>> = arr
        .iter()
        .map(|item| {
            headers
                .iter()
                .map(|h| item.get(h).cloned().unwrap_or(Value::Null))
                .collect()
        })
        .collect();
    Some(json!({
        "headers": headers,
        "rows": rows,
        "count": rows.len(),
    }))
}

/// Output stability contract version. Incremented on breaking changes to JSON field names.
const API_VERSION: u32 = 1;

/// Print a successful JSON response with additional data fields merged in.
/// In compact mode, arrays of 10+ objects are auto-converted to columnar format.
pub fn json_output(data: Value) {
    let mut obj = match data {
        Value::Object(map) => map,
        Value::Array(ref arr)
            if is_compact() && arr.len() >= 10 && arr.first().map_or(false, |v| v.is_object()) =>
        {
            let mut m = serde_json::Map::new();
            if let Some(columnar) = to_columnar(arr) {
                m.insert("data".to_string(), columnar);
                m.insert("format".to_string(), json!("columnar"));
            } else {
                m.insert("data".to_string(), data);
            }
            m
        }
        _ => {
            let mut m = serde_json::Map::new();
            m.insert("data".to_string(), data);
            m
        }
    };
    obj.insert("api_version".to_string(), json!(API_VERSION));
    obj.insert("success".to_string(), json!(true));
    let mut val = Value::Object(obj);
    if COMPACT.load(Ordering::Relaxed) {
        strip_compact(&mut val);
    }
    println!(
        "{}",
        serde_json::to_string(&val).expect("JSON serialization of output value should not fail")
    );
}

/// Print an error JSON response and exit with code 1.
#[allow(dead_code)]
pub fn error_exit(message: &str) -> ! {
    let out = json!({
        "api_version": API_VERSION,
        "success": false,
        "error": message,
    });
    eprintln!(
        "{}",
        serde_json::to_string(&out).expect("JSON serialization of error output should not fail")
    );
    std::process::exit(1);
}

/// Print an error JSON response and exit with code 2 (blocked).
#[allow(dead_code)]
pub fn blocked_exit(message: &str) -> ! {
    let out = json!({
        "api_version": API_VERSION,
        "success": false,
        "error": message,
    });
    eprintln!(
        "{}",
        serde_json::to_string(&out).expect("JSON serialization of blocked output should not fail")
    );
    std::process::exit(2);
}
