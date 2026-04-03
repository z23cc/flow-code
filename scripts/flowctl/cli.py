"""
CLI entry point for flowctl — argparse setup and command dispatch.

All command handlers are imported from flowctl.commands.* modules.
"""

import argparse

from flowctl.commands.stack import (
    cmd_invariants_show,
    cmd_invariants_init,
    cmd_invariants_check,
    cmd_guard,
    cmd_stack_detect,
    cmd_stack_set,
    cmd_stack_show,
)
from flowctl.commands.admin import (
    cmd_init,
    cmd_detect,
    cmd_doctor,
    cmd_status,
    cmd_ralph_pause,
    cmd_ralph_resume,
    cmd_ralph_stop,
    cmd_ralph_status,
    cmd_config_get,
    cmd_config_set,
    cmd_review_backend,
    cmd_validate,
)
from flowctl.commands.gap import (
    cmd_gap_add,
    cmd_gap_list,
    cmd_gap_resolve,
    cmd_gap_check,
)
from flowctl.commands.findings import cmd_parse_findings
from flowctl.commands.epic import (
    cmd_epic_create,
    cmd_epic_set_plan,
    cmd_epic_set_plan_review_status,
    cmd_epic_set_completion_review_status,
    cmd_epic_set_branch,
    cmd_epic_set_title,
    cmd_epic_add_dep,
    cmd_epic_rm_dep,
    cmd_epic_set_backend,
    cmd_epic_close,
    cmd_epic_reopen,
    cmd_epic_archive,
    cmd_epic_clean,
)
from flowctl.commands.task import (
    cmd_task_create,
    cmd_dep_add,
    cmd_task_set_deps,
    cmd_task_set_backend,
    cmd_task_show_backend,
    cmd_task_set_description,
    cmd_task_set_acceptance,
    cmd_task_set_spec,
    cmd_task_reset,
    cmd_dep_rm,
    cmd_task_skip,
    cmd_task_split,
)
from flowctl.commands.workflow import (
    cmd_ready,
    cmd_next,
    cmd_queue,
    cmd_start,
    cmd_done,
    cmd_block,
    cmd_restart,
    cmd_state_path,
    cmd_migrate_state,
)
from flowctl.commands.query import (
    cmd_show,
    cmd_epics,
    cmd_files,
    cmd_tasks,
    cmd_list,
    cmd_cat,
    cmd_lock,
    cmd_unlock,
    cmd_lock_check,
)
from flowctl.commands.memory import (
    cmd_memory_init,
    cmd_memory_add,
    cmd_memory_read,
    cmd_memory_list,
    cmd_memory_search,
    cmd_memory_inject,
    cmd_memory_verify,
    cmd_memory_gc,
)
from flowctl.commands.rp import (
    cmd_prep_chat,
    cmd_rp_windows,
    cmd_rp_pick_window,
    cmd_rp_ensure_workspace,
    cmd_rp_builder,
    cmd_rp_prompt_get,
    cmd_rp_prompt_set,
    cmd_rp_select_get,
    cmd_rp_select_add,
    cmd_rp_chat_send,
    cmd_rp_prompt_export,
    cmd_rp_setup_review,
)
from flowctl.commands.review import (
    CODEX_EFFORT_LEVELS,
    cmd_codex_check,
    cmd_codex_impl_review,
    cmd_codex_plan_review,
    cmd_codex_completion_review,
    cmd_codex_adversarial,
    cmd_checkpoint_save,
    cmd_checkpoint_restore,
    cmd_checkpoint_delete,
)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="flowctl - CLI for .flow/ task tracking",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    # init
    p_init = subparsers.add_parser("init", help="Initialize .flow/ directory")
    p_init.add_argument("--json", action="store_true", help="JSON output")
    p_init.set_defaults(func=cmd_init)

    # detect
    p_detect = subparsers.add_parser("detect", help="Check if .flow/ exists")
    p_detect.add_argument("--json", action="store_true", help="JSON output")
    p_detect.set_defaults(func=cmd_detect)

    # status
    p_status = subparsers.add_parser("status", help="Show .flow state and active runs")
    p_status.add_argument("--json", action="store_true", help="JSON output")
    p_status.set_defaults(func=cmd_status)

    # config
    p_config = subparsers.add_parser("config", help="Config commands")
    config_sub = p_config.add_subparsers(dest="config_cmd", required=True)

    p_config_get = config_sub.add_parser("get", help="Get config value")
    p_config_get.add_argument("key", help="Config key (e.g., memory.enabled)")
    p_config_get.add_argument("--json", action="store_true", help="JSON output")
    p_config_get.set_defaults(func=cmd_config_get)

    p_config_set = config_sub.add_parser("set", help="Set config value")
    p_config_set.add_argument("key", help="Config key (e.g., memory.enabled)")
    p_config_set.add_argument("value", help="Config value")
    p_config_set.add_argument("--json", action="store_true", help="JSON output")
    p_config_set.set_defaults(func=cmd_config_set)

    # invariants
    p_inv = subparsers.add_parser("invariants", help="Architecture invariant registry")
    inv_sub = p_inv.add_subparsers(dest="inv_cmd", required=True)

    p_inv_init = inv_sub.add_parser("init", help="Create invariants.md template")
    p_inv_init.add_argument("--force", action="store_true", help="Overwrite existing")
    p_inv_init.add_argument("--json", action="store_true", help="JSON output")
    p_inv_init.set_defaults(func=cmd_invariants_init)

    p_inv_show = inv_sub.add_parser("show", help="Show invariants")
    p_inv_show.add_argument("--json", action="store_true", help="JSON output")
    p_inv_show.set_defaults(func=cmd_invariants_show)

    p_inv_check = inv_sub.add_parser("check", help="Run all verify commands")
    p_inv_check.add_argument("--json", action="store_true", help="JSON output")
    p_inv_check.set_defaults(func=cmd_invariants_check)

    # guard
    p_guard = subparsers.add_parser("guard", help="Run test/lint/typecheck guards from stack config")
    p_guard.add_argument("--layer", default="all", help="Run guards for specific layer (backend, frontend, or all)")
    p_guard.add_argument("--json", action="store_true", help="JSON output")
    p_guard.set_defaults(func=cmd_guard)

    # stack
    p_stack = subparsers.add_parser("stack", help="Stack profile commands")
    stack_sub = p_stack.add_subparsers(dest="stack_cmd", required=True)

    p_stack_detect = stack_sub.add_parser("detect", help="Auto-detect project stack")
    p_stack_detect.add_argument("--dry-run", action="store_true", help="Show detection without saving")
    p_stack_detect.add_argument("--json", action="store_true", help="JSON output")
    p_stack_detect.set_defaults(func=cmd_stack_detect)

    p_stack_set = stack_sub.add_parser("set", help="Set stack config from JSON file")
    p_stack_set.add_argument("--file", required=True, help="JSON file path (or - for stdin)")
    p_stack_set.add_argument("--json", action="store_true", help="JSON output")
    p_stack_set.set_defaults(func=cmd_stack_set)

    p_stack_show = stack_sub.add_parser("show", help="Show current stack config")
    p_stack_show.add_argument("--json", action="store_true", help="JSON output")
    p_stack_show.set_defaults(func=cmd_stack_show)

    # review-backend (helper for skills)
    p_review_backend = subparsers.add_parser(
        "review-backend", help="Get review backend (ASK if not configured)"
    )
    p_review_backend.add_argument(
        "--compare",
        help="Compare review receipts (comma-separated file paths)",
    )
    p_review_backend.add_argument(
        "--epic",
        help="Auto-discover review receipts for epic (e.g., fn-1-api)",
    )
    p_review_backend.add_argument("--json", action="store_true", help="JSON output")
    p_review_backend.set_defaults(func=cmd_review_backend)

    # memory
    p_memory = subparsers.add_parser("memory", help="Memory commands (v2: atomic entries)")
    memory_sub = p_memory.add_subparsers(dest="memory_cmd", required=True)

    p_memory_init = memory_sub.add_parser("init", help="Initialize memory (auto-migrates legacy)")
    p_memory_init.add_argument("--json", action="store_true", help="JSON output")
    p_memory_init.set_defaults(func=cmd_memory_init)

    p_memory_add = memory_sub.add_parser("add", help="Add atomic memory entry")
    p_memory_add.add_argument("type", help="Type: pitfall, convention, or decision")
    p_memory_add.add_argument("content", help="Entry content")
    p_memory_add.add_argument("--json", action="store_true", help="JSON output")
    p_memory_add.set_defaults(func=cmd_memory_add)

    p_memory_read = memory_sub.add_parser("read", help="Read entries (L3: full content)")
    p_memory_read.add_argument(
        "--type", help="Filter by type: pitfall, convention, or decision"
    )
    p_memory_read.add_argument("--json", action="store_true", help="JSON output")
    p_memory_read.set_defaults(func=cmd_memory_read)

    p_memory_list = memory_sub.add_parser("list", help="List entries with ref counts")
    p_memory_list.add_argument("--json", action="store_true", help="JSON output")
    p_memory_list.set_defaults(func=cmd_memory_list)

    p_memory_search = memory_sub.add_parser("search", help="Search entries by pattern")
    p_memory_search.add_argument("pattern", help="Search pattern (regex)")
    p_memory_search.add_argument("--json", action="store_true", help="JSON output")
    p_memory_search.set_defaults(func=cmd_memory_search)

    p_memory_inject = memory_sub.add_parser(
        "inject", help="Inject relevant entries (progressive disclosure)"
    )
    p_memory_inject.add_argument("--type", help="Filter by type")
    p_memory_inject.add_argument("--tags", help="Filter by tags (comma-separated)")
    p_memory_inject.add_argument(
        "--full", action="store_true", help="L3: inject full content of all entries"
    )
    p_memory_inject.add_argument("--json", action="store_true", help="JSON output")
    p_memory_inject.set_defaults(func=cmd_memory_inject)

    p_memory_verify = memory_sub.add_parser(
        "verify", help="Mark entry as verified (still valid)"
    )
    p_memory_verify.add_argument("id", type=int, help="Entry ID to verify")
    p_memory_verify.add_argument("--json", action="store_true", help="JSON output")
    p_memory_verify.set_defaults(func=cmd_memory_verify)

    p_memory_gc = memory_sub.add_parser(
        "gc", help="Garbage collect stale entries"
    )
    p_memory_gc.add_argument(
        "--days", type=int, default=90, help="Remove entries older than N days with 0 refs (default: 90)"
    )
    p_memory_gc.add_argument(
        "--dry-run", action="store_true", help="Show what would be removed"
    )
    p_memory_gc.add_argument("--json", action="store_true", help="JSON output")
    p_memory_gc.set_defaults(func=cmd_memory_gc)

    # epic create
    p_epic = subparsers.add_parser("epic", help="Epic commands")
    epic_sub = p_epic.add_subparsers(dest="epic_cmd", required=True)

    p_epic_create = epic_sub.add_parser("create", help="Create new epic")
    p_epic_create.add_argument("--title", required=True, help="Epic title")
    p_epic_create.add_argument("--branch", help="Branch name to store on epic")
    p_epic_create.add_argument("--json", action="store_true", help="JSON output")
    p_epic_create.set_defaults(func=cmd_epic_create)

    p_epic_set_plan = epic_sub.add_parser("set-plan", help="Set epic spec from file")
    p_epic_set_plan.add_argument("id", help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_epic_set_plan.add_argument("--file", required=True, help="Markdown file (use '-' for stdin)")
    p_epic_set_plan.add_argument("--json", action="store_true", help="JSON output")
    p_epic_set_plan.set_defaults(func=cmd_epic_set_plan)

    p_epic_set_review = epic_sub.add_parser(
        "set-plan-review-status", help="Set plan review status"
    )
    p_epic_set_review.add_argument("id", help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_epic_set_review.add_argument(
        "--status",
        required=True,
        choices=["ship", "needs_work", "unknown"],
        help="Plan review status",
    )
    p_epic_set_review.add_argument("--json", action="store_true", help="JSON output")
    p_epic_set_review.set_defaults(func=cmd_epic_set_plan_review_status)

    p_epic_set_completion_review = epic_sub.add_parser(
        "set-completion-review-status", help="Set completion review status"
    )
    p_epic_set_completion_review.add_argument("id", help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_epic_set_completion_review.add_argument(
        "--status",
        required=True,
        choices=["ship", "needs_work", "unknown"],
        help="Completion review status",
    )
    p_epic_set_completion_review.add_argument("--json", action="store_true", help="JSON output")
    p_epic_set_completion_review.set_defaults(func=cmd_epic_set_completion_review_status)

    p_epic_set_branch = epic_sub.add_parser("set-branch", help="Set epic branch name")
    p_epic_set_branch.add_argument("id", help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_epic_set_branch.add_argument("--branch", required=True, help="Branch name")
    p_epic_set_branch.add_argument("--json", action="store_true", help="JSON output")
    p_epic_set_branch.set_defaults(func=cmd_epic_set_branch)

    p_epic_set_title = epic_sub.add_parser(
        "set-title", help="Rename epic by setting a new title (updates slug)"
    )
    p_epic_set_title.add_argument("id", help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_epic_set_title.add_argument("--title", required=True, help="New title for the epic")
    p_epic_set_title.add_argument("--json", action="store_true", help="JSON output")
    p_epic_set_title.set_defaults(func=cmd_epic_set_title)

    p_epic_close = epic_sub.add_parser("close", help="Close epic")
    p_epic_close.add_argument("id", help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_epic_close.add_argument("--skip-gap-check", action="store_true", help="Bypass gap registry gate (use with caution)")
    p_epic_close.add_argument("--json", action="store_true", help="JSON output")
    p_epic_close.set_defaults(func=cmd_epic_close)

    p_epic_reopen = epic_sub.add_parser("reopen", help="Reopen a closed epic")
    p_epic_reopen.add_argument("id", help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_epic_reopen.add_argument("--json", action="store_true", help="JSON output")
    p_epic_reopen.set_defaults(func=cmd_epic_reopen)

    p_epic_archive = epic_sub.add_parser(
        "archive", help="Archive closed epic to .flow/.archive/"
    )
    p_epic_archive.add_argument("id", help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_epic_archive.add_argument(
        "--force", action="store_true", help="Archive even if not closed"
    )
    p_epic_archive.add_argument("--json", action="store_true", help="JSON output")
    p_epic_archive.set_defaults(func=cmd_epic_archive)

    p_epic_clean = epic_sub.add_parser(
        "clean", help="Archive all closed epics at once"
    )
    p_epic_clean.add_argument("--json", action="store_true", help="JSON output")
    p_epic_clean.set_defaults(func=cmd_epic_clean)

    p_epic_add_dep = epic_sub.add_parser("add-dep", help="Add epic-level dependency")
    p_epic_add_dep.add_argument("epic", help="Epic ID")
    p_epic_add_dep.add_argument("depends_on", help="Epic ID to depend on")
    p_epic_add_dep.add_argument("--json", action="store_true", help="JSON output")
    p_epic_add_dep.set_defaults(func=cmd_epic_add_dep)

    p_epic_rm_dep = epic_sub.add_parser("rm-dep", help="Remove epic-level dependency")
    p_epic_rm_dep.add_argument("epic", help="Epic ID")
    p_epic_rm_dep.add_argument("depends_on", help="Epic ID to remove from deps")
    p_epic_rm_dep.add_argument("--json", action="store_true", help="JSON output")
    p_epic_rm_dep.set_defaults(func=cmd_epic_rm_dep)

    p_epic_set_backend = epic_sub.add_parser(
        "set-backend", help="Set default backend specs for impl/review/sync"
    )
    p_epic_set_backend.add_argument("id", help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_epic_set_backend.add_argument(
        "--impl", help="Default impl backend spec (e.g., 'codex:gpt-5.4-high')"
    )
    p_epic_set_backend.add_argument(
        "--review", help="Default review backend spec (e.g., 'claude:opus')"
    )
    p_epic_set_backend.add_argument(
        "--sync", help="Default sync backend spec (e.g., 'claude:haiku')"
    )
    p_epic_set_backend.add_argument("--json", action="store_true", help="JSON output")
    p_epic_set_backend.set_defaults(func=cmd_epic_set_backend)

    # task create
    p_task = subparsers.add_parser("task", help="Task commands")
    task_sub = p_task.add_subparsers(dest="task_cmd", required=True)

    p_task_create = task_sub.add_parser("create", help="Create new task")
    p_task_create.add_argument("--epic", required=True, help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_task_create.add_argument("--title", required=True, help="Task title")
    p_task_create.add_argument("--deps", help="Comma-separated dependency IDs")
    p_task_create.add_argument(
        "--acceptance-file", help="Markdown file with acceptance criteria"
    )
    p_task_create.add_argument(
        "--priority", type=int, help="Priority (lower = earlier)"
    )
    p_task_create.add_argument(
        "--domain",
        choices=["frontend", "backend", "architecture", "testing", "docs", "ops", "general"],
        help="Task domain (e.g., frontend, backend)",
    )
    p_task_create.add_argument(
        "--files",
        help="Comma-separated owned file paths (e.g., src/auth.ts,src/routes.ts)",
    )
    p_task_create.add_argument("--json", action="store_true", help="JSON output")
    p_task_create.set_defaults(func=cmd_task_create)

    p_task_desc = task_sub.add_parser("set-description", help="Set task description")
    p_task_desc.add_argument("id", help="Task ID (e.g., fn-1.2, fn-1-add-auth.2)")
    p_task_desc.add_argument("--file", required=True, help="Markdown file (use '-' for stdin)")
    p_task_desc.add_argument("--json", action="store_true", help="JSON output")
    p_task_desc.set_defaults(func=cmd_task_set_description)

    p_task_acc = task_sub.add_parser("set-acceptance", help="Set task acceptance")
    p_task_acc.add_argument("id", help="Task ID (e.g., fn-1.2, fn-1-add-auth.2)")
    p_task_acc.add_argument("--file", required=True, help="Markdown file (use '-' for stdin)")
    p_task_acc.add_argument("--json", action="store_true", help="JSON output")
    p_task_acc.set_defaults(func=cmd_task_set_acceptance)

    p_task_set_spec = task_sub.add_parser(
        "set-spec", help="Set task spec (full file or sections)"
    )
    p_task_set_spec.add_argument("id", help="Task ID (e.g., fn-1.2, fn-1-add-auth.2)")
    p_task_set_spec.add_argument(
        "--file", help="Full spec file (use '-' for stdin) - replaces entire spec"
    )
    p_task_set_spec.add_argument(
        "--description", help="Description section file (use '-' for stdin)"
    )
    p_task_set_spec.add_argument(
        "--acceptance", help="Acceptance section file (use '-' for stdin)"
    )
    p_task_set_spec.add_argument("--json", action="store_true", help="JSON output")
    p_task_set_spec.set_defaults(func=cmd_task_set_spec)

    p_task_reset = task_sub.add_parser("reset", help="Reset task to todo")
    p_task_reset.add_argument("task_id", help="Task ID (e.g., fn-1.2, fn-1-add-auth.2)")
    p_task_reset.add_argument(
        "--cascade", action="store_true", help="Also reset dependent tasks (same epic)"
    )
    p_task_reset.add_argument("--json", action="store_true", help="JSON output")
    p_task_reset.set_defaults(func=cmd_task_reset)

    p_task_skip = task_sub.add_parser("skip", help="Skip task (mark as permanently skipped)")
    p_task_skip.add_argument("task_id", help="Task ID")
    p_task_skip.add_argument("--reason", help="Why the task is being skipped")
    p_task_skip.add_argument("--json", action="store_true", help="JSON output")
    p_task_skip.set_defaults(func=cmd_task_skip)

    p_task_split = task_sub.add_parser("split", help="Split task into sub-tasks (runtime DAG mutation)")
    p_task_split.add_argument("task_id", help="Task ID to split")
    p_task_split.add_argument("--titles", required=True, help="Sub-task titles separated by '|' (e.g., 'Backend|Frontend|Tests')")
    p_task_split.add_argument("--chain", action="store_true", help="Chain sub-tasks sequentially (each depends on previous)")
    p_task_split.add_argument("--json", action="store_true", help="JSON output")
    p_task_split.set_defaults(func=cmd_task_split)

    p_task_set_backend = task_sub.add_parser(
        "set-backend", help="Set backend specs for impl/review/sync"
    )
    p_task_set_backend.add_argument("id", help="Task ID (e.g., fn-1.2, fn-1-add-auth.2)")
    p_task_set_backend.add_argument(
        "--impl", help="Impl backend spec (e.g., 'codex:gpt-5.4-high')"
    )
    p_task_set_backend.add_argument(
        "--review", help="Review backend spec (e.g., 'claude:opus')"
    )
    p_task_set_backend.add_argument(
        "--sync", help="Sync backend spec (e.g., 'claude:haiku')"
    )
    p_task_set_backend.add_argument("--json", action="store_true", help="JSON output")
    p_task_set_backend.set_defaults(func=cmd_task_set_backend)

    p_task_show_backend = task_sub.add_parser(
        "show-backend", help="Show effective backend specs (task + epic levels)"
    )
    p_task_show_backend.add_argument("id", help="Task ID (e.g., fn-1.2, fn-1-add-auth.2)")
    p_task_show_backend.add_argument("--json", action="store_true", help="JSON output")
    p_task_show_backend.set_defaults(func=cmd_task_show_backend)

    p_task_set_deps = task_sub.add_parser(
        "set-deps", help="Set task dependencies (comma-separated)"
    )
    p_task_set_deps.add_argument("task_id", help="Task ID (e.g., fn-1.2, fn-1-add-auth.2)")
    p_task_set_deps.add_argument(
        "--deps", required=True, help="Comma-separated dependency IDs (e.g., fn-1-add-auth.1,fn-1-add-auth.2)"
    )
    p_task_set_deps.add_argument("--json", action="store_true", help="JSON output")
    p_task_set_deps.set_defaults(func=cmd_task_set_deps)

    # dep add
    p_dep = subparsers.add_parser("dep", help="Dependency commands")
    dep_sub = p_dep.add_subparsers(dest="dep_cmd", required=True)

    p_dep_add = dep_sub.add_parser("add", help="Add dependency")
    p_dep_add.add_argument("task", help="Task ID (e.g., fn-1.2, fn-1-add-auth.2)")
    p_dep_add.add_argument("depends_on", help="Dependency task ID (e.g., fn-1.1, fn-1-add-auth.1)")
    p_dep_add.add_argument("--json", action="store_true", help="JSON output")
    p_dep_add.set_defaults(func=cmd_dep_add)

    p_dep_rm = dep_sub.add_parser("rm", help="Remove dependency")
    p_dep_rm.add_argument("task", help="Task ID")
    p_dep_rm.add_argument("depends_on", help="Dependency to remove")
    p_dep_rm.add_argument("--json", action="store_true", help="JSON output")
    p_dep_rm.set_defaults(func=cmd_dep_rm)

    # gap
    p_gap = subparsers.add_parser("gap", help="Requirement gap registry")
    gap_sub = p_gap.add_subparsers(dest="gap_cmd", required=True)

    p_gap_add = gap_sub.add_parser("add", help="Register a requirement gap")
    p_gap_add.add_argument("--epic", required=True, help="Epic ID (e.g., fn-1-add-auth)")
    p_gap_add.add_argument("--capability", "--title", required=True, help="What is missing")
    p_gap_add.add_argument("--priority", default="required", choices=["required", "important", "nice-to-have"], help="Gap priority (default: required)")
    p_gap_add.add_argument("--source", default="manual", help="Where gap was found (default: manual)")
    p_gap_add.add_argument("--task", default=None, help="Task ID that addresses this gap")
    p_gap_add.add_argument("--json", action="store_true", help="JSON output")
    p_gap_add.set_defaults(func=cmd_gap_add)

    p_gap_list = gap_sub.add_parser("list", help="List gaps for an epic")
    p_gap_list.add_argument("--epic", required=True, help="Epic ID")
    p_gap_list.add_argument("--status", choices=["open", "resolved"], help="Filter by status")
    p_gap_list.add_argument("--json", action="store_true", help="JSON output")
    p_gap_list.set_defaults(func=cmd_gap_list)

    p_gap_resolve = gap_sub.add_parser("resolve", help="Mark a gap as resolved")
    p_gap_resolve.add_argument("--epic", required=True, help="Epic ID")
    gap_resolve_target = p_gap_resolve.add_mutually_exclusive_group(required=True)
    gap_resolve_target.add_argument("--capability", "--title", help="Capability to resolve (used to find the gap)")
    gap_resolve_target.add_argument("--id", dest="gap_id", help="Gap ID to resolve directly (e.g., gap-b07f8fd3)")
    p_gap_resolve.add_argument("--evidence", required=True, help="How the gap was resolved")
    p_gap_resolve.add_argument("--json", action="store_true", help="JSON output")
    p_gap_resolve.set_defaults(func=cmd_gap_resolve)

    p_gap_check = gap_sub.add_parser("check", help="Gate check: pass/fail based on unresolved gaps")
    p_gap_check.add_argument("--epic", required=True, help="Epic ID")
    p_gap_check.add_argument("--json", action="store_true", help="JSON output")
    p_gap_check.set_defaults(func=cmd_gap_check)

    # parse-findings
    p_pf = subparsers.add_parser(
        "parse-findings",
        help="Extract structured findings from review output",
    )
    p_pf.add_argument(
        "--file", required=True,
        help="Review output file (or '-' for stdin)",
    )
    p_pf.add_argument(
        "--epic", default=None,
        help="Epic ID (required with --register)",
    )
    p_pf.add_argument(
        "--register", action="store_true",
        help="Auto-register critical/major findings as gaps",
    )
    p_pf.add_argument(
        "--source", default="manual",
        help="Gap source label (default: manual)",
    )
    p_pf.add_argument("--json", action="store_true", help="JSON output")
    p_pf.set_defaults(func=cmd_parse_findings)

    # show
    p_show = subparsers.add_parser("show", help="Show epic or task")
    p_show.add_argument("id", help="Epic or task ID (e.g., fn-1-add-auth, fn-1-add-auth.2)")
    p_show.add_argument("--json", action="store_true", help="JSON output")
    p_show.set_defaults(func=cmd_show)

    # epics
    p_epics = subparsers.add_parser("epics", help="List all epics")
    p_epics.add_argument("--json", action="store_true", help="JSON output")
    p_epics.set_defaults(func=cmd_epics)

    # files (ownership map)
    p_files = subparsers.add_parser("files", help="Show file ownership map for epic")
    p_files.add_argument("--epic", required=True, help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_files.add_argument("--json", action="store_true", help="JSON output")
    p_files.set_defaults(func=cmd_files)

    # lock (Teams file locking)
    p_lock = subparsers.add_parser("lock", help="Lock files for a task (Teams mode)")
    p_lock.add_argument("--task", required=True, help="Task ID that owns the files")
    p_lock.add_argument("--files", required=True, help="Comma-separated file paths to lock")
    p_lock.add_argument("--json", action="store_true", help="JSON output")
    p_lock.set_defaults(func=cmd_lock)

    # unlock (Teams file unlocking)
    p_unlock = subparsers.add_parser("unlock", help="Unlock files for a task (Teams mode)")
    p_unlock.add_argument("--task", default="", help="Task ID to unlock files for")
    p_unlock.add_argument("--files", help="Comma-separated file paths (omit for all task files)")
    p_unlock.add_argument("--all", action="store_true", help="Clear ALL file locks")
    p_unlock.add_argument("--json", action="store_true", help="JSON output")
    p_unlock.set_defaults(func=cmd_unlock)

    # lock-check (Teams file lock inspection)
    p_lock_check = subparsers.add_parser("lock-check", help="Check file lock status (Teams mode)")
    p_lock_check.add_argument("--file", help="Specific file to check (omit to list all)")
    p_lock_check.add_argument("--json", action="store_true", help="JSON output")
    p_lock_check.set_defaults(func=cmd_lock_check)

    p_tasks = subparsers.add_parser("tasks", help="List tasks")
    p_tasks.add_argument("--epic", help="Filter by epic ID (e.g., fn-1, fn-1-add-auth)")
    p_tasks.add_argument(
        "--status",
        choices=["todo", "in_progress", "blocked", "done"],
        help="Filter by status",
    )
    p_tasks.add_argument(
        "--domain",
        choices=["frontend", "backend", "architecture", "testing", "docs", "ops", "general"],
        help="Filter by domain",
    )
    p_tasks.add_argument("--json", action="store_true", help="JSON output")
    p_tasks.set_defaults(func=cmd_tasks)

    # list
    p_list = subparsers.add_parser("list", help="List all epics and tasks")
    p_list.add_argument("--json", action="store_true", help="JSON output")
    p_list.set_defaults(func=cmd_list)

    # cat
    p_cat = subparsers.add_parser("cat", help="Print spec markdown")
    p_cat.add_argument("id", help="Epic or task ID (e.g., fn-1-add-auth, fn-1-add-auth.2)")
    p_cat.set_defaults(func=cmd_cat)

    # ready
    p_ready = subparsers.add_parser("ready", help="List ready tasks")
    p_ready.add_argument("--epic", required=True, help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_ready.add_argument("--json", action="store_true", help="JSON output")
    p_ready.set_defaults(func=cmd_ready)

    # queue
    p_queue = subparsers.add_parser("queue", help="Show multi-epic queue status")
    p_queue.add_argument("--json", action="store_true", help="JSON output")
    p_queue.set_defaults(func=cmd_queue)

    # next
    p_next = subparsers.add_parser("next", help="Select next plan/work unit")
    p_next.add_argument("--epics-file", help="JSON file with ordered epic list")
    p_next.add_argument(
        "--require-plan-review",
        action="store_true",
        help="Require plan review before work",
    )
    p_next.add_argument(
        "--require-completion-review",
        action="store_true",
        help="Require completion review when all tasks done",
    )
    p_next.add_argument("--json", action="store_true", help="JSON output")
    p_next.set_defaults(func=cmd_next)

    # start
    p_start = subparsers.add_parser("start", help="Start task")
    p_start.add_argument("id", help="Task ID (e.g., fn-1.2, fn-1-add-auth.2)")
    p_start.add_argument(
        "--force", action="store_true", help="Skip status/dependency/claim checks"
    )
    p_start.add_argument("--note", help="Claim note (e.g., reason for taking over)")
    p_start.add_argument("--json", action="store_true", help="JSON output")
    p_start.set_defaults(func=cmd_start)

    # done
    p_done = subparsers.add_parser("done", help="Complete task")
    p_done.add_argument("id", help="Task ID (e.g., fn-1.2, fn-1-add-auth.2)")
    p_done.add_argument("--summary-file", help="Done summary markdown file")
    p_done.add_argument("--summary", help="Done summary (inline text)")
    p_done.add_argument("--evidence-json", help="Evidence JSON file path or inline JSON string (auto-detected)")
    p_done.add_argument("--evidence", help="Evidence JSON (inline string, legacy — prefer --evidence-json)")
    p_done.add_argument("--force", action="store_true", help="Skip status checks")
    p_done.add_argument("--json", action="store_true", help="JSON output")
    p_done.set_defaults(func=cmd_done)

    # restart
    p_restart = subparsers.add_parser(
        "restart", help="Restart task and cascade-reset downstream dependents"
    )
    p_restart.add_argument("id", help="Task ID (e.g., fn-1.2, fn-1-add-auth.2)")
    p_restart.add_argument(
        "--dry-run", action="store_true", help="Show what would be reset without doing it"
    )
    p_restart.add_argument(
        "--force", action="store_true", help="Allow restart even if tasks are in_progress"
    )
    p_restart.add_argument("--json", action="store_true", help="JSON output")
    p_restart.set_defaults(func=cmd_restart)

    # block
    p_block = subparsers.add_parser("block", help="Block task with reason")
    p_block.add_argument("id", help="Task ID (e.g., fn-1.2, fn-1-add-auth.2)")
    p_block.add_argument(
        "--reason-file", required=True, help="Markdown file with block reason"
    )
    p_block.add_argument("--json", action="store_true", help="JSON output")
    p_block.set_defaults(func=cmd_block)

    # state-path
    p_state_path = subparsers.add_parser(
        "state-path", help="Show resolved state directory path"
    )
    p_state_path.add_argument("--task", help="Task ID to show state file path for")
    p_state_path.add_argument("--json", action="store_true", help="JSON output")
    p_state_path.set_defaults(func=cmd_state_path)

    # migrate-state
    p_migrate = subparsers.add_parser(
        "migrate-state", help="Migrate runtime state from definition files to state-dir"
    )
    p_migrate.add_argument(
        "--clean",
        action="store_true",
        help="Remove runtime fields from definition files after migration",
    )
    p_migrate.add_argument("--json", action="store_true", help="JSON output")
    p_migrate.set_defaults(func=cmd_migrate_state)

    # validate
    p_validate = subparsers.add_parser("validate", help="Validate epic or all")
    p_validate.add_argument("--epic", help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_validate.add_argument(
        "--all", action="store_true", help="Validate all epics and tasks"
    )
    p_validate.add_argument("--json", action="store_true", help="JSON output")
    p_validate.set_defaults(func=cmd_validate)

    # doctor
    p_doctor = subparsers.add_parser(
        "doctor", help="Run comprehensive state health diagnostics"
    )
    p_doctor.add_argument("--json", action="store_true", help="JSON output")
    p_doctor.set_defaults(func=cmd_doctor)

    # checkpoint
    p_checkpoint = subparsers.add_parser("checkpoint", help="Checkpoint commands")
    checkpoint_sub = p_checkpoint.add_subparsers(dest="checkpoint_cmd", required=True)

    p_checkpoint_save = checkpoint_sub.add_parser(
        "save", help="Save epic state to checkpoint"
    )
    p_checkpoint_save.add_argument("--epic", required=True, help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_checkpoint_save.add_argument("--json", action="store_true", help="JSON output")
    p_checkpoint_save.set_defaults(func=cmd_checkpoint_save)

    p_checkpoint_restore = checkpoint_sub.add_parser(
        "restore", help="Restore epic state from checkpoint"
    )
    p_checkpoint_restore.add_argument("--epic", required=True, help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_checkpoint_restore.add_argument("--json", action="store_true", help="JSON output")
    p_checkpoint_restore.set_defaults(func=cmd_checkpoint_restore)

    p_checkpoint_delete = checkpoint_sub.add_parser(
        "delete", help="Delete checkpoint for epic"
    )
    p_checkpoint_delete.add_argument("--epic", required=True, help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_checkpoint_delete.add_argument("--json", action="store_true", help="JSON output")
    p_checkpoint_delete.set_defaults(func=cmd_checkpoint_delete)

    # prep-chat (for rp-cli chat_send JSON escaping)
    p_prep = subparsers.add_parser(
        "prep-chat", help="Prepare JSON for rp-cli chat_send"
    )
    p_prep.add_argument(
        "id", nargs="?", help="(ignored) Epic/task ID for compatibility"
    )
    p_prep.add_argument(
        "--message-file", required=True, help="File containing message text"
    )
    p_prep.add_argument(
        "--mode", default="chat", choices=["chat", "ask"], help="Chat mode"
    )
    p_prep.add_argument("--new-chat", action="store_true", help="Start new chat")
    p_prep.add_argument("--chat-name", help="Name for new chat")
    p_prep.add_argument(
        "--selected-paths", nargs="*", help="Files to include in context"
    )
    p_prep.add_argument("--output", "-o", help="Output file (default: stdout)")
    p_prep.set_defaults(func=cmd_prep_chat)

    # ralph (Ralph run control)
    p_ralph = subparsers.add_parser("ralph", help="Ralph run control commands")
    ralph_sub = p_ralph.add_subparsers(dest="ralph_cmd", required=True)

    p_ralph_pause = ralph_sub.add_parser("pause", help="Pause a Ralph run")
    p_ralph_pause.add_argument("--run", help="Run ID (auto-detect if single)")
    p_ralph_pause.add_argument("--json", action="store_true", help="JSON output")
    p_ralph_pause.set_defaults(func=cmd_ralph_pause)

    p_ralph_resume = ralph_sub.add_parser("resume", help="Resume a paused Ralph run")
    p_ralph_resume.add_argument("--run", help="Run ID (auto-detect if single)")
    p_ralph_resume.add_argument("--json", action="store_true", help="JSON output")
    p_ralph_resume.set_defaults(func=cmd_ralph_resume)

    p_ralph_stop = ralph_sub.add_parser("stop", help="Request a Ralph run to stop")
    p_ralph_stop.add_argument("--run", help="Run ID (auto-detect if single)")
    p_ralph_stop.add_argument("--json", action="store_true", help="JSON output")
    p_ralph_stop.set_defaults(func=cmd_ralph_stop)

    p_ralph_status = ralph_sub.add_parser("status", help="Show Ralph run status")
    p_ralph_status.add_argument("--run", help="Run ID (auto-detect if single)")
    p_ralph_status.add_argument("--json", action="store_true", help="JSON output")
    p_ralph_status.set_defaults(func=cmd_ralph_status)

    # rp (RepoPrompt wrappers)
    p_rp = subparsers.add_parser("rp", help="RepoPrompt helpers")
    rp_sub = p_rp.add_subparsers(dest="rp_cmd", required=True)

    p_rp_windows = rp_sub.add_parser(
        "windows", help="List RepoPrompt windows (raw JSON)"
    )
    p_rp_windows.add_argument("--json", action="store_true", help="JSON output (raw)")
    p_rp_windows.set_defaults(func=cmd_rp_windows)

    p_rp_pick = rp_sub.add_parser("pick-window", help="Pick window by repo root")
    p_rp_pick.add_argument("--repo-root", required=True, help="Repo root path")
    p_rp_pick.add_argument("--json", action="store_true", help="JSON output")
    p_rp_pick.set_defaults(func=cmd_rp_pick_window)

    p_rp_ws = rp_sub.add_parser(
        "ensure-workspace", help="Ensure workspace and switch window"
    )
    p_rp_ws.add_argument("--window", type=int, required=True, help="Window id")
    p_rp_ws.add_argument("--repo-root", required=True, help="Repo root path")
    p_rp_ws.set_defaults(func=cmd_rp_ensure_workspace)

    p_rp_builder = rp_sub.add_parser("builder", help="Run builder and return tab")
    p_rp_builder.add_argument("--window", type=int, required=True, help="Window id")
    p_rp_builder.add_argument("--summary", required=True, help="Builder summary")
    p_rp_builder.add_argument(
        "--response-type",
        dest="response_type",
        choices=["review", "plan", "question", "clarify"],
        help="Builder response type (requires RP 1.6.0+)",
    )
    p_rp_builder.add_argument("--json", action="store_true", help="JSON output")
    p_rp_builder.set_defaults(func=cmd_rp_builder)

    p_rp_prompt_get = rp_sub.add_parser("prompt-get", help="Get current prompt")
    p_rp_prompt_get.add_argument("--window", type=int, required=True, help="Window id")
    p_rp_prompt_get.add_argument("--tab", required=True, help="Tab id or name")
    p_rp_prompt_get.set_defaults(func=cmd_rp_prompt_get)

    p_rp_prompt_set = rp_sub.add_parser("prompt-set", help="Set current prompt")
    p_rp_prompt_set.add_argument("--window", type=int, required=True, help="Window id")
    p_rp_prompt_set.add_argument("--tab", required=True, help="Tab id or name")
    p_rp_prompt_set.add_argument("--message-file", required=True, help="Message file")
    p_rp_prompt_set.set_defaults(func=cmd_rp_prompt_set)

    p_rp_select_get = rp_sub.add_parser("select-get", help="Get selection")
    p_rp_select_get.add_argument("--window", type=int, required=True, help="Window id")
    p_rp_select_get.add_argument("--tab", required=True, help="Tab id or name")
    p_rp_select_get.set_defaults(func=cmd_rp_select_get)

    p_rp_select_add = rp_sub.add_parser("select-add", help="Add files to selection")
    p_rp_select_add.add_argument("--window", type=int, required=True, help="Window id")
    p_rp_select_add.add_argument("--tab", required=True, help="Tab id or name")
    p_rp_select_add.add_argument("paths", nargs="+", help="Paths to add")
    p_rp_select_add.set_defaults(func=cmd_rp_select_add)

    p_rp_chat = rp_sub.add_parser("chat-send", help="Send chat via rp-cli")
    p_rp_chat.add_argument("--window", type=int, required=True, help="Window id")
    p_rp_chat.add_argument("--tab", required=True, help="Tab id or name")
    p_rp_chat.add_argument("--message-file", required=True, help="Message file")
    p_rp_chat.add_argument("--new-chat", action="store_true", help="Start new chat")
    p_rp_chat.add_argument("--chat-name", help="Chat name (with --new-chat)")
    p_rp_chat.add_argument(
        "--chat-id",
        dest="chat_id",
        help="Continue specific chat by ID (RP 1.6.0+)",
    )
    p_rp_chat.add_argument(
        "--mode",
        choices=["chat", "review", "plan", "edit"],
        default="chat",
        help="Chat mode (default: chat)",
    )
    p_rp_chat.add_argument(
        "--selected-paths", nargs="*", help="Override selected paths"
    )
    p_rp_chat.add_argument(
        "--json", action="store_true", help="JSON output (no review text)"
    )
    p_rp_chat.set_defaults(func=cmd_rp_chat_send)

    p_rp_export = rp_sub.add_parser("prompt-export", help="Export prompt to file")
    p_rp_export.add_argument("--window", type=int, required=True, help="Window id")
    p_rp_export.add_argument("--tab", required=True, help="Tab id or name")
    p_rp_export.add_argument("--out", required=True, help="Output file")
    p_rp_export.set_defaults(func=cmd_rp_prompt_export)

    p_rp_setup = rp_sub.add_parser(
        "setup-review", help="Atomic: pick-window + workspace + builder"
    )
    p_rp_setup.add_argument("--repo-root", required=True, help="Repo root path")
    p_rp_setup.add_argument("--summary", required=True, help="Builder summary/instructions")
    p_rp_setup.add_argument(
        "--response-type",
        dest="response_type",
        choices=["review"],
        help="Use builder review mode (requires RP 1.6.0+)",
    )
    p_rp_setup.add_argument(
        "--create",
        action="store_true",
        help="Create new RP window if none matches (requires RP 1.5.68+)",
    )
    p_rp_setup.add_argument("--json", action="store_true", help="JSON output")
    p_rp_setup.set_defaults(func=cmd_rp_setup_review)

    # codex (Codex CLI wrappers)
    p_codex = subparsers.add_parser("codex", help="Codex CLI helpers")
    codex_sub = p_codex.add_subparsers(dest="codex_cmd", required=True)

    p_codex_check = codex_sub.add_parser("check", help="Check codex availability")
    p_codex_check.add_argument("--json", action="store_true", help="JSON output")
    p_codex_check.set_defaults(func=cmd_codex_check)

    p_codex_impl = codex_sub.add_parser("impl-review", help="Implementation review")
    p_codex_impl.add_argument(
        "task",
        nargs="?",
        default=None,
        help="Task ID (e.g., fn-1.2, fn-1-add-auth.2), optional for standalone",
    )
    p_codex_impl.add_argument("--base", required=True, help="Base branch for diff")
    p_codex_impl.add_argument(
        "--focus", help="Focus areas for standalone review (comma-separated)"
    )
    p_codex_impl.add_argument(
        "--receipt", help="Receipt file path for session continuity"
    )
    p_codex_impl.add_argument("--json", action="store_true", help="JSON output")
    p_codex_impl.add_argument(
        "--sandbox",
        choices=["read-only", "workspace-write", "danger-full-access", "auto"],
        default="auto",
        help="Sandbox mode (auto: danger-full-access on Windows, read-only on Unix)",
    )
    p_codex_impl.add_argument(
        "--effort", choices=CODEX_EFFORT_LEVELS, default="high",
        help="Model reasoning effort level (default: high)",
    )
    p_codex_impl.set_defaults(func=cmd_codex_impl_review)

    p_codex_plan = codex_sub.add_parser("plan-review", help="Plan review")
    p_codex_plan.add_argument("epic", help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_codex_plan.add_argument(
        "--files",
        required=True,
        help="Comma-separated file paths to embed for context (required)",
    )
    p_codex_plan.add_argument("--base", default="main", help="Base branch for context")
    p_codex_plan.add_argument(
        "--receipt", help="Receipt file path for session continuity"
    )
    p_codex_plan.add_argument("--json", action="store_true", help="JSON output")
    p_codex_plan.add_argument(
        "--sandbox",
        choices=["read-only", "workspace-write", "danger-full-access", "auto"],
        default="auto",
        help="Sandbox mode (auto: danger-full-access on Windows, read-only on Unix)",
    )
    p_codex_plan.add_argument(
        "--effort", choices=CODEX_EFFORT_LEVELS, default="high",
        help="Model reasoning effort level (default: high)",
    )
    p_codex_plan.set_defaults(func=cmd_codex_plan_review)

    p_codex_adversarial = codex_sub.add_parser(
        "adversarial", help="Adversarial review — tries to break the code, not validate it"
    )
    p_codex_adversarial.add_argument("--base", default="main", help="Base branch for diff")
    p_codex_adversarial.add_argument("--focus", help="Specific area to pressure-test (e.g., 'race conditions', 'auth bypass')")
    p_codex_adversarial.add_argument("--json", action="store_true", help="JSON output")
    p_codex_adversarial.add_argument(
        "--sandbox", default="auto",
        help="Sandbox mode: read-only (default), network-disabled, danger-full-access, or auto"
    )
    p_codex_adversarial.add_argument(
        "--effort", choices=CODEX_EFFORT_LEVELS, default="high",
        help="Model reasoning effort level (default: high)",
    )
    p_codex_adversarial.set_defaults(func=cmd_codex_adversarial)

    p_codex_completion = codex_sub.add_parser(
        "completion-review", help="Epic completion review"
    )
    p_codex_completion.add_argument("epic", help="Epic ID (e.g., fn-1, fn-1-add-auth)")
    p_codex_completion.add_argument(
        "--base", default="main", help="Base branch for diff"
    )
    p_codex_completion.add_argument(
        "--receipt", help="Receipt file path for session continuity"
    )
    p_codex_completion.add_argument("--json", action="store_true", help="JSON output")
    p_codex_completion.add_argument(
        "--sandbox",
        choices=["read-only", "workspace-write", "danger-full-access", "auto"],
        default="auto",
        help="Sandbox mode (auto: danger-full-access on Windows, read-only on Unix)",
    )
    p_codex_completion.add_argument(
        "--effort", choices=CODEX_EFFORT_LEVELS, default="high",
        help="Model reasoning effort level (default: high)",
    )
    p_codex_completion.set_defaults(func=cmd_codex_completion_review)

    args = parser.parse_args()
    args.func(args)
