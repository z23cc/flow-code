"""Workflow state transition commands: ready, next, queue, start, done, block, restart, state-path, migrate-state, worker-prompt, worker-phase."""

import argparse
import json
import re
from datetime import datetime as _dt, timezone as _tz
from pathlib import Path
from typing import Any

from flowctl.core.constants import (
    EPICS_DIR,
    PHASE_DEFS,
    PHASE_SEQ_DEFAULT,
    PHASE_SEQ_REVIEW,
    PHASE_SEQ_TDD,
    RUNTIME_FIELDS,
    TASKS_DIR,
)
from flowctl.core.ids import (
    epic_id_from_task,
    is_epic_id,
    is_task_id,
    normalize_epic,
    normalize_task,
    parse_id,
    task_priority,
)
from flowctl.core.io import (
    atomic_write,
    atomic_write_json,
    error_exit,
    json_output,
    load_json,
    load_json_or_exit,
    now_iso,
    read_text_or_exit,
)
from flowctl.core.paths import (
    ensure_flow_exists,
    get_flow_dir,
    get_state_dir,
)
from flowctl.core.state import (
    get_state_store,
    load_task_definition,
    load_all_tasks_with_state,
    load_task_with_state,
    reset_task_runtime,
    save_task_runtime,
)
from flowctl.core.git import get_actor
from flowctl.commands.task import (
    clear_task_evidence,
    find_dependents,
    get_task_section,
    patch_task_section,
)
from flowctl.core.constants import REVIEWS_DIR


# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------

def cmd_ready(args: argparse.Namespace) -> None:
    """List ready tasks for an epic."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    if not is_epic_id(args.epic):
        error_exit(
            f"Invalid epic ID: {args.epic}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)", use_json=args.json
        )

    flow_dir = get_flow_dir()
    epic_path = flow_dir / EPICS_DIR / f"{args.epic}.json"

    if not epic_path.exists():
        error_exit(f"Epic {args.epic} not found", use_json=args.json)

    # MU-2: Get current actor for display (marks your tasks)
    current_actor = get_actor()

    # Get all tasks for epic (with merged runtime state)
    tasks_dir = flow_dir / TASKS_DIR
    if not tasks_dir.exists():
        error_exit(
            f"{TASKS_DIR}/ missing. Run 'flowctl init' or fix repo state.",
            use_json=args.json,
        )
    tasks = {}
    for task_file in tasks_dir.glob(f"{args.epic}.*.json"):
        task_id = task_file.stem
        if not is_task_id(task_id):
            continue  # Skip non-task files (e.g., fn-1.2-review.json)
        task_data = load_task_with_state(task_id, use_json=args.json)
        if "id" not in task_data:
            continue  # Skip artifact files (GH-21)
        tasks[task_data["id"]] = task_data

    # Find ready tasks (status=todo, all deps done)
    ready = []
    in_progress = []
    blocked = []

    for task_id, task in tasks.items():
        # MU-2: Track in_progress tasks separately
        if task["status"] == "in_progress":
            in_progress.append(task)
            continue

        if task["status"] in ("done", "skipped"):
            continue

        if task["status"] == "blocked":
            blocked.append({"task": task, "blocked_by": ["status=blocked"]})
            continue

        # Check all deps are done (skipped counts as satisfied)
        deps_done = True
        blocking_deps = []
        for dep in task["depends_on"]:
            if dep not in tasks:
                deps_done = False
                blocking_deps.append(dep)
            elif tasks[dep]["status"] not in ("done", "skipped"):
                deps_done = False
                blocking_deps.append(dep)

        if deps_done:
            ready.append(task)
        else:
            blocked.append({"task": task, "blocked_by": blocking_deps})

    # Sort by numeric suffix
    def sort_key(t):
        _, task_num = parse_id(t["id"])
        return (
            task_priority(t),
            task_num if task_num is not None else 0,
            t.get("title", ""),
        )

    ready.sort(key=sort_key)
    in_progress.sort(key=sort_key)
    blocked.sort(key=lambda x: sort_key(x["task"]))

    if args.json:
        json_output(
            {
                "epic": args.epic,
                "actor": current_actor,
                "ready": [
                    {"id": t["id"], "title": t["title"], "depends_on": t["depends_on"]}
                    for t in ready
                ],
                "in_progress": [
                    {"id": t["id"], "title": t["title"], "assignee": t.get("assignee")}
                    for t in in_progress
                ],
                "blocked": [
                    {
                        "id": b["task"]["id"],
                        "title": b["task"]["title"],
                        "blocked_by": b["blocked_by"],
                    }
                    for b in blocked
                ],
            }
        )
    else:
        print(f"Ready tasks for {args.epic} (actor: {current_actor}):")
        if ready:
            for t in ready:
                print(f"  {t['id']}: {t['title']}")
        else:
            print("  (none)")
        if in_progress:
            print("\nIn progress:")
            for t in in_progress:
                assignee = t.get("assignee") or "unknown"
                marker = " (you)" if assignee == current_actor else ""
                print(f"  {t['id']}: {t['title']} [{assignee}]{marker}")
        if blocked:
            print("\nBlocked:")
            for b in blocked:
                print(
                    f"  {b['task']['id']}: {b['task']['title']} (by: {', '.join(b['blocked_by'])})"
                )


def cmd_next(args: argparse.Namespace) -> None:
    """Select the next plan/work unit."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    flow_dir = get_flow_dir()

    # Resolve epics list
    epic_ids: list[str] = []
    if args.epics_file:
        data = load_json_or_exit(
            Path(args.epics_file), "Epics file", use_json=args.json
        )
        epics_val = data.get("epics")
        if not isinstance(epics_val, list):
            error_exit(
                "Epics file must be JSON with key 'epics' as a list", use_json=args.json
            )
        for e in epics_val:
            if not isinstance(e, str) or not is_epic_id(e):
                error_exit(f"Invalid epic ID in epics file: {e}", use_json=args.json)
            epic_ids.append(e)
    else:
        epics_dir = flow_dir / EPICS_DIR
        if epics_dir.exists():
            for epic_file in sorted(epics_dir.glob("fn-*.json")):
                # Match: fn-N.json, fn-N-xxx.json (short), fn-N-slug.json (long)
                match = re.match(
                    r"^fn-(\d+)(?:-[a-z0-9][a-z0-9-]*[a-z0-9]|-[a-z0-9]{1,3})?\.json$",
                    epic_file.name,
                )
                if match:
                    epic_ids.append(epic_file.stem)  # Use full ID from filename
            epic_ids.sort(key=lambda e: parse_id(e)[0] or 0)

    current_actor = get_actor()

    def sort_key(t: dict) -> tuple[int, int]:
        _, task_num = parse_id(t["id"])
        return (task_priority(t), task_num if task_num is not None else 0)

    blocked_epics: dict[str, list[str]] = {}

    for epic_id in epic_ids:
        epic_path = flow_dir / EPICS_DIR / f"{epic_id}.json"
        if not epic_path.exists():
            if args.epics_file:
                error_exit(f"Epic {epic_id} not found", use_json=args.json)
            continue

        epic_data = normalize_epic(
            load_json_or_exit(epic_path, f"Epic {epic_id}", use_json=args.json)
        )
        if epic_data.get("status") == "done":
            continue

        # Skip epics blocked by epic-level dependencies
        blocked_by: list[str] = []
        for dep in epic_data.get("depends_on_epics", []) or []:
            if dep == epic_id:
                continue
            dep_path = flow_dir / EPICS_DIR / f"{dep}.json"
            if not dep_path.exists():
                blocked_by.append(dep)
                continue
            dep_data = normalize_epic(
                load_json_or_exit(dep_path, f"Epic {dep}", use_json=args.json)
            )
            if dep_data.get("status") != "done":
                blocked_by.append(dep)
        if blocked_by:
            blocked_epics[epic_id] = blocked_by
            continue

        if args.require_plan_review and epic_data.get("plan_review_status") != "ship":
            if args.json:
                json_output(
                    {
                        "status": "plan",
                        "epic": epic_id,
                        "task": None,
                        "reason": "needs_plan_review",
                    }
                )
            else:
                print(f"plan {epic_id} needs_plan_review")
            return

        tasks_dir = flow_dir / TASKS_DIR
        if not tasks_dir.exists():
            error_exit(
                f"{TASKS_DIR}/ missing. Run 'flowctl init' or fix repo state.",
                use_json=args.json,
            )

        # Batch-load all tasks for this epic in a single directory scan
        tasks = load_all_tasks_with_state(epic_id)

        # Resume in_progress tasks owned by current actor
        in_progress = [
            t
            for t in tasks.values()
            if t.get("status") == "in_progress" and t.get("assignee") == current_actor
        ]
        in_progress.sort(key=sort_key)
        if in_progress:
            task_id = in_progress[0]["id"]
            if args.json:
                json_output(
                    {
                        "status": "work",
                        "epic": epic_id,
                        "task": task_id,
                        "reason": "resume_in_progress",
                    }
                )
            else:
                print(f"work {task_id} resume_in_progress")
            return

        # Ready tasks by deps + priority
        ready: list[dict] = []
        for task in tasks.values():
            if task.get("status") != "todo":
                continue
            if task.get("status") == "blocked":
                continue
            deps_done = True
            for dep in task.get("depends_on", []):
                dep_task = tasks.get(dep)
                if not dep_task or dep_task.get("status") not in ("done", "skipped"):
                    deps_done = False
                    break
            if deps_done:
                ready.append(task)

        ready.sort(key=sort_key)
        if ready:
            task_id = ready[0]["id"]
            if args.json:
                json_output(
                    {
                        "status": "work",
                        "epic": epic_id,
                        "task": task_id,
                        "reason": "ready_task",
                    }
                )
            else:
                print(f"work {task_id} ready_task")
            return

        # Check if all tasks are done and completion review is needed
        if (
            args.require_completion_review
            and tasks
            and all(t.get("status") == "done" for t in tasks.values())
            and epic_data.get("completion_review_status") != "ship"
        ):
            if args.json:
                json_output(
                    {
                        "status": "completion_review",
                        "epic": epic_id,
                        "task": None,
                        "reason": "needs_completion_review",
                    }
                )
            else:
                print(f"completion_review {epic_id} needs_completion_review")
            return

    if args.json:
        payload = {"status": "none", "epic": None, "task": None, "reason": "none"}
        if blocked_epics:
            payload["reason"] = "blocked_by_epic_deps"
            payload["blocked_epics"] = blocked_epics
        json_output(payload)
    else:
        if blocked_epics:
            print("none blocked_by_epic_deps")
            for epic_id, deps in blocked_epics.items():
                print(f"  {epic_id}: {', '.join(deps)}")
        else:
            print("none")


def cmd_queue(args: argparse.Namespace) -> None:
    """Show multi-epic queue status with dependency visualization."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    flow_dir = get_flow_dir()
    epics_dir = flow_dir / EPICS_DIR
    tasks_dir = flow_dir / TASKS_DIR

    if not epics_dir.exists():
        error_exit("No epics found.", use_json=args.json)

    current_actor = get_actor()

    # Collect all epics
    epics: list[dict] = []
    for epic_file in sorted(epics_dir.glob("fn-*.json")):
        match = re.match(
            r"^fn-(\d+)(?:-[a-z0-9][a-z0-9-]*[a-z0-9]|-[a-z0-9]{1,3})?\.json$",
            epic_file.name,
        )
        if not match:
            continue
        epic_data = normalize_epic(
            load_json_or_exit(epic_file, f"Epic {epic_file.stem}", use_json=args.json)
        )
        epic_id = epic_data.get("id", epic_file.stem)

        # Count tasks by status
        task_counts = {"todo": 0, "in_progress": 0, "done": 0, "blocked": 0, "ready": 0}
        task_list = []
        if tasks_dir.exists():
            for task_file in tasks_dir.glob(f"{epic_id}.*.json"):
                task_id = task_file.stem
                if not is_task_id(task_id):
                    continue
                task_data = load_task_with_state(task_id, use_json=args.json)
                if "id" not in task_data:
                    continue
                task_list.append(task_data)
                status = task_data.get("status", "todo")
                if status in task_counts:
                    task_counts[status] += 1

        # Calculate ready tasks
        all_tasks = {t["id"]: t for t in task_list}
        for task in task_list:
            if task.get("status") != "todo":
                continue
            deps_done = all(
                all_tasks.get(d, {}).get("status") in ("done", "skipped")
                for d in task.get("depends_on", [])
            )
            if deps_done:
                task_counts["ready"] += 1

        # Check epic-level deps
        epic_deps = epic_data.get("depends_on_epics", []) or []
        blocked_by: list[str] = []
        for dep in epic_deps:
            if dep == epic_id:
                continue
            dep_path = epics_dir / f"{dep}.json"
            if not dep_path.exists():
                blocked_by.append(dep)
                continue
            dep_data = normalize_epic(
                load_json_or_exit(dep_path, f"Epic {dep}", use_json=args.json)
            )
            if dep_data.get("status") != "done":
                blocked_by.append(dep)

        total_tasks = sum(task_counts.values())
        epics.append({
            "id": epic_id,
            "title": epic_data.get("title", ""),
            "status": epic_data.get("status", "open"),
            "plan_review_status": epic_data.get("plan_review_status", "unknown"),
            "completion_review_status": epic_data.get("completion_review_status", "unknown"),
            "depends_on_epics": epic_deps,
            "blocked_by": blocked_by,
            "tasks": task_counts,
            "total_tasks": total_tasks,
            "progress": round(task_counts["done"] / total_tasks * 100) if total_tasks > 0 else 0,
        })

    # Sort: open epics first (with unblocked before blocked), then done
    def epic_sort_key(e: dict) -> tuple:
        status_order = 0 if e["status"] != "done" else 2
        if e["blocked_by"]:
            status_order = 1
        epic_num, _ = parse_id(e["id"])
        return (status_order, epic_num or 0)

    epics.sort(key=epic_sort_key)

    if args.json:
        json_output({"actor": current_actor, "epics": epics, "total": len(epics)})
    else:
        open_epics = [e for e in epics if e["status"] != "done"]
        done_epics = [e for e in epics if e["status"] == "done"]

        print(f"Queue ({len(open_epics)} open, {len(done_epics)} done):\n")

        for e in epics:
            if e["status"] == "done":
                status_icon = "\u2713"
            elif e["blocked_by"]:
                status_icon = "\u2298"
            elif e["tasks"]["ready"] > 0:
                status_icon = "\u25b6"
            else:
                status_icon = "\u25cb"

            tc = e["tasks"]
            bar_len = 20
            done_bars = round(e["progress"] / 100 * bar_len) if e["total_tasks"] > 0 else 0
            bar = "\u2588" * done_bars + "\u2591" * (bar_len - done_bars)

            print(f"  {status_icon} {e['id']}: {e['title']}")
            print(f"    [{bar}] {e['progress']}%  done={tc['done']} ready={tc['ready']} todo={tc['todo']} in_progress={tc['in_progress']} blocked={tc['blocked']}")

            if e["blocked_by"]:
                print(f"    \u2298 blocked by: {', '.join(e['blocked_by'])}")
            if e["depends_on_epics"] and not e["blocked_by"]:
                print(f"    \u2192 deps (resolved): {', '.join(e['depends_on_epics'])}")

            print()


def cmd_start(args: argparse.Namespace) -> None:
    """Start a task (set status to in_progress)."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    if not is_task_id(args.id):
        error_exit(
            f"Invalid task ID: {args.id}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)", use_json=args.json
        )

    # Load task definition for dependency info (outside lock)
    # Normalize to handle legacy "deps" field
    task_def = normalize_task(load_task_definition(args.id, use_json=args.json))
    depends_on = task_def.get("depends_on", []) or []

    # Validate all dependencies are done/skipped (outside lock - this is read-only check)
    if not args.force:
        for dep in depends_on:
            dep_data = load_task_with_state(dep, use_json=args.json)
            if dep_data["status"] not in ("done", "skipped"):
                error_exit(
                    f"Cannot start task {args.id}: dependency {dep} is '{dep_data['status']}', not 'done'. "
                    f"Complete dependencies first or use --force to override.",
                    use_json=args.json,
                )

    current_actor = get_actor()
    store = get_state_store()

    # Atomic claim: validation + write inside lock to prevent race conditions
    with store.lock_task(args.id):
        # Re-load runtime state inside lock for accurate check
        runtime = store.load_runtime(args.id)
        if runtime is None:
            # Backward compat: extract from definition
            runtime = {k: task_def[k] for k in RUNTIME_FIELDS if k in task_def}
            if not runtime:
                runtime = {"status": "todo"}

        status = runtime.get("status", "todo")
        existing_assignee = runtime.get("assignee")

        # Cannot start done task
        if status == "done":
            error_exit(
                f"Cannot start task {args.id}: status is 'done'.", use_json=args.json
            )

        # Blocked requires --force
        if status == "blocked" and not args.force:
            error_exit(
                f"Cannot start task {args.id}: status is 'blocked'. Use --force to override.",
                use_json=args.json,
            )

        # Check if claimed by someone else (unless --force)
        if not args.force and existing_assignee and existing_assignee != current_actor:
            error_exit(
                f"Cannot start task {args.id}: claimed by '{existing_assignee}'. "
                f"Use --force to override.",
                use_json=args.json,
            )

        # Validate task is in todo status (unless --force or resuming own task)
        if not args.force and status != "todo":
            # Allow resuming your own in_progress task
            if not (status == "in_progress" and existing_assignee == current_actor):
                error_exit(
                    f"Cannot start task {args.id}: status is '{status}', expected 'todo'. "
                    f"Use --force to override.",
                    use_json=args.json,
                )

        # Build runtime state updates
        runtime_updates = {**runtime, "status": "in_progress", "updated_at": now_iso()}
        if not existing_assignee:
            runtime_updates["assignee"] = current_actor
            runtime_updates["claimed_at"] = now_iso()
        if args.note:
            runtime_updates["claim_note"] = args.note
        elif args.force and existing_assignee and existing_assignee != current_actor:
            # Force override: note the takeover
            runtime_updates["assignee"] = current_actor
            runtime_updates["claimed_at"] = now_iso()
            if not args.note:
                runtime_updates["claim_note"] = f"Taken over from {existing_assignee}"

        # Write inside lock
        store.save_runtime(args.id, runtime_updates)

    # NOTE: We no longer update epic timestamp on task start/done.
    # Epic timestamp only changes on epic-level operations (set-plan, close).
    # This reduces merge conflicts in multi-user scenarios.

    if args.json:
        json_output(
            {
                "id": args.id,
                "status": "in_progress",
                "message": f"Task {args.id} started",
            }
        )
    else:
        print(f"Task {args.id} started")


def cmd_done(args: argparse.Namespace) -> None:
    """Complete a task with summary and evidence."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    if not is_task_id(args.id):
        error_exit(
            f"Invalid task ID: {args.id}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)", use_json=args.json
        )

    flow_dir = get_flow_dir()
    task_spec_path = flow_dir / TASKS_DIR / f"{args.id}.md"

    # Load task with merged runtime state (fail early before any writes)
    task_data = load_task_with_state(args.id, use_json=args.json)

    # MU-2: Require in_progress status (unless --force)
    if not args.force and task_data["status"] != "in_progress":
        status = task_data["status"]
        if status == "done":
            error_exit(
                f"Task {args.id} is already done.",
                use_json=args.json,
            )
        else:
            error_exit(
                f"Task {args.id} is '{status}', not 'in_progress'. Use --force to override.",
                use_json=args.json,
            )

    # MU-2: Prevent cross-actor completion (unless --force)
    current_actor = get_actor()
    existing_assignee = task_data.get("assignee")
    if not args.force and existing_assignee and existing_assignee != current_actor:
        error_exit(
            f"Cannot complete task {args.id}: claimed by '{existing_assignee}'. "
            f"Use --force to override.",
            use_json=args.json,
        )

    # Get summary: file > inline > default
    summary: str
    if args.summary_file:
        summary = read_text_or_exit(
            Path(args.summary_file), "Summary file", use_json=args.json
        )
    elif args.summary:
        summary = args.summary
    else:
        summary = "- Task completed"

    # Get evidence: file-or-inline > inline > default
    evidence: dict
    if args.evidence_json:
        val = args.evidence_json.strip()
        if val.startswith("{"):
            # Inline JSON string passed directly
            evidence_raw = val
        else:
            # File path
            evidence_raw = read_text_or_exit(
                Path(val), "Evidence file", use_json=args.json
            )
        try:
            evidence = json.loads(evidence_raw)
        except json.JSONDecodeError as e:
            error_exit(f"Evidence JSON invalid: {e}", use_json=args.json)
    elif args.evidence:
        try:
            evidence = json.loads(args.evidence)
        except json.JSONDecodeError as e:
            error_exit(f"Evidence invalid JSON: {e}", use_json=args.json)
    else:
        evidence = {"commits": [], "tests": [], "prs": []}

    if not isinstance(evidence, dict):
        error_exit(
            "Evidence JSON must be an object with keys: commits/tests/prs",
            use_json=args.json,
        )

    # Calculate duration from claimed_at (start time) to now
    duration_seconds = None
    claimed_at = task_data.get("claimed_at")
    if claimed_at:
        try:
            _start = _dt.fromisoformat(claimed_at.replace("Z", "+00:00"))
            duration_seconds = round((_dt.now(_tz.utc) - _start).total_seconds())
        except (ValueError, TypeError):
            pass

    # Validate workspace_changes if present (warn on bad format, don't block)
    ws_changes = evidence.get("workspace_changes")
    ws_warning = None
    if ws_changes is not None:
        if not isinstance(ws_changes, dict):
            ws_warning = "workspace_changes must be an object"
            ws_changes = None
        else:
            required_ws_keys = {"baseline_rev", "final_rev", "files_changed", "insertions", "deletions"}
            missing_ws = required_ws_keys - set(ws_changes.keys())
            if missing_ws:
                ws_warning = f"workspace_changes missing keys: {', '.join(sorted(missing_ws))}"

    # Format evidence as markdown (coerce to strings, handle string-vs-array)
    def to_list(val: Any) -> list:
        if val is None:
            return []
        if isinstance(val, str):
            return [val] if val else []
        return list(val)

    evidence_md = []
    commits = [str(x) for x in to_list(evidence.get("commits"))]
    tests = [str(x) for x in to_list(evidence.get("tests"))]
    prs = [str(x) for x in to_list(evidence.get("prs"))]
    evidence_md.append(f"- Commits: {', '.join(commits)}" if commits else "- Commits:")
    evidence_md.append(f"- Tests: {', '.join(tests)}" if tests else "- Tests:")
    evidence_md.append(f"- PRs: {', '.join(prs)}" if prs else "- PRs:")
    if ws_changes and not ws_warning:
        evidence_md.append(
            f"- Workspace: {ws_changes.get('files_changed', 0)} files changed, "
            f"+{ws_changes.get('insertions', 0)} -{ws_changes.get('deletions', 0)} "
            f"({ws_changes.get('baseline_rev', '?')[:7]}..{ws_changes.get('final_rev', '?')[:7]})"
        )
    if duration_seconds is not None:
        mins, secs = divmod(duration_seconds, 60)
        dur_str = f"{mins}m {secs}s" if mins else f"{secs}s"
        evidence_md.append(f"- Duration: {dur_str}")
    evidence_content = "\n".join(evidence_md)

    # Read current spec
    current_spec = read_text_or_exit(
        task_spec_path, f"Task {args.id} spec", use_json=args.json
    )

    # Patch sections
    try:
        updated_spec = patch_task_section(current_spec, "## Done summary", summary)
        updated_spec = patch_task_section(updated_spec, "## Evidence", evidence_content)
    except ValueError as e:
        error_exit(str(e), use_json=args.json)

    # Add duration to evidence
    if duration_seconds is not None:
        evidence["duration_seconds"] = duration_seconds

    # All validation passed - now write.
    # Write runtime state FIRST (authoritative source via load_task_with_state),
    # so a crash after this point still marks the task as done.
    runtime_done = {"status": "done", "evidence": evidence, "completed_at": now_iso()}
    if duration_seconds is not None:
        runtime_done["duration_seconds"] = duration_seconds
    save_task_runtime(args.id, runtime_done)

    # Then write spec (summary + evidence markdown)
    atomic_write(task_spec_path, updated_spec)

    # Archive review receipt if present in evidence
    review_receipt = evidence.get("review_receipt")
    if review_receipt and isinstance(review_receipt, dict):
        reviews_dir = flow_dir / REVIEWS_DIR
        reviews_dir.mkdir(parents=True, exist_ok=True)
        mode = review_receipt.get("mode", "unknown")
        rtype = review_receipt.get("type", "review")
        receipt_filename = f"{rtype}-{args.id}-{mode}.json"
        atomic_write_json(reviews_dir / receipt_filename, review_receipt)

    # NOTE: We no longer update epic timestamp on task done.
    # This reduces merge conflicts in multi-user scenarios.

    if args.json:
        result = {"id": args.id, "status": "done", "message": f"Task {args.id} completed"}
        if duration_seconds is not None:
            result["duration_seconds"] = duration_seconds
        if ws_warning:
            result["warning"] = ws_warning
        json_output(result)
    else:
        duration_str = ""
        if duration_seconds is not None:
            mins, secs = divmod(duration_seconds, 60)
            duration_str = f" ({mins}m {secs}s)" if mins else f" ({secs}s)"
        print(f"Task {args.id} completed{duration_str}")
        if ws_warning:
            print(f"  warning: {ws_warning}")


def cmd_block(args: argparse.Namespace) -> None:
    """Block a task with a reason."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    if not is_task_id(args.id):
        error_exit(
            f"Invalid task ID: {args.id}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)", use_json=args.json
        )

    flow_dir = get_flow_dir()
    task_spec_path = flow_dir / TASKS_DIR / f"{args.id}.md"

    # Load task with merged runtime state
    task_data = load_task_with_state(args.id, use_json=args.json)

    if task_data["status"] == "done":
        error_exit(
            f"Cannot block task {args.id}: status is 'done'.", use_json=args.json
        )

    reason = read_text_or_exit(
        Path(args.reason_file), "Reason file", use_json=args.json
    ).strip()
    if not reason:
        error_exit("Reason file is empty", use_json=args.json)

    current_spec = read_text_or_exit(
        task_spec_path, f"Task {args.id} spec", use_json=args.json
    )
    summary = get_task_section(current_spec, "## Done summary")
    if summary.strip().lower() in ["tbd", ""]:
        new_summary = f"Blocked:\n{reason}"
    else:
        new_summary = f"{summary}\n\nBlocked:\n{reason}"

    try:
        updated_spec = patch_task_section(current_spec, "## Done summary", new_summary)
    except ValueError as e:
        error_exit(str(e), use_json=args.json)

    atomic_write(task_spec_path, updated_spec)

    # Write runtime state to state-dir (not definition file)
    save_task_runtime(args.id, {"status": "blocked", "blocked_reason": reason})

    if args.json:
        json_output(
            {"id": args.id, "status": "blocked", "message": f"Task {args.id} blocked"}
        )
    else:
        print(f"Task {args.id} blocked")


def cmd_restart(args: argparse.Namespace) -> None:
    """Restart a task and cascade-reset all downstream dependents.

    Unlike `task reset`, this is a top-level convenience command that always
    cascades. It also supports --dry-run and --force for in_progress dependents.
    """
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    task_id = args.id
    if not is_task_id(task_id):
        error_exit(
            f"Invalid task ID: {task_id}. Expected format: fn-N.M or fn-N-slug.M",
            use_json=args.json,
        )

    flow_dir = get_flow_dir()
    task_json_path = flow_dir / TASKS_DIR / f"{task_id}.json"

    if not task_json_path.exists():
        error_exit(f"Task {task_id} not found", use_json=args.json)

    # Load task with merged runtime state
    task_data = load_task_with_state(task_id, use_json=args.json)

    # Check epic not closed
    epic_id = epic_id_from_task(task_id)
    epic_path = flow_dir / EPICS_DIR / f"{epic_id}.json"
    if epic_path.exists():
        epic_data = load_json_or_exit(epic_path, f"Epic {epic_id}", use_json=args.json)
        if epic_data.get("status") == "done":
            error_exit(
                f"Cannot restart task in closed epic {epic_id}", use_json=args.json
            )

    current_status = task_data.get("status", "todo")

    # Find all downstream dependents (always cascade)
    dependents = find_dependents(task_id, same_epic=True)

    # Check for in_progress tasks (target + dependents)
    in_progress_ids = []
    if current_status == "in_progress":
        in_progress_ids.append(task_id)
    for dep_id in dependents:
        dep_data = load_task_with_state(dep_id, use_json=args.json)
        if dep_data.get("status") == "in_progress":
            in_progress_ids.append(dep_id)

    if in_progress_ids and not args.force:
        error_exit(
            f"Cannot restart: tasks in progress: {', '.join(in_progress_ids)}. "
            f"Use --force to override.",
            use_json=args.json,
        )

    # Build the full reset list
    all_ids = [task_id] + dependents
    to_reset = []
    skipped = []
    for tid in all_ids:
        td = load_task_with_state(tid, use_json=args.json)
        st = td.get("status", "todo")
        if st == "todo":
            skipped.append(tid)
            continue
        to_reset.append(tid)

    # Dry-run mode
    if args.dry_run:
        if args.json:
            json_output({
                "dry_run": True,
                "would_reset": to_reset,
                "already_todo": skipped,
                "in_progress_overridden": in_progress_ids if args.force else [],
            })
        else:
            print(f"Dry run \u2014 would restart {len(to_reset)} task(s):")
            for tid in to_reset:
                td = load_task_with_state(tid, use_json=args.json)
                st = td.get("status", "todo")
                marker = " (force)" if tid in in_progress_ids else ""
                print(f"  {tid}  {st} -> todo{marker}")
            if skipped:
                print(f"Already todo: {', '.join(skipped)}")
        return

    # Execute reset
    reset_ids = []
    for tid in to_reset:
        # Reset runtime state
        reset_task_runtime(tid)

        # Clear legacy fields from definition file
        tid_path = flow_dir / TASKS_DIR / f"{tid}.json"
        if tid_path.exists():
            def_data = load_json(tid_path)
            for field in ("blocked_reason", "completed_at", "assignee",
                          "claimed_at", "claim_note", "evidence"):
                def_data.pop(field, None)
            def_data.pop("status", None)
            def_data["updated_at"] = now_iso()
            atomic_write_json(tid_path, def_data)

        # Clear evidence from spec
        clear_task_evidence(tid)
        reset_ids.append(tid)

    if args.json:
        json_output({
            "success": True,
            "reset": reset_ids,
            "skipped": skipped,
            "cascade_from": task_id,
        })
    else:
        if not reset_ids:
            print(f"Nothing to restart \u2014 {task_id} and dependents already todo.")
        else:
            print(f"Restarted from {task_id} (cascade: {len(reset_ids) - (1 if task_id in reset_ids else 0)} downstream):\n")
            for tid in reset_ids:
                marker = " (target)" if tid == task_id else ""
                print(f"  {tid}  -> todo{marker}")


def cmd_state_path(args: argparse.Namespace) -> None:
    """Show resolved state directory path."""
    state_dir = get_state_dir()

    if args.task:
        if not is_task_id(args.task):
            error_exit(
                f"Invalid task ID: {args.task}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
                use_json=args.json,
            )
        state_path = state_dir / "tasks" / f"{args.task}.state.json"
        if args.json:
            json_output({"state_dir": str(state_dir), "task_state_path": str(state_path)})
        else:
            print(state_path)
    else:
        if args.json:
            json_output({"state_dir": str(state_dir)})
        else:
            print(state_dir)


def cmd_migrate_state(args: argparse.Namespace) -> None:
    """Migrate runtime state from definition files to state-dir."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    flow_dir = get_flow_dir()
    tasks_dir = flow_dir / TASKS_DIR
    store = get_state_store()

    migrated = []
    skipped = []

    if not tasks_dir.exists():
        if args.json:
            json_output({"migrated": [], "skipped": [], "message": "No tasks directory"})
        else:
            print("No tasks directory found.")
        return

    for task_file in tasks_dir.glob("fn-*.json"):
        task_id = task_file.stem
        if not is_task_id(task_id):
            continue  # Skip non-task files (e.g., fn-1.2-review.json)

        # Check if state file already exists
        if store.load_runtime(task_id) is not None:
            skipped.append(task_id)
            continue

        # Load definition and extract runtime fields
        try:
            definition = load_json(task_file)
        except Exception:
            skipped.append(task_id)
            continue

        runtime = {k: definition[k] for k in RUNTIME_FIELDS if k in definition}
        if not runtime or runtime.get("status") == "todo":
            # No runtime state to migrate
            skipped.append(task_id)
            continue

        # Write runtime state
        store.save_runtime(task_id, runtime)
        migrated.append(task_id)

        # Optionally clean definition file (only with --clean flag)
        if args.clean:
            clean_def = {k: v for k, v in definition.items() if k not in RUNTIME_FIELDS}
            atomic_write_json(task_file, clean_def)

    if args.json:
        json_output({
            "migrated": migrated,
            "skipped": skipped,
            "cleaned": args.clean,
        })
    else:
        print(f"Migrated: {len(migrated)} tasks")
        if migrated:
            for t in migrated:
                print(f"  {t}")
        print(f"Skipped: {len(skipped)} tasks (already migrated or no state)")
        if args.clean:
            print("Definition files cleaned (runtime fields removed)")


# ---------------------------------------------------------------------------
# worker-prompt: conditional prompt trimming
# ---------------------------------------------------------------------------

def _get_plugin_root() -> Path:
    """Get the flow-code plugin root directory.

    This file lives at scripts/flowctl/commands/workflow.py,
    so plugin root is 4 levels up.
    """
    return Path(__file__).resolve().parent.parent.parent.parent


def _parse_worker_sections(content: str) -> list[dict[str, str]]:
    """Parse worker.md into tagged sections using HTML comment markers.

    Returns a list of dicts: {"tag": "core"|"team"|"tdd"|"review"|"memory", "content": "..."}
    Content between sections (frontmatter, blank lines between markers) is discarded.
    """
    sections: list[dict[str, str]] = []
    pattern = re.compile(
        r"<!-- section:(\w+) -->\n(.*?)<!-- /section:\1 -->",
        re.DOTALL,
    )
    for m in pattern.finditer(content):
        tag = m.group(1)
        body = m.group(2).strip()
        if body:
            sections.append({"tag": tag, "content": body})
    return sections


def _extract_phase_content(worker_md_content: str, include_tags: set[str]) -> dict[str, str]:
    """Extract per-phase content from worker.md, filtered by section tags.

    Returns a dict mapping phase_id -> markdown content for that phase.
    Phases are identified by ``## Phase N:`` headings within the assembled text.

    The algorithm:
    1. Parse all section blocks (``<!-- section:tag -->`` markers).
    2. Filter to only the tags in *include_tags*.
    3. Concatenate the filtered sections in document order.
    4. Split the concatenated text by ``## Phase N:`` headings.
    5. Map each heading's phase id to its content (heading + body until next heading).
    """
    sections = _parse_worker_sections(worker_md_content)
    included = [s["content"] for s in sections if s["tag"] in include_tags]
    full_text = "\n\n".join(included)

    # Split by phase headings: ## Phase <id>: <title>
    # Captures phase id (e.g., "1", "2a", "2.5", "5b", "0")
    phase_pattern = re.compile(r"(?=^## Phase (\d+[a-z]?(?:\.\d+)?):)", re.MULTILINE)

    parts = phase_pattern.split(full_text)
    # parts alternates: [pre-phase-text, phase_id, phase_body, phase_id, phase_body, ...]
    # The first element is text before any ## Phase heading (preamble).

    phases: dict[str, str] = {}
    # Start at index 1 (skip preamble), step by 2 (id, body pairs)
    i = 1
    while i < len(parts) - 1:
        phase_id = parts[i]
        body = parts[i + 1].strip()
        phases[phase_id] = body
        i += 2

    return phases


def _estimate_tokens(text: str) -> int:
    """Rough token estimate: words * 1.3 for English/Markdown.

    This approximates typical BPE tokenization where most English words
    map to 1-2 tokens, plus punctuation and formatting tokens.
    """
    words = len(text.split())
    return max(1, int(words * 1.3))


def _build_bootstrap_prompt(
    task_id: str,
    epic_id: str,
    flowctl_path: str,
    *,
    no_team: bool = False,
    tdd: bool = False,
    review: str | None = None,
    ralph: bool = False,
) -> str:
    """Build a minimal ~200 token bootstrap prompt for phase-gate execution.

    Instead of the full worker.md, this prompt instructs the worker to call
    ``worker-phase next`` in a loop, executing one phase at a time.
    Teams mode is the default; pass ``no_team=True`` for worktree isolation.
    """
    # Build mode flags string for worker-phase calls
    flags = []
    if no_team:
        flags.append("--no-team")
    if tdd:
        flags.append("--tdd")
    if review:
        flags.append(f"--review {review}")
    mode_flags = " ".join(flags)
    mode_flags_str = f" {mode_flags}" if mode_flags else ""

    lines = [
        "# Task Implementation Worker (Phase-Gate Mode)",
        "",
        "You implement a single flow-code task by executing phases sequentially.",
        "",
        "## Configuration",
        f"- TASK_ID: {task_id}",
        f"- EPIC_ID: {epic_id}",
        f"- FLOWCTL: {flowctl_path}",
        f"- REVIEW_MODE: {review or 'none'}",
        f"- RALPH_MODE: {str(ralph).lower()}",
        f"- TDD_MODE: {str(tdd).lower()}",
    ]
    if not no_team:
        lines.append("- TEAM_MODE: true")

    lines.extend([
        "",
        "## Execution Loop",
        "",
        "Repeat until all_done is true:",
        "",
        "```bash",
        f"$FLOWCTL worker-phase next --task $TASK_ID{mode_flags_str} --json",
        "```",
        "",
        "1. Read the returned `content` field — it contains full phase instructions.",
        "2. Execute the phase as described.",
        "3. When done, mark it complete:",
        "",
        "```bash",
        f"$FLOWCTL worker-phase done --task $TASK_ID --phase <PHASE_ID>{mode_flags_str} --json",
        "```",
        "",
        "4. Call `worker-phase next` again for the next phase.",
        "5. Stop when `all_done` is `true`.",
    ])

    return "\n".join(lines)


def cmd_worker_prompt(args: argparse.Namespace) -> None:
    """Output a trimmed worker prompt based on mode flags."""
    from flowctl.core.config import get_config

    # Handle --bootstrap mode: minimal ~200 token prompt
    if getattr(args, "bootstrap", False):
        task_id = args.task
        epic_id = epic_id_from_task(task_id) if is_task_id(task_id) else task_id

        plugin_root = _get_plugin_root()
        flowctl_path = str(plugin_root / "scripts" / "flowctl.py")

        prompt_text = _build_bootstrap_prompt(
            task_id=task_id,
            epic_id=epic_id,
            flowctl_path=flowctl_path,
            no_team=getattr(args, "no_team", False),
            tdd=args.tdd,
            review=args.review,
            ralph=getattr(args, "ralph", False),
        )
        estimated_tokens = _estimate_tokens(prompt_text)

        if args.json:
            json_output({
                "prompt": prompt_text,
                "mode": "bootstrap",
                "estimated_tokens": estimated_tokens,
            })
        else:
            print(prompt_text)
        return

    worker_path = _get_plugin_root() / "agents" / "worker.md"
    if not worker_path.exists():
        error_exit(
            f"agents/worker.md not found at {worker_path}", use_json=args.json
        )

    raw = worker_path.read_text(encoding="utf-8")

    # Strip YAML frontmatter (between --- markers)
    frontmatter_match = re.match(r"^---\n.*?\n---\n", raw, re.DOTALL)
    if frontmatter_match:
        raw = raw[frontmatter_match.end():]

    sections = _parse_worker_sections(raw)
    if not sections:
        error_exit(
            "No section markers found in worker.md. Expected <!-- section:tag --> markers.",
            use_json=args.json,
        )

    # Determine which tags to include
    # Teams mode is the default — always include "team" unless --no-team
    include_tags = {"core", "team"}

    if getattr(args, "no_team", False):
        include_tags.discard("team")

    if args.tdd:
        include_tags.add("tdd")

    if args.review:
        include_tags.add("review")

    # Auto-include memory if config says it's enabled
    memory_enabled = get_config("memory.enabled", False)
    if memory_enabled:
        include_tags.add("memory")

    # Filter sections
    included = [s for s in sections if s["tag"] in include_tags]
    included_tags = sorted({s["tag"] for s in included})

    # Assemble output
    prompt_parts = [s["content"] for s in included]
    prompt_text = "\n\n".join(prompt_parts)
    estimated_tokens = _estimate_tokens(prompt_text)

    if args.json:
        json_output({
            "prompt": prompt_text,
            "mode": "full",
            "sections": included_tags,
            "estimated_tokens": estimated_tokens,
        })
    else:
        print(prompt_text)


# ---------------------------------------------------------------------------
# worker-phase: phase-gate sequential execution
# ---------------------------------------------------------------------------

def _build_phase_sequence(tdd: bool = False, review: bool = False) -> list[str]:
    """Build the phase sequence based on mode flags.

    Teams mode is the default — Phase 0 is always included in PHASE_SEQ_DEFAULT.
    Modes combine additively: --tdd --review adds their phases to the default.
    """
    if not tdd and not review:
        return list(PHASE_SEQ_DEFAULT)

    # Start from the full canonical order for merging
    canonical = ["0", "1", "2a", "2", "2.5", "3", "4", "5", "5b", "6"]

    # Collect all phases from applicable sequences
    phases: set[str] = set(PHASE_SEQ_DEFAULT)
    if tdd:
        phases |= set(PHASE_SEQ_TDD)
    if review:
        phases |= set(PHASE_SEQ_REVIEW)

    # Return in canonical order
    return [p for p in canonical if p in phases]


def _load_phase_progress(task_id: str) -> dict:
    """Load phase_progress from runtime state, initializing if absent."""
    store = get_state_store()
    runtime = store.load_runtime(task_id)
    if runtime and "phase_progress" in runtime:
        return runtime["phase_progress"]
    return {"current_phase": None, "completed_phases": [], "started_at": None}


def _save_phase_progress(task_id: str, progress: dict) -> None:
    """Persist phase_progress to runtime state."""
    save_task_runtime(task_id, {"phase_progress": progress})


def cmd_worker_phase_next(args: argparse.Namespace) -> None:
    """Return the next uncompleted phase for a task."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    task_id = args.task
    if not is_task_id(task_id):
        error_exit(
            f"Invalid task ID: {task_id}. Expected format: fn-N.M or fn-N-slug.M",
            use_json=args.json,
        )

    # Build sequence from flags
    seq = _build_phase_sequence(
        tdd=args.tdd,
        review=args.review is not None,
    )

    # Load current progress
    progress = _load_phase_progress(task_id)
    completed = set(progress.get("completed_phases", []))

    # Find first uncompleted phase
    next_phase = None
    for phase_id in seq:
        if phase_id not in completed:
            next_phase = phase_id
            break

    if next_phase is None:
        # All phases done
        if args.json:
            json_output({"phase": None, "all_done": True, "sequence": seq})
        else:
            print("All phases completed.")
        return

    # Initialize progress if this is the first call
    if progress["started_at"] is None:
        progress["started_at"] = now_iso()
        progress["current_phase"] = next_phase
        _save_phase_progress(task_id, progress)

    phase_def = PHASE_DEFS.get(next_phase, (next_phase, f"Phase {next_phase}", ""))
    phase_id, title, done_condition = phase_def

    # Extract phase content from worker.md
    from flowctl.core.config import get_config
    worker_path = _get_plugin_root() / "agents" / "worker.md"
    phase_content = ""
    if worker_path.exists():
        raw = worker_path.read_text(encoding="utf-8")
        # Strip YAML frontmatter
        frontmatter_match = re.match(r"^---\n.*?\n---\n", raw, re.DOTALL)
        if frontmatter_match:
            raw = raw[frontmatter_match.end():]

        # Build include tags — Teams mode is the default (always include "team")
        include_tags = {"core", "team"}
        if getattr(args, "no_team", False):
            include_tags.discard("team")
        if args.tdd:
            include_tags.add("tdd")
        if args.review is not None:
            include_tags.add("review")
        memory_enabled = get_config("memory.enabled", False)
        if memory_enabled:
            include_tags.add("memory")

        phase_map = _extract_phase_content(raw, include_tags)
        phase_content = phase_map.get(next_phase, "")

    if args.json:
        result_data = {
            "phase": phase_id,
            "title": title,
            "done_condition": done_condition,
            "content": phase_content,
            "completed_phases": sorted(completed, key=lambda x: seq.index(x) if x in seq else 999),
            "sequence": seq,
            "all_done": False,
        }
        json_output(result_data)
    else:
        print(f"Next phase: {phase_id} - {title}")
        print(f"Done when: {done_condition}")
        if completed:
            print(f"Completed: {', '.join(sorted(completed))}")


def cmd_worker_phase_done(args: argparse.Namespace) -> None:
    """Mark a phase as done and advance to the next."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    task_id = args.task
    if not is_task_id(task_id):
        error_exit(
            f"Invalid task ID: {task_id}. Expected format: fn-N.M or fn-N-slug.M",
            use_json=args.json,
        )

    phase = args.phase

    # Build sequence from flags
    seq = _build_phase_sequence(
        tdd=args.tdd,
        review=args.review is not None,
    )

    # Validate phase exists in sequence
    if phase not in seq:
        error_exit(
            f"Phase '{phase}' is not in the current sequence: {', '.join(seq)}. "
            f"Check your mode flags (--tdd, --review).",
            use_json=args.json,
        )

    # Load current progress
    progress = _load_phase_progress(task_id)
    completed = progress.get("completed_phases", [])
    completed_set = set(completed)

    # Find expected next phase (first uncompleted)
    expected = None
    for pid in seq:
        if pid not in completed_set:
            expected = pid
            break

    if expected is None:
        error_exit(
            "All phases are already completed. Nothing to mark done.",
            use_json=args.json,
        )

    # Validate sequential execution — cannot skip phases
    if phase != expected:
        error_exit(
            f"Expected phase {expected}, got phase {phase}. Cannot skip phases.",
            use_json=args.json,
        )

    # Mark phase as completed
    completed.append(phase)
    completed_set.add(phase)

    # Advance current_phase to next uncompleted
    next_phase = None
    for pid in seq:
        if pid not in completed_set:
            next_phase = pid
            break

    progress["completed_phases"] = completed
    progress["current_phase"] = next_phase
    _save_phase_progress(task_id, progress)

    all_done = next_phase is None

    if args.json:
        result: dict[str, Any] = {
            "completed_phase": phase,
            "completed_phases": completed,
            "all_done": all_done,
        }
        if next_phase:
            phase_def = PHASE_DEFS.get(next_phase, (next_phase, f"Phase {next_phase}", ""))
            result["next_phase"] = {
                "phase": phase_def[0],
                "title": phase_def[1],
                "done_condition": phase_def[2],
            }
        json_output(result)
    else:
        print(f"Phase {phase} marked done.")
        if next_phase:
            phase_def = PHASE_DEFS.get(next_phase, (next_phase, f"Phase {next_phase}", ""))
            print(f"Next: {phase_def[0]} - {phase_def[1]}")
        else:
            print("All phases completed.")
