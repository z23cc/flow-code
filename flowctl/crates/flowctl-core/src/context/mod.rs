//! ContextAssembler — builds rich, self-contained work packages for each action.
//!
//! Reads codebase, code graph, knowledge, and constraints to provide the LLM
//! with everything it needs to complete a node — no searching required.

use std::path::Path;

use crate::domain::action_spec::{ActionContext, AttemptRef, FileSlice, PatternRef};
use crate::domain::goal::Goal;
use crate::domain::node::Node;
use crate::graph_store::CodeGraph;
use crate::knowledge::learner::Learner;
use crate::storage::attempt_store::AttemptStore;

/// Assembles complete context for a work action.
pub struct ContextAssembler {
    root: std::path::PathBuf,
}

impl ContextAssembler {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    /// Build a self-contained context for a node.
    pub fn assemble(&self, goal: &Goal, node: &Node) -> ActionContext {
        let files = self.gather_files(node);
        let patterns = self.inject_patterns(node);
        let constraints = self.compile_constraints(goal, node);
        let prior_attempts = self.load_attempts(&goal.id, &node.id);

        ActionContext {
            files,
            patterns,
            constraints,
            related_symbols: vec![],
            prior_attempts,
            guard_failures: vec![],
        }
    }

    /// Read owned files + dependent files from code graph + keyword-matched files.
    fn gather_files(&self, node: &Node) -> Vec<FileSlice> {
        let mut slices = Vec::new();

        // 1. Owned files: structure summary (progressive disclosure)
        for path in &node.owned_files {
            let full_path = self.root.join(path);
            if let Ok(content) = std::fs::read_to_string(&full_path) {
                let total_lines = content.lines().count();
                slices.push(FileSlice {
                    path: path.clone(),
                    content: truncate_content(&content, 50),
                    reason: "owned file — you will modify this. Use flow_query for full content.".into(),
                    lines: None,
                    total_lines: Some(total_lines),
                });
            }
        }

        // 2. Dependent files from code graph
        let graph_path = self.root.join(".flow").join("graph.bin");
        if graph_path.exists() {
            if let Ok(graph) = CodeGraph::load(&graph_path) {
                for path in &node.owned_files {
                    let impact = graph.find_impact(path);
                    for dep_path in impact.into_iter().take(5) {
                        if slices.iter().any(|f| f.path == dep_path) {
                            continue;
                        }
                        let full_path = self.root.join(&dep_path);
                        if let Ok(content) = std::fs::read_to_string(&full_path) {
                            slices.push(FileSlice {
                                path: dep_path,
                                content: truncate_content(&content, 20),
                                reason: "dependency — context only".into(),
                                lines: None,
                                total_lines: Some(content.lines().count()),
                            });
                        }
                    }
                }
            }
        }

        // 3. No more keyword walking — delegate file discovery to RepoPrompt.
        // The recommended_workflow in ActionSpec guides LLM to use RP file_search
        // and get_code_structure for richer, more precise context.

        slices
    }

    /// Inject relevant patterns from knowledge layer.
    fn inject_patterns(&self, node: &Node) -> Vec<PatternRef> {
        let flow_dir = self.root.join(".flow");
        let learner = Learner::new(&flow_dir);
        learner
            .inject_for_node(&node.objective, 5)
            .unwrap_or_default()
            .into_iter()
            .map(|p| PatternRef {
                name: p.name,
                approach: p.approach,
                confidence: p.confidence,
            })
            .collect()
    }

    /// Compile constraints from goal + node + project.
    fn compile_constraints(&self, goal: &Goal, node: &Node) -> Vec<String> {
        let mut constraints = Vec::new();

        // Node-level constraints
        constraints.extend(node.constraints.iter().cloned());

        // Goal-level constraints
        constraints.extend(goal.constraints.iter().cloned());

        // Project-level constraints from .flow/config.json
        let config_path = self.root.join(".flow").join("config.json");
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(cfg) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(rules) = cfg.get("constraints").and_then(|c| c.as_array()) {
                    for rule in rules {
                        if let Some(s) = rule.as_str() {
                            constraints.push(s.to_string());
                        }
                    }
                }
            }
        }

        constraints
    }

    /// Load prior attempts for a node.
    fn load_attempts(&self, goal_id: &str, node_id: &str) -> Vec<AttemptRef> {
        let flow_dir = self.root.join(".flow");
        let store = AttemptStore::new(&flow_dir);
        store
            .list_for_node(goal_id, node_id)
            .unwrap_or_default()
            .into_iter()
            .map(|a| AttemptRef {
                seq: a.seq,
                summary: a.summary,
                status: a.status,
            })
            .collect()
    }
}

/// Truncate content to a max number of lines, keeping the first N lines.
fn truncate_content(content: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() <= max_lines {
        content.to_string()
    } else {
        let mut result: String = lines[..max_lines].join("\n");
        result.push_str(&format!("\n// ... {} more lines", lines.len() - max_lines));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::goal::Goal;
    use crate::domain::node::Node;
    use tempfile::TempDir;

    fn make_goal() -> Goal {
        Goal::new("g-test".into(), "test goal".into(),
            crate::domain::goal::PlanningMode::Direct,
            crate::domain::goal::SuccessModel::Criteria)
    }

    #[test]
    fn test_assemble_with_owned_files() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".flow/knowledge")).unwrap();
        std::fs::write(tmp.path().join("hello.rs"), "fn main() {}").unwrap();

        let asm = ContextAssembler::new(tmp.path());
        let goal = make_goal();
        let mut node = Node::new("n-1".into(), "test".into());
        node.owned_files = vec!["hello.rs".into()];

        let ctx = asm.assemble(&goal, &node);
        assert_eq!(ctx.files.len(), 1);
        assert_eq!(ctx.files[0].path, "hello.rs");
        assert!(ctx.files[0].content.contains("fn main"));
    }

    #[test]
    fn test_assemble_missing_file_skipped() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".flow/knowledge")).unwrap();

        let asm = ContextAssembler::new(tmp.path());
        let goal = make_goal();
        let mut node = Node::new("n-1".into(), "test".into());
        node.owned_files = vec!["nonexistent.rs".into()];

        let ctx = asm.assemble(&goal, &node);
        assert_eq!(ctx.files.len(), 0);
    }

    #[test]
    fn test_assemble_compiles_constraints() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".flow/knowledge")).unwrap();

        let asm = ContextAssembler::new(tmp.path());
        let mut goal = make_goal();
        goal.constraints = vec!["no unsafe".into()];
        let mut node = Node::new("n-1".into(), "test".into());
        node.constraints = vec!["use thiserror".into()];

        let ctx = asm.assemble(&goal, &node);
        assert!(ctx.constraints.contains(&"use thiserror".to_string()));
        assert!(ctx.constraints.contains(&"no unsafe".to_string()));
    }

    #[test]
    fn test_truncate_content() {
        let content = "line1\nline2\nline3\nline4\nline5";
        let truncated = truncate_content(content, 3);
        assert!(truncated.contains("line1"));
        assert!(truncated.contains("line3"));
        assert!(truncated.contains("2 more lines"));
        assert!(!truncated.contains("line4"));
    }

    #[test]
    fn test_no_owned_files_returns_empty() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".flow/knowledge")).unwrap();

        let asm = ContextAssembler::new(tmp.path());
        let goal = make_goal();
        let node = Node::new("n-1".into(), "add email verification".into());

        let ctx = asm.assemble(&goal, &node);
        // Without owned_files, context.files is empty — RP handles discovery
        assert!(ctx.files.is_empty());
    }
}
