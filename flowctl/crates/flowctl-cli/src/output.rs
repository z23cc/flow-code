//! JSON output helpers for consistent CLI responses.

use serde_json::{json, Value};

/// Shared output options (flattened into every subcommand via clap).
#[derive(clap::Args, Debug, Clone)]
pub struct OutputOpts {
    /// Output as JSON.
    #[arg(long, global = true)]
    pub json: bool,
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
    println!("{}", serde_json::to_string(&Value::Object(obj)).unwrap());
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
