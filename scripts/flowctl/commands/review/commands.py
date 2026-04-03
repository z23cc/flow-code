"""Review commands: check, impl-review, plan-review, completion-review."""

import argparse
import shutil
import sys
from pathlib import Path

from flowctl.core.constants import SPECS_DIR, TASKS_DIR
from flowctl.core.git import (
    gather_context_hints,
    get_changed_files,
    get_diff_context,
    get_embedded_file_contents,
)
from flowctl.core.ids import is_epic_id, is_task_id
from flowctl.core.io import (
    error_exit,
    json_output,
)
from flowctl.core.paths import ensure_flow_exists, get_flow_dir, get_repo_root

from flowctl.commands.review.codex_utils import (
    delete_stale_receipt,
    get_codex_version,
    is_sandbox_failure,
    load_receipt,
    parse_codex_verdict,
    resolve_codex_sandbox,
    run_codex_exec,
    save_receipt,
    CODEX_EFFORT_LEVELS,
)
from flowctl.commands.review.prompts import (
    build_review_prompt,
    build_standalone_review_prompt,
    build_rereview_preamble,
    build_completion_review_prompt,
)

def cmd_codex_check(args: argparse.Namespace) -> None:
    """Check if codex CLI is available and return version."""
    codex = shutil.which("codex")
    available = codex is not None
    version = get_codex_version() if available else None

    if args.json:
        json_output({"available": available, "version": version})
    else:
        if available:
            print(f"codex available: {version or 'unknown version'}")
        else:
            print("codex not available")



def cmd_codex_impl_review(args: argparse.Namespace) -> None:
    """Run implementation review via codex exec."""
    task_id = args.task
    base_branch = args.base
    focus = getattr(args, "focus", None)

    # Standalone mode (no task ID) - review branch without task context
    standalone = task_id is None

    if not standalone:
        # Task-specific review requires .flow/
        if not ensure_flow_exists():
            error_exit(".flow/ does not exist", use_json=args.json)

        # Validate task ID
        if not is_task_id(task_id):
            error_exit(f"Invalid task ID: {task_id}", use_json=args.json)

        # Load task spec
        flow_dir = get_flow_dir()
        task_spec_path = flow_dir / TASKS_DIR / f"{task_id}.md"

        if not task_spec_path.exists():
            error_exit(f"Task spec not found: {task_spec_path}", use_json=args.json)

        task_spec = task_spec_path.read_text(encoding="utf-8")

    # Get diff summary + content via shared helper
    diff_summary, diff_content = get_diff_context(base_branch)

    # Always embed changed file contents so Codex doesn't waste turns reading
    # files from disk. Without embedding, Codex exhausts its turn budget on
    # sed/rg commands before producing a verdict (observed 114 turns with no
    # verdict on complex epics). The FLOW_CODEX_EMBED_MAX_BYTES budget cap
    # prevents oversized prompts.
    changed_files = get_changed_files(base_branch)
    embedded_content, embed_stats = get_embedded_file_contents(changed_files)

    # Only forbid disk reads when ALL files were fully embedded. If the budget
    # was exhausted or files were truncated, allow Codex to read the remainder
    # from disk so it doesn't review with incomplete context.
    files_embedded = not embed_stats.get("budget_skipped") and not embed_stats.get("truncated")
    if standalone:
        prompt = build_standalone_review_prompt(base_branch, focus, diff_summary, files_embedded)
        # Append embedded files and diff content to standalone prompt
        if diff_content:
            prompt += f"\n\n<diff_content>\n{diff_content}\n</diff_content>"
        if embedded_content:
            prompt += f"\n\n<embedded_files>\n{embedded_content}\n</embedded_files>"
    else:
        # Get context hints for task-specific review
        context_hints = gather_context_hints(base_branch)
        prompt = build_review_prompt(
            "impl", task_spec, context_hints, diff_summary,
            embedded_files=embedded_content, diff_content=diff_content,
            files_embedded=files_embedded
        )

    # Check for existing session in receipt (indicates re-review)
    receipt_path = args.receipt if hasattr(args, "receipt") and args.receipt else None
    session_id, is_rereview = load_receipt(receipt_path)

    # For re-reviews, prepend instruction to re-read changed files
    if is_rereview:
        changed_files = get_changed_files(base_branch)
        if changed_files:
            rereview_preamble = build_rereview_preamble(
                changed_files, "implementation", files_embedded
            )
            prompt = rereview_preamble + prompt

    # Resolve sandbox mode (never pass 'auto' to Codex CLI)
    try:
        sandbox = resolve_codex_sandbox(getattr(args, "sandbox", "auto"))
    except ValueError as e:
        error_exit(str(e), use_json=args.json, code=2)

    # Run codex
    effort = getattr(args, "effort", "high")
    output, thread_id, exit_code, stderr = run_codex_exec(
        prompt, session_id=session_id, sandbox=sandbox, effort=effort
    )

    # Check for sandbox failures (clear stale receipt and exit)
    if is_sandbox_failure(exit_code, output, stderr):
        delete_stale_receipt(receipt_path)
        msg = (
            "Codex sandbox blocked operations. "
            "Try --sandbox danger-full-access (or auto) or set CODEX_SANDBOX=danger-full-access"
        )
        error_exit(msg, use_json=args.json, code=3)

    # Handle non-sandbox failures
    if exit_code != 0:
        delete_stale_receipt(receipt_path)
        msg = (stderr or output or "codex exec failed").strip()
        error_exit(f"codex exec failed: {msg}", use_json=args.json, code=2)

    # Parse verdict
    verdict = parse_codex_verdict(output)

    # Fail if no verdict found (don't let UNKNOWN pass as success)
    if not verdict:
        delete_stale_receipt(receipt_path)
        error_exit(
            "Codex review completed but no verdict found in output. "
            "Expected <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>",
            use_json=args.json,
            code=2,
        )

    # Determine review id (task_id for task reviews, "branch" for standalone)
    review_id = task_id if task_id else "branch"

    # Write receipt if path provided
    if receipt_path:
        save_receipt(
            receipt_path,
            review_type="impl_review",
            review_id=review_id,
            verdict=verdict,
            session_id=thread_id,
            output=output,
            base_branch=base_branch,
            focus=focus,
        )

    # Output
    if args.json:
        json_output(
            {
                "type": "impl_review",
                "id": review_id,
                "verdict": verdict,
                "session_id": thread_id,
                "mode": "codex",
                "standalone": standalone,
                "review": output,  # Full review feedback for fix loop
            }
        )
    else:
        print(output)
        print(f"\nVERDICT={verdict or 'UNKNOWN'}")



def cmd_codex_plan_review(args: argparse.Namespace) -> None:
    """Run plan review via codex exec."""
    if not ensure_flow_exists():
        error_exit(".flow/ does not exist", use_json=args.json)

    epic_id = args.epic

    # Validate epic ID
    if not is_epic_id(epic_id):
        error_exit(f"Invalid epic ID: {epic_id}", use_json=args.json)

    # Require --files argument for plan-review (no automatic file parsing)
    files_arg = getattr(args, "files", None)
    if not files_arg:
        error_exit(
            "plan-review requires --files argument (comma-separated CODE file paths). "
            "On Windows: files are embedded for context. On Unix: used as relevance list. "
            "Example: --files src/main.py,src/utils.py",
            use_json=args.json,
        )

    # Parse and validate files list (repo-relative paths only)
    repo_root = get_repo_root()
    file_paths = []
    invalid_paths = []
    for f in files_arg.split(","):
        f = f.strip()
        if not f:
            continue
        # Check if path is repo-relative and exists
        full_path = (repo_root / f).resolve()
        try:
            full_path.relative_to(repo_root)
            if full_path.exists():
                file_paths.append(f)
            else:
                invalid_paths.append(f"{f} (not found)")
        except ValueError:
            invalid_paths.append(f"{f} (outside repo)")

    if invalid_paths:
        # Warn but continue with valid paths
        print(f"Warning: Skipping invalid paths: {', '.join(invalid_paths)}", file=sys.stderr)

    if not file_paths:
        error_exit(
            "No valid file paths provided. Use --files with comma-separated repo-relative code paths.",
            use_json=args.json,
        )

    # Load epic spec
    flow_dir = get_flow_dir()
    epic_spec_path = flow_dir / SPECS_DIR / f"{epic_id}.md"

    if not epic_spec_path.exists():
        error_exit(f"Epic spec not found: {epic_spec_path}", use_json=args.json)

    epic_spec = epic_spec_path.read_text(encoding="utf-8")

    # Load task specs for this epic
    tasks_dir = flow_dir / TASKS_DIR
    task_specs_parts = []
    for task_file in sorted(tasks_dir.glob(f"{epic_id}.*.md")):
        task_id = task_file.stem
        task_content = task_file.read_text(encoding="utf-8")
        task_specs_parts.append(f"### {task_id}\n\n{task_content}")

    task_specs = "\n\n---\n\n".join(task_specs_parts) if task_specs_parts else ""

    # Always embed file contents so Codex doesn't waste turns reading files
    # from disk. See cmd_codex_impl_review comment for rationale.
    embedded_content, embed_stats = get_embedded_file_contents(file_paths)

    # Get context hints (from main branch for plans)
    base_branch = args.base if hasattr(args, "base") and args.base else "main"
    context_hints = gather_context_hints(base_branch)

    # Only forbid disk reads when ALL files were fully embedded.
    files_embedded = not embed_stats.get("budget_skipped") and not embed_stats.get("truncated")
    prompt = build_review_prompt(
        "plan", epic_spec, context_hints, task_specs=task_specs, embedded_files=embedded_content,
        files_embedded=files_embedded
    )

    # Always include requested files list (even on Unix where they're not embedded)
    # This tells reviewer what code files are relevant to the plan
    if file_paths:
        files_list = "\n".join(f"- {f}" for f in file_paths)
        prompt += f"\n\n<requested_files>\nThe following code files are relevant to this plan:\n{files_list}\n</requested_files>"

    # Check for existing session in receipt (indicates re-review)
    receipt_path = args.receipt if hasattr(args, "receipt") and args.receipt else None
    session_id, is_rereview = load_receipt(receipt_path)

    # For re-reviews, prepend instruction to re-read spec files
    if is_rereview:
        # For plan reviews, epic spec and task specs may change
        # Use relative paths for portability
        repo_root = get_repo_root()
        spec_files = [str(epic_spec_path.relative_to(repo_root))]
        # Add task spec files
        for task_file in sorted(tasks_dir.glob(f"{epic_id}.*.md")):
            spec_files.append(str(task_file.relative_to(repo_root)))
        rereview_preamble = build_rereview_preamble(spec_files, "plan", files_embedded)
        prompt = rereview_preamble + prompt

    # Resolve sandbox mode (never pass 'auto' to Codex CLI)
    try:
        sandbox = resolve_codex_sandbox(getattr(args, "sandbox", "auto"))
    except ValueError as e:
        error_exit(str(e), use_json=args.json, code=2)

    # Run codex
    effort = getattr(args, "effort", "high")
    output, thread_id, exit_code, stderr = run_codex_exec(
        prompt, session_id=session_id, sandbox=sandbox, effort=effort
    )

    # Check for sandbox failures (clear stale receipt and exit)
    if is_sandbox_failure(exit_code, output, stderr):
        delete_stale_receipt(receipt_path)
        msg = (
            "Codex sandbox blocked operations. "
            "Try --sandbox danger-full-access (or auto) or set CODEX_SANDBOX=danger-full-access"
        )
        error_exit(msg, use_json=args.json, code=3)

    # Handle non-sandbox failures
    if exit_code != 0:
        delete_stale_receipt(receipt_path)
        msg = (stderr or output or "codex exec failed").strip()
        error_exit(f"codex exec failed: {msg}", use_json=args.json, code=2)

    # Parse verdict
    verdict = parse_codex_verdict(output)

    # Fail if no verdict found (don't let UNKNOWN pass as success)
    if not verdict:
        delete_stale_receipt(receipt_path)
        error_exit(
            "Codex review completed but no verdict found in output. "
            "Expected <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>",
            use_json=args.json,
            code=2,
        )

    # Write receipt if path provided
    if receipt_path:
        save_receipt(
            receipt_path,
            review_type="plan_review",
            review_id=epic_id,
            verdict=verdict,
            session_id=thread_id,
            output=output,
        )

    # Output
    if args.json:
        json_output(
            {
                "type": "plan_review",
                "id": epic_id,
                "verdict": verdict,
                "session_id": thread_id,
                "mode": "codex",
                "review": output,  # Full review feedback for fix loop
            }
        )
    else:
        print(output)
        print(f"\nVERDICT={verdict or 'UNKNOWN'}")



def cmd_codex_completion_review(args: argparse.Namespace) -> None:
    """Run epic completion review via codex exec.

    Verifies that all epic requirements are implemented before closing.
    Two-phase approach: extract requirements, then verify coverage.
    """
    if not ensure_flow_exists():
        error_exit(".flow/ does not exist", use_json=args.json)

    epic_id = args.epic

    # Validate epic ID
    if not is_epic_id(epic_id):
        error_exit(f"Invalid epic ID: {epic_id}", use_json=args.json)

    flow_dir = get_flow_dir()

    # Load epic spec
    epic_spec_path = flow_dir / SPECS_DIR / f"{epic_id}.md"
    if not epic_spec_path.exists():
        error_exit(f"Epic spec not found: {epic_spec_path}", use_json=args.json)

    epic_spec = epic_spec_path.read_text(encoding="utf-8")

    # Load task specs for this epic
    tasks_dir = flow_dir / TASKS_DIR
    task_specs_parts = []
    for task_file in sorted(tasks_dir.glob(f"{epic_id}.*.md")):
        task_id = task_file.stem
        task_content = task_file.read_text(encoding="utf-8")
        task_specs_parts.append(f"### {task_id}\n\n{task_content}")

    task_specs = "\n\n---\n\n".join(task_specs_parts) if task_specs_parts else ""

    # Get base branch for diff (default to main)
    base_branch = args.base if hasattr(args, "base") and args.base else "main"

    # Get diff summary + content via shared helper
    diff_summary, diff_content = get_diff_context(base_branch)

    # Always embed changed file contents. See cmd_codex_impl_review comment
    # for rationale.
    changed_files = get_changed_files(base_branch)
    embedded_content, embed_stats = get_embedded_file_contents(changed_files)

    # Only forbid disk reads when ALL files were fully embedded.
    files_embedded = not embed_stats.get("budget_skipped") and not embed_stats.get("truncated")
    prompt = build_completion_review_prompt(
        epic_spec,
        task_specs,
        diff_summary,
        diff_content,
        embedded_files=embedded_content,
        files_embedded=files_embedded,
    )

    # Check for existing session in receipt (indicates re-review)
    receipt_path = args.receipt if hasattr(args, "receipt") and args.receipt else None
    session_id, is_rereview = load_receipt(receipt_path)

    # For re-reviews, prepend instruction to re-read changed files
    if is_rereview:
        changed_files = get_changed_files(base_branch)
        if changed_files:
            rereview_preamble = build_rereview_preamble(
                changed_files, "completion", files_embedded
            )
            prompt = rereview_preamble + prompt

    # Resolve sandbox mode
    try:
        sandbox = resolve_codex_sandbox(getattr(args, "sandbox", "auto"))
    except ValueError as e:
        error_exit(str(e), use_json=args.json, code=2)

    # Run codex
    effort = getattr(args, "effort", "high")
    output, thread_id, exit_code, stderr = run_codex_exec(
        prompt, session_id=session_id, sandbox=sandbox, effort=effort
    )

    # Check for sandbox failures
    if is_sandbox_failure(exit_code, output, stderr):
        delete_stale_receipt(receipt_path)
        msg = (
            "Codex sandbox blocked operations. "
            "Try --sandbox danger-full-access (or auto) or set CODEX_SANDBOX=danger-full-access"
        )
        error_exit(msg, use_json=args.json, code=3)

    # Handle non-sandbox failures
    if exit_code != 0:
        delete_stale_receipt(receipt_path)
        msg = (stderr or output or "codex exec failed").strip()
        error_exit(f"codex exec failed: {msg}", use_json=args.json, code=2)

    # Parse verdict
    verdict = parse_codex_verdict(output)

    # Fail if no verdict found
    if not verdict:
        delete_stale_receipt(receipt_path)
        error_exit(
            "Codex review completed but no verdict found in output. "
            "Expected <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>",
            use_json=args.json,
            code=2,
        )

    # Preserve session_id for continuity (avoid clobbering on resumed sessions)
    session_id_to_write = thread_id or session_id

    # Write receipt if path provided
    if receipt_path:
        save_receipt(
            receipt_path,
            review_type="completion_review",
            review_id=epic_id,
            verdict=verdict,
            session_id=session_id_to_write,
            output=output,
            base_branch=base_branch,
        )

    # Output
    if args.json:
        json_output(
            {
                "type": "completion_review",
                "id": epic_id,
                "base": base_branch,
                "verdict": verdict,
                "session_id": session_id_to_write,
                "mode": "codex",
                "review": output,
            }
        )
    else:
        print(output)
        print(f"\nVERDICT={verdict or 'UNKNOWN'}")


# ─────────────────────────────────────────────────────────────────────────────
# Checkpoint commands
# ─────────────────────────────────────────────────────────────────────────────

