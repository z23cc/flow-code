//! `flowctl code-structure` command: extract symbol definitions from files.

use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output, pretty_output};

#[derive(Subcommand, Debug)]
pub enum CodeStructureCmd {
    /// Extract symbols from a file or directory.
    Extract {
        /// Path to a file or directory (default: current directory).
        #[arg(long, default_value = ".")]
        path: String,
    },
}

pub fn dispatch(cmd: &CodeStructureCmd, json: bool) {
    match cmd {
        CodeStructureCmd::Extract { path } => cmd_extract(json, path),
    }
}

fn cmd_extract(json_mode: bool, path: &str) {
    let p = std::path::Path::new(path);
    if !p.exists() {
        error_exit(&format!("Path not found: {path}"));
    }

    let symbols = if p.is_file() {
        match flowctl_core::code_structure::extract_symbols(p) {
            Ok(s) => s,
            Err(e) => error_exit(&format!("Extraction failed: {e}")),
        }
    } else {
        match flowctl_core::code_structure::extract_all_symbols(p) {
            Ok(s) => s,
            Err(e) => error_exit(&format!("Extraction failed: {e}")),
        }
    };

    if json_mode {
        let items: Vec<serde_json::Value> = symbols
            .iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "kind": s.kind,
                    "file": s.file,
                    "line": s.line,
                    "signature": s.signature,
                })
            })
            .collect();
        json_output(json!({
            "symbols": items,
            "count": items.len(),
        }));
    } else {
        let mut current_file = String::new();
        let mut text = String::new();
        for s in &symbols {
            if s.file != current_file {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&format!("{}:\n", s.file));
                current_file = s.file.clone();
            }
            text.push_str(&format!("  L{}: [{}] {}\n", s.line, s.kind, s.signature));
        }
        if text.is_empty() {
            text = String::from("No symbols found.\n");
        }
        pretty_output("code_structure", &text);
    }
}
