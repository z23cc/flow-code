//! Hook subcommands: auto-memory and ralph-guard.
//!
//! These are Claude Code hook scripts ported from Python to Rust.
//! They read JSON from stdin, perform validation/extraction, and use
//! exit codes 0 (allow) and 2 (block) per the hook protocol.

mod auto_memory;
mod commit_gate;
mod common;
mod compact;
mod ralph_guard;
mod rtk_rewrite;

use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum HookCmd {
    /// Extract session memories from transcript (Stop hook).
    AutoMemory,
    /// Enforce Ralph workflow rules (Pre/PostToolUse, Stop hooks).
    RalphGuard,
    /// Gate git commit on flowctl guard pass (Pre/PostToolUse hook).
    CommitGate,
    /// Inject .flow/ state into compaction context (PreCompact hook).
    PreCompact,
    /// Inject active task context for subagents (SubagentStart hook).
    SubagentContext,
    /// Sync Claude task completion with .flow/ state (TaskCompleted hook).
    TaskCompleted,
    /// Rewrite Bash commands via rtk token optimizer (PreToolUse hook).
    RtkRewrite,
}

pub fn dispatch(cmd: &HookCmd) {
    match cmd {
        HookCmd::AutoMemory => auto_memory::cmd_auto_memory(),
        HookCmd::RalphGuard => ralph_guard::cmd_ralph_guard(),
        HookCmd::CommitGate => commit_gate::cmd_commit_gate(),
        HookCmd::PreCompact => compact::cmd_pre_compact(),
        HookCmd::SubagentContext => compact::cmd_subagent_context(),
        HookCmd::TaskCompleted => compact::cmd_task_completed(),
        HookCmd::RtkRewrite => rtk_rewrite::cmd_rtk_rewrite(),
    }
}
