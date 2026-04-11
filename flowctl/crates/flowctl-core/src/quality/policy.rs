//! PolicyEngine — unified governance with physical enforcement.
//!
//! Two adapters: MCP internal (check_mcp) and PreToolUse hook (check_hook).
//! Claude can bypass MCP by calling Edit/Write/Bash directly — the hook adapter
//! catches those cases.

use serde::{Deserialize, Serialize};

/// Policy decision — Block, Warn, or Allow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    Warn(String),
    Block(String),
}

/// A policy rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub id: String,
    pub condition: PolicyCondition,
    pub action: PolicyAction,
    pub reason: String,
}

/// Conditions that trigger a policy check.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PolicyCondition {
    EditLockedFile { file_pattern: String },
    CommitWithoutGuard,
    WriteReceiptBeforeReview,
    DirectStateEdit,
}

/// Policy enforcement action.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    Block { message: String },
    Warn { message: String },
    Allow,
}

/// Context for a policy check.
#[derive(Debug, Clone)]
pub struct PolicyContext {
    pub active_locks: Vec<FileLock>,
    pub guard_ran: bool,
    pub current_node: Option<String>,
}

/// A file lock entry for policy checking.
#[derive(Debug, Clone)]
pub struct FileLock {
    pub file_path: String,
    pub node_id: String,
    pub mode: String,
}

/// The PolicyEngine evaluates rules against context.
pub struct PolicyEngine {
    #[allow(dead_code)]
    rules: Vec<PolicyRule>,
}

impl PolicyEngine {
    pub fn new() -> Self {
        Self {
            rules: Self::default_rules(),
        }
    }

    /// Default rules for V3.
    fn default_rules() -> Vec<PolicyRule> {
        vec![
            PolicyRule {
                id: "lock-check".into(),
                condition: PolicyCondition::EditLockedFile {
                    file_pattern: "*".into(),
                },
                action: PolicyAction::Block {
                    message: "File is locked by another node".into(),
                },
                reason: "Prevent concurrent edits to locked files".into(),
            },
            PolicyRule {
                id: "state-protection".into(),
                condition: PolicyCondition::DirectStateEdit,
                action: PolicyAction::Block {
                    message: "Direct .flow/ state edits are not allowed — use flowctl commands".into(),
                },
                reason: "State consistency requires going through flowctl".into(),
            },
        ]
    }

    /// Check MCP tool call against policies.
    pub fn check_mcp(&self, tool_name: &str, node_id: Option<&str>, context: &PolicyContext) -> PolicyDecision {
        // Check lock violations for node operations
        if tool_name == "node.start" || tool_name == "node.finish" {
            if let Some(current) = node_id {
                for lock in &context.active_locks {
                    if lock.node_id != current {
                        return PolicyDecision::Block(format!(
                            "File {} is locked by node {}",
                            lock.file_path, lock.node_id
                        ));
                    }
                }
            }
        }
        PolicyDecision::Allow
    }

    /// Check hook (PreToolUse) — validates Edit/Write/Bash calls.
    pub fn check_hook(&self, tool_name: &str, file_path: Option<&str>, context: &PolicyContext) -> PolicyDecision {
        // Block direct .flow/ state edits
        if let Some(path) = file_path {
            if path.contains(".flow/") && (path.ends_with(".json") || path.ends_with(".jsonl")) {
                if tool_name == "Edit" || tool_name == "Write" {
                    return PolicyDecision::Block(
                        "Direct .flow/ state edits are not allowed — use flowctl commands".into(),
                    );
                }
            }

            // Check file locks
            for lock in &context.active_locks {
                if path.ends_with(&lock.file_path) || lock.file_path.ends_with(path) {
                    if let Some(current) = &context.current_node {
                        if &lock.node_id != current {
                            return PolicyDecision::Block(format!(
                                "File {} is locked by node {} — you are node {}",
                                path, lock.node_id, current
                            ));
                        }
                    }
                }
            }
        }

        PolicyDecision::Allow
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with_lock(file: &str, node: &str) -> PolicyContext {
        PolicyContext {
            active_locks: vec![FileLock {
                file_path: file.into(),
                node_id: node.into(),
                mode: "write".into(),
            }],
            guard_ran: false,
            current_node: None,
        }
    }

    #[test]
    fn test_hook_blocks_state_edit() {
        let engine = PolicyEngine::new();
        let ctx = PolicyContext {
            active_locks: vec![],
            guard_ran: false,
            current_node: None,
        };
        let result = engine.check_hook("Write", Some(".flow/goals/g-1/goal.json"), &ctx);
        assert!(matches!(result, PolicyDecision::Block(_)));
    }

    #[test]
    fn test_hook_allows_normal_edit() {
        let engine = PolicyEngine::new();
        let ctx = PolicyContext {
            active_locks: vec![],
            guard_ran: false,
            current_node: None,
        };
        let result = engine.check_hook("Edit", Some("src/main.rs"), &ctx);
        assert_eq!(result, PolicyDecision::Allow);
    }

    #[test]
    fn test_hook_blocks_locked_file() {
        let engine = PolicyEngine::new();
        let ctx = ctx_with_lock("src/auth.rs", "n-1");
        let mut ctx_with_node = ctx;
        ctx_with_node.current_node = Some("n-2".into());
        let result = engine.check_hook("Edit", Some("src/auth.rs"), &ctx_with_node);
        assert!(matches!(result, PolicyDecision::Block(_)));
    }

    #[test]
    fn test_hook_allows_own_locked_file() {
        let engine = PolicyEngine::new();
        let mut ctx = ctx_with_lock("src/auth.rs", "n-1");
        ctx.current_node = Some("n-1".into());
        let result = engine.check_hook("Edit", Some("src/auth.rs"), &ctx);
        assert_eq!(result, PolicyDecision::Allow);
    }
}
