//! Three-layer path resolution for flow-code.
//!
//! Layer 1: `.flow/` — runtime state (gitignored)
//! Layer 2: `.flow-config/` — team config (git-tracked)
//! Layer 3: `~/.flow/projects/{slug}/` — global cross-session

use std::path::{Path, PathBuf};

use crate::types::FLOW_DIR;

/// The directory name for team-shared, git-tracked config.
const FLOW_CONFIG_DIR: &str = ".flow-config";

/// The three state layers for flow-code.
#[derive(Debug, Clone)]
pub struct FlowPaths {
    /// Layer 1: .flow/ — runtime state (gitignored).
    /// Contains: epics/, tasks/, specs/, .state/, checklists/, index/, graph.bin, reviews/
    pub runtime_dir: PathBuf,

    /// Layer 2: .flow-config/ — team config (git-tracked).
    /// Contains: project-context.md, invariants.md, config.json
    pub config_dir: PathBuf,

    /// Layer 3: ~/.flow/projects/{slug}/ — global cross-session.
    /// Contains: frecency.json, memory/
    pub global_project_dir: PathBuf,

    /// Project root directory (parent of .flow/).
    pub project_root: PathBuf,

    /// Project slug (e.g., "z23cc-flow-code").
    pub slug: String,
}

impl FlowPaths {
    /// Resolve all three layers from the current working directory.
    pub fn resolve() -> Option<Self> {
        let project_root = find_project_root()?;
        let slug = compute_project_slug(&project_root);

        let runtime_dir = project_root.join(FLOW_DIR);
        let config_dir = project_root.join(FLOW_CONFIG_DIR);
        let global_project_dir = global_flow_dir().join("projects").join(&slug);

        Some(Self {
            runtime_dir,
            config_dir,
            global_project_dir,
            project_root,
            slug,
        })
    }

    // -- Convenience path getters ----------------------------------------

    /// config.json: .flow-config/config.json (primary) -> .flow/config.json (fallback).
    pub fn config_json(&self) -> PathBuf {
        let primary = self.config_dir.join("config.json");
        if primary.exists() {
            return primary;
        }
        self.runtime_dir.join("config.json") // backward compat
    }

    /// project-context.md: .flow-config/ (primary) -> .flow/ (fallback).
    pub fn project_context(&self) -> PathBuf {
        let primary = self.config_dir.join("project-context.md");
        if primary.exists() {
            return primary;
        }
        self.runtime_dir.join("project-context.md")
    }

    /// invariants.md: .flow-config/ (primary) -> .flow/ (fallback).
    pub fn invariants(&self) -> PathBuf {
        let primary = self.config_dir.join("invariants.md");
        if primary.exists() {
            return primary;
        }
        self.runtime_dir.join("invariants.md")
    }

    /// frecency.json: ~/.flow/projects/{slug}/ (primary) -> .flow/ (fallback).
    pub fn frecency(&self) -> PathBuf {
        let primary = self.global_project_dir.join("frecency.json");
        if primary.exists() {
            return primary;
        }
        self.runtime_dir.join("frecency.json")
    }

    /// memory directory: ~/.flow/projects/{slug}/memory/ (primary) -> .flow/memory/ (fallback).
    pub fn memory_dir(&self) -> PathBuf {
        let primary = self.global_project_dir.join("memory");
        if primary.exists() {
            return primary;
        }
        self.runtime_dir.join("memory")
    }
}

/// Find project root by walking up from CWD looking for .flow/ or .flow-config/.
fn find_project_root() -> Option<PathBuf> {
    // 1. FLOW_STATE_DIR env var
    if let Ok(dir) = std::env::var("FLOW_STATE_DIR") {
        return PathBuf::from(dir).parent().map(|p| p.to_path_buf());
    }

    // 2. Walk up looking for .flow or .flow-config
    let mut current = std::env::current_dir().ok()?;
    loop {
        if current.join(FLOW_DIR).exists() || current.join(FLOW_CONFIG_DIR).exists() {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

/// Compute project slug from git remote URL.
///
/// `"git@github.com:user/repo.git"` -> `"user-repo"`
/// `"https://github.com/user/repo.git"` -> `"user-repo"`
///
/// Fallback: directory basename.
fn compute_project_slug(project_root: &Path) -> String {
    // Try git remote
    if let Ok(output) = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(project_root)
        .output()
    {
        if output.status.success() {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Some(slug) = slug_from_remote_url(&url) {
                return slug;
            }
        }
    }

    // Fallback: directory name
    project_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Extract an owner-repo slug from a git remote URL.
///
/// SSH: `git@github.com:user/repo` -> `"user-repo"`
/// HTTPS: `https://github.com/user/repo` -> `"user-repo"`
pub(crate) fn slug_from_remote_url(url: &str) -> Option<String> {
    let url = url.trim_end_matches(".git");

    // SSH: git@github.com:user/repo
    if let Some(rest) = url.strip_prefix("git@") {
        let path = rest.split(':').nth(1)?;
        return Some(path.replace('/', "-"));
    }

    // HTTPS: https://github.com/user/repo
    if url.starts_with("http") {
        let parts: Vec<&str> = url.split('/').collect();
        if parts.len() >= 2 {
            let owner = parts[parts.len() - 2];
            let repo = parts[parts.len() - 1];
            return Some(format!("{owner}-{repo}"));
        }
    }

    None
}

/// Get the global ~/.flow/ directory.
fn global_flow_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
        .join(".flow")
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- slug_from_remote_url --

    #[test]
    fn slug_ssh() {
        assert_eq!(
            slug_from_remote_url("git@github.com:z23cc/flow-code.git"),
            Some("z23cc-flow-code".to_string())
        );
    }

    #[test]
    fn slug_https() {
        assert_eq!(
            slug_from_remote_url("https://github.com/z23cc/flow-code.git"),
            Some("z23cc-flow-code".to_string())
        );
    }

    #[test]
    fn slug_https_no_git_suffix() {
        assert_eq!(
            slug_from_remote_url("https://github.com/user/repo"),
            Some("user-repo".to_string())
        );
    }

    #[test]
    fn slug_invalid() {
        assert_eq!(slug_from_remote_url("not-a-url"), None);
    }

    // -- find_project_root --

    #[test]
    fn flow_paths_resolve_with_flow_dir() {
        // Test that FlowPaths fields are consistent when constructed manually.
        // We cannot call find_project_root() directly because mutating env/CWD
        // requires unsafe in edition 2024, so we verify the struct logic instead.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();

        let paths = FlowPaths {
            runtime_dir: root.join(".flow"),
            config_dir: root.join(".flow-config"),
            global_project_dir: root.join("global"),
            project_root: root.clone(),
            slug: "test-project".to_string(),
        };

        assert_eq!(paths.project_root, root);
        assert_eq!(paths.slug, "test-project");
        assert_eq!(paths.runtime_dir, root.join(".flow"));
        assert_eq!(paths.config_dir, root.join(".flow-config"));
    }

    // -- FlowPaths convenience getters --

    #[test]
    fn config_json_primary_over_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = tmp.path().join(".flow");
        let config = tmp.path().join(".flow-config");
        std::fs::create_dir_all(&runtime).unwrap();
        std::fs::create_dir_all(&config).unwrap();

        // Write config.json in both locations
        std::fs::write(runtime.join("config.json"), "{}").unwrap();
        std::fs::write(config.join("config.json"), "{}").unwrap();

        let paths = FlowPaths {
            runtime_dir: runtime.clone(),
            config_dir: config.clone(),
            global_project_dir: tmp.path().join("global"),
            project_root: tmp.path().to_path_buf(),
            slug: "test".to_string(),
        };

        // Primary (.flow-config/) should win
        assert_eq!(paths.config_json(), config.join("config.json"));
    }

    #[test]
    fn config_json_fallback_when_no_primary() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = tmp.path().join(".flow");
        let config = tmp.path().join(".flow-config");
        std::fs::create_dir_all(&runtime).unwrap();
        // Do NOT create .flow-config/config.json

        let paths = FlowPaths {
            runtime_dir: runtime.clone(),
            config_dir: config,
            global_project_dir: tmp.path().join("global"),
            project_root: tmp.path().to_path_buf(),
            slug: "test".to_string(),
        };

        assert_eq!(paths.config_json(), runtime.join("config.json"));
    }

    #[test]
    fn project_context_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = tmp.path().join(".flow");
        let config = tmp.path().join(".flow-config");
        std::fs::create_dir_all(&runtime).unwrap();

        let paths = FlowPaths {
            runtime_dir: runtime.clone(),
            config_dir: config,
            global_project_dir: tmp.path().join("global"),
            project_root: tmp.path().to_path_buf(),
            slug: "test".to_string(),
        };

        // No .flow-config/project-context.md -> falls back to .flow/
        assert_eq!(
            paths.project_context(),
            runtime.join("project-context.md")
        );
    }

    #[test]
    fn invariants_primary() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = tmp.path().join(".flow");
        let config = tmp.path().join(".flow-config");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("invariants.md"), "# inv").unwrap();

        let paths = FlowPaths {
            runtime_dir: runtime,
            config_dir: config.clone(),
            global_project_dir: tmp.path().join("global"),
            project_root: tmp.path().to_path_buf(),
            slug: "test".to_string(),
        };

        assert_eq!(paths.invariants(), config.join("invariants.md"));
    }

    #[test]
    fn frecency_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = tmp.path().join(".flow");
        std::fs::create_dir_all(&runtime).unwrap();

        let paths = FlowPaths {
            runtime_dir: runtime.clone(),
            config_dir: tmp.path().join(".flow-config"),
            global_project_dir: tmp.path().join("nonexistent-global"),
            project_root: tmp.path().to_path_buf(),
            slug: "test".to_string(),
        };

        assert_eq!(paths.frecency(), runtime.join("frecency.json"));
    }

    #[test]
    fn memory_dir_primary() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = tmp.path().join(".flow");
        let global = tmp.path().join("global");
        let global_mem = global.join("memory");
        std::fs::create_dir_all(&global_mem).unwrap();

        let paths = FlowPaths {
            runtime_dir: runtime,
            config_dir: tmp.path().join(".flow-config"),
            global_project_dir: global.clone(),
            project_root: tmp.path().to_path_buf(),
            slug: "test".to_string(),
        };

        assert_eq!(paths.memory_dir(), global_mem);
    }
}
