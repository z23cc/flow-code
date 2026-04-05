//! Outputs layer: lightweight narrative handoff between worker tasks.
//!
//! Stored as `.flow/outputs/<task-id>.md` files containing `## Summary` /
//! `## Surprises` / `## Decisions` sections. This is separate from the
//! verified memory system — outputs is a lightweight, file-native narrative
//! layer gated on its own `outputs.enabled` config key.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// An entry in the outputs store — pointer + metadata for a `.flow/outputs/*.md` file.
///
/// Per memory convention #008: protocol types live in flowctl-core so all
/// transport layers (CLI, daemon, MCP) share the same shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputEntry {
    /// Task ID (e.g. `fn-20-abf.2`).
    pub task_id: String,
    /// Absolute path to the output markdown file.
    pub path: PathBuf,
    /// File mtime as seconds since UNIX epoch.
    pub mtime: u64,
}
