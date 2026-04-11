//! GuardRunner — internalized quality gate execution.
//!
//! Runs lint/test/typecheck commands at risk-proportional depth.
//! No subprocess spawning of flowctl — runs commands directly.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::domain::action_spec::GuardCommandResult;
use crate::domain::node::GuardDepth;

/// A guard command with its required depth level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardCommand {
    pub name: String,
    pub command: String,
    pub depth: GuardDepth,
}

/// Result of running all applicable guard commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardResult {
    pub passed: bool,
    pub results: Vec<GuardCommandResult>,
    pub depth: String,
}

/// Runs quality guard checks at risk-proportional depth.
pub struct GuardRunner {
    commands: Vec<GuardCommand>,
    root: PathBuf,
}

impl GuardRunner {
    pub fn new(root: &Path) -> Self {
        let commands = Self::load_commands(root);
        Self {
            commands,
            root: root.to_path_buf(),
        }
    }

    /// Run guard at a specific depth. Returns pass/fail with details.
    pub fn run(&self, depth: GuardDepth) -> GuardResult {
        let applicable: Vec<_> = self.commands.iter()
            .filter(|c| c.depth <= depth)
            .collect();

        let mut results = Vec::new();
        for cmd in &applicable {
            let output = Command::new("sh")
                .arg("-c")
                .arg(&cmd.command)
                .current_dir(&self.root)
                .output();

            results.push(GuardCommandResult {
                command: cmd.name.clone(),
                passed: output.as_ref().map(|o| o.status.success()).unwrap_or(false),
                stdout: output.as_ref()
                    .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                    .unwrap_or_default(),
                stderr: output.as_ref()
                    .map(|o| String::from_utf8_lossy(&o.stderr).to_string())
                    .unwrap_or_default(),
            });
        }

        let depth_name = match depth {
            GuardDepth::Trivial => "trivial",
            GuardDepth::Standard => "standard",
            GuardDepth::Thorough => "thorough",
        };

        GuardResult {
            passed: results.iter().all(|r| r.passed),
            results,
            depth: depth_name.to_string(),
        }
    }

    /// Get the list of commands that would run at a given depth.
    pub fn commands_for_depth(&self, depth: GuardDepth) -> Vec<String> {
        self.commands.iter()
            .filter(|c| c.depth <= depth)
            .map(|c| c.command.clone())
            .collect()
    }

    /// Load guard commands from project config or use sensible defaults.
    fn load_commands(root: &Path) -> Vec<GuardCommand> {
        // Try to read from .flow/config.json guard section
        let config_path = root.join(".flow").join("config.json");
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(cfg) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(guards) = cfg.get("guard").and_then(|g| g.as_array()) {
                    let mut commands = Vec::new();
                    for g in guards {
                        if let (Some(name), Some(cmd), depth) = (
                            g.get("name").and_then(|v| v.as_str()),
                            g.get("command").and_then(|v| v.as_str()),
                            g.get("depth").and_then(|v| v.as_str()),
                        ) {
                            commands.push(GuardCommand {
                                name: name.to_string(),
                                command: cmd.to_string(),
                                depth: match depth {
                                    Some("thorough") => GuardDepth::Thorough,
                                    Some("trivial") => GuardDepth::Trivial,
                                    _ => GuardDepth::Standard,
                                },
                            });
                        }
                    }
                    if !commands.is_empty() {
                        return commands;
                    }
                }
            }
        }

        // Auto-detect from project structure
        Self::detect_defaults(root)
    }

    /// Create with explicit commands (for testing).
    pub fn with_commands(root: &Path, commands: Vec<GuardCommand>) -> Self {
        Self { commands, root: root.to_path_buf() }
    }

    fn detect_defaults(root: &Path) -> Vec<GuardCommand> {
        let mut commands = Vec::new();

        // Rust project — check root and common subdirs
        let cargo_dir = Self::find_project_file(root, "Cargo.toml");
        if let Some(dir) = cargo_dir {
            let prefix = if dir == root.to_path_buf() {
                String::new()
            } else {
                let rel = dir.strip_prefix(root).unwrap_or(&dir);
                format!("cd {} && ", rel.display())
            };
            commands.push(GuardCommand {
                name: "cargo build".into(),
                command: format!("{prefix}cargo build --all 2>&1"),
                depth: GuardDepth::Trivial,
            });
            commands.push(GuardCommand {
                name: "cargo test".into(),
                command: format!("{prefix}cargo test --all 2>&1"),
                depth: GuardDepth::Standard,
            });
            commands.push(GuardCommand {
                name: "cargo clippy".into(),
                command: format!("{prefix}cargo clippy --all-targets -- -D warnings 2>&1"),
                depth: GuardDepth::Thorough,
            });
        }

        // Node/TypeScript project
        let pkg_dir = Self::find_project_file(root, "package.json");
        if let Some(dir) = pkg_dir {
            let prefix = if dir == root.to_path_buf() {
                String::new()
            } else {
                let rel = dir.strip_prefix(root).unwrap_or(&dir);
                format!("cd {} && ", rel.display())
            };
            commands.push(GuardCommand {
                name: "npm test".into(),
                command: format!("{prefix}npm test 2>&1"),
                depth: GuardDepth::Standard,
            });
            if dir.join("tsconfig.json").exists() {
                commands.push(GuardCommand {
                    name: "tsc".into(),
                    command: format!("{prefix}npx tsc --noEmit 2>&1"),
                    depth: GuardDepth::Standard,
                });
            }
        }

        // Python project
        if root.join("pyproject.toml").exists() || root.join("setup.py").exists() {
            commands.push(GuardCommand {
                name: "pytest".into(),
                command: "python -m pytest 2>&1".into(),
                depth: GuardDepth::Standard,
            });
        }

        commands
    }

    /// Find a project marker file in root or one level of subdirectories.
    fn find_project_file(root: &Path, filename: &str) -> Option<PathBuf> {
        // Check root first
        if root.join(filename).exists() {
            return Some(root.to_path_buf());
        }
        // Check immediate subdirs
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join(filename).exists() {
                    // Skip hidden dirs and common non-project dirs
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if !name_str.starts_with('.') && name_str != "node_modules" && name_str != "target" {
                        return Some(path);
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_guard_runner_pass() {
        let tmp = TempDir::new().unwrap();
        let guard = GuardRunner::with_commands(tmp.path(), vec![
            GuardCommand {
                name: "true".into(),
                command: "true".into(),
                depth: GuardDepth::Trivial,
            },
        ]);
        let result = guard.run(GuardDepth::Standard);
        assert!(result.passed);
        assert_eq!(result.results.len(), 1);
        assert!(result.results[0].passed);
    }

    #[test]
    fn test_guard_runner_fail() {
        let tmp = TempDir::new().unwrap();
        let guard = GuardRunner::with_commands(tmp.path(), vec![
            GuardCommand {
                name: "false".into(),
                command: "false".into(),
                depth: GuardDepth::Standard,
            },
        ]);
        let result = guard.run(GuardDepth::Standard);
        assert!(!result.passed);
        assert!(!result.results[0].passed);
    }

    #[test]
    fn test_guard_depth_filtering() {
        let tmp = TempDir::new().unwrap();
        let guard = GuardRunner::with_commands(tmp.path(), vec![
            GuardCommand { name: "trivial".into(), command: "true".into(), depth: GuardDepth::Trivial },
            GuardCommand { name: "standard".into(), command: "true".into(), depth: GuardDepth::Standard },
            GuardCommand { name: "thorough".into(), command: "true".into(), depth: GuardDepth::Thorough },
        ]);
        // Trivial depth should only run trivial commands
        assert_eq!(guard.commands_for_depth(GuardDepth::Trivial).len(), 1);
        // Standard should run trivial + standard
        assert_eq!(guard.commands_for_depth(GuardDepth::Standard).len(), 2);
        // Thorough should run all
        assert_eq!(guard.commands_for_depth(GuardDepth::Thorough).len(), 3);
    }

    #[test]
    fn test_detect_defaults_rust_project() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        let commands = GuardRunner::detect_defaults(tmp.path());
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.name.contains("cargo")));
    }

    #[test]
    fn test_detect_defaults_rust_subdir() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("backend");
        std::fs::create_dir_all(&subdir).unwrap();
        std::fs::write(subdir.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        let commands = GuardRunner::detect_defaults(tmp.path());
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.command.contains("cd backend")));
    }

    #[test]
    fn test_find_project_file_root() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "").unwrap();
        let found = GuardRunner::find_project_file(tmp.path(), "Cargo.toml");
        assert_eq!(found, Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn test_find_project_file_subdir() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("myapp");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("package.json"), "{}").unwrap();
        let found = GuardRunner::find_project_file(tmp.path(), "package.json");
        assert_eq!(found, Some(sub));
    }
}
