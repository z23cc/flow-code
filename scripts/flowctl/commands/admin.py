"""Admin commands: init, detect, status, ralph control, config, review-backend, validate, doctor."""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from datetime import datetime
from pathlib import Path
from typing import Optional

from flowctl.core.constants import (
    CONFIG_FILE,
    EPICS_DIR,
    MEMORY_DIR,
    META_FILE,
    REVIEWS_DIR,
    SCHEMA_VERSION,
    SPECS_DIR,
    SUPPORTED_SCHEMA_VERSIONS,
    TASK_SPEC_HEADINGS,
    TASK_STATUS,
    TASKS_DIR,
)
from flowctl.core.config import (
    deep_merge,
    get_config,
    get_default_config,
    load_flow_config,
    set_config,
)
from flowctl.core.ids import is_epic_id, is_task_id, normalize_epic
from flowctl.core.io import (
    atomic_write_json,
    error_exit,
    is_supported_schema,
    json_output,
    load_json,
    load_json_or_exit,
)
from flowctl.core.paths import ensure_flow_exists, get_flow_dir, get_repo_root, get_state_dir
from flowctl.core.state import get_state_store, load_task_with_state
from flowctl.commands.stack import detect_stack


# --- Ralph Run Detection ---


def find_active_runs() -> list[dict]:
    """Find active Ralph runs by scanning scripts/ralph/runs/*/progress.txt.

    A run is active if progress.txt exists AND does NOT contain 'promise=COMPLETE'.
    Returns list of dicts with run info.
    """
    repo_root = get_repo_root()
    runs_dir = repo_root / "scripts" / "ralph" / "runs"
    active_runs = []

    if not runs_dir.exists():
        return active_runs

    for run_dir in runs_dir.iterdir():
        if not run_dir.is_dir():
            continue
        progress_file = run_dir / "progress.txt"
        if not progress_file.exists():
            continue

        content = progress_file.read_text(encoding="utf-8", errors="replace")

        # Run is complete if it contains the completion marker block
        # Require both completion_reason= AND promise=COMPLETE to avoid
        # false positives from per-iteration promise= logging
        if "completion_reason=" in content and "promise=COMPLETE" in content:
            continue

        # Parse progress info from content
        run_info = {
            "id": run_dir.name,
            "path": str(run_dir),
            "iteration": None,
            "current_epic": None,
            "current_task": None,
            "paused": (run_dir / "PAUSE").exists(),
            "stopped": (run_dir / "STOP").exists(),
        }

        # Extract iteration number (format: "iteration: N" or "Iteration N")
        iter_match = re.search(r"iteration[:\s]+(\d+)", content, re.IGNORECASE)
        if iter_match:
            run_info["iteration"] = int(iter_match.group(1))

        # Extract current epic/task (format varies, try common patterns)
        epic_match = re.search(r"epic[:\s]+(fn-[\w-]+)", content, re.IGNORECASE)
        if epic_match:
            run_info["current_epic"] = epic_match.group(1)

        task_match = re.search(
            r"task[:\s]+(fn-[\w.-]+\.\d+)", content, re.IGNORECASE
        )
        if task_match:
            run_info["current_task"] = task_match.group(1)

        active_runs.append(run_info)

    return active_runs


def find_active_run(
    run_id: Optional[str] = None, use_json: bool = False
) -> tuple[str, Path]:
    """Find a single active run. Auto-detect if run_id is None.

    Returns (run_id, run_dir) tuple.
    """
    runs = find_active_runs()
    if run_id:
        matches = [r for r in runs if r["id"] == run_id]
        if not matches:
            error_exit(f"Run {run_id} not found or not active", use_json=use_json)
        return matches[0]["id"], Path(matches[0]["path"])
    if len(runs) == 0:
        error_exit("No active runs", use_json=use_json)
    if len(runs) > 1:
        ids = ", ".join(r["id"] for r in runs)
        error_exit(f"Multiple active runs, specify --run: {ids}", use_json=use_json)
    return runs[0]["id"], Path(runs[0]["path"])


# --- Validation helpers ---


def validate_task_spec_headings(content: str) -> list[str]:
    """Validate task spec has required headings exactly once. Returns errors."""
    errors = []
    for heading in TASK_SPEC_HEADINGS:
        # Use regex anchored to line start to avoid matching inside code blocks
        pattern = rf"^{re.escape(heading)}\s*$"
        count = len(re.findall(pattern, content, flags=re.MULTILINE))
        if count == 0:
            errors.append(f"Missing required heading: {heading}")
        elif count > 1:
            errors.append(f"Duplicate heading: {heading} (found {count} times)")
    return errors


def validate_flow_root(flow_dir: Path) -> list[str]:
    """Validate .flow/ root invariants. Returns list of errors."""
    errors = []

    # Check meta.json exists and is valid
    meta_path = flow_dir / META_FILE
    if not meta_path.exists():
        errors.append(f"meta.json missing: {meta_path}")
    else:
        try:
            meta = load_json(meta_path)
            if not is_supported_schema(meta.get("schema_version")):
                errors.append(
                    "schema_version unsupported in meta.json "
                    f"(expected {', '.join(map(str, SUPPORTED_SCHEMA_VERSIONS))}, "
                    f"got {meta.get('schema_version')})"
                )
        except json.JSONDecodeError as e:
            errors.append(f"meta.json invalid JSON: {e}")
        except Exception as e:
            errors.append(f"meta.json unreadable: {e}")

    # Check required subdirectories exist
    for subdir in [EPICS_DIR, SPECS_DIR, TASKS_DIR, MEMORY_DIR, REVIEWS_DIR]:
        if not (flow_dir / subdir).exists():
            errors.append(f"Required directory missing: {subdir}/")

    return errors


def validate_epic(
    flow_dir: Path, epic_id: str, use_json: bool = True
) -> tuple[list[str], list[str], int]:
    """Validate a single epic. Returns (errors, warnings, task_count)."""
    errors = []
    warnings = []

    epic_path = flow_dir / EPICS_DIR / f"{epic_id}.json"

    if not epic_path.exists():
        errors.append(f"Epic {epic_id} not found")
        return errors, warnings, 0

    epic_data = normalize_epic(
        load_json_or_exit(epic_path, f"Epic {epic_id}", use_json=use_json)
    )

    # Check epic spec exists
    epic_spec = flow_dir / SPECS_DIR / f"{epic_id}.md"
    if not epic_spec.exists():
        errors.append(f"Epic spec missing: {epic_spec}")

    # Validate epic dependencies
    deps = epic_data.get("depends_on_epics", [])
    if deps is None:
        deps = []
    if not isinstance(deps, list):
        errors.append(f"Epic {epic_id}: depends_on_epics must be a list")
    else:
        for dep in deps:
            if not isinstance(dep, str) or not is_epic_id(dep):
                errors.append(
                    f"Epic {epic_id}: invalid depends_on_epics entry '{dep}'"
                )
                continue
            if dep == epic_id:
                errors.append(
                    f"Epic {epic_id}: depends_on_epics cannot include itself"
                )
                continue
            dep_path = flow_dir / EPICS_DIR / f"{dep}.json"
            if not dep_path.exists():
                errors.append(
                    f"Epic {epic_id}: depends_on_epics missing epic {dep}"
                )

    # Get all tasks (with merged runtime state for accurate status)
    tasks_dir = flow_dir / TASKS_DIR
    tasks = {}
    if tasks_dir.exists():
        for task_file in tasks_dir.glob(f"{epic_id}.*.json"):
            task_id = task_file.stem
            if not is_task_id(task_id):
                continue  # Skip non-task files (e.g., fn-1.2-review.json)
            # Use merged state to get accurate status
            task_data = load_task_with_state(task_id, use_json=use_json)
            if "id" not in task_data:
                continue  # Skip artifact files (GH-21)
            tasks[task_data["id"]] = task_data

    # Validate each task
    for task_id, task in tasks.items():
        # Validate status (use merged state which defaults to "todo" if missing)
        status = task.get("status", "todo")
        if status not in TASK_STATUS:
            errors.append(f"Task {task_id}: invalid status '{status}'")

        # Check task spec exists
        task_spec_path = flow_dir / TASKS_DIR / f"{task_id}.md"
        if not task_spec_path.exists():
            errors.append(f"Task spec missing: {task_spec_path}")
        else:
            # Validate task spec headings
            try:
                spec_content = task_spec_path.read_text(encoding="utf-8")
            except Exception as e:
                errors.append(f"Task {task_id}: spec unreadable ({e})")
                continue
            heading_errors = validate_task_spec_headings(spec_content)
            for he in heading_errors:
                errors.append(f"Task {task_id}: {he}")

        # Check dependencies exist and are within epic
        for dep in task["depends_on"]:
            if dep not in tasks:
                errors.append(f"Task {task_id}: dependency {dep} not found")
            if not dep.startswith(epic_id + "."):
                errors.append(
                    f"Task {task_id}: dependency {dep} is outside epic {epic_id}"
                )

    # Validate gaps array
    gaps = epic_data.get("gaps", [])
    if not isinstance(gaps, list):
        errors.append(f"Epic {epic_id}: gaps must be a list")
    else:
        gap_ids = set()
        for i, gap in enumerate(gaps):
            if not isinstance(gap, dict):
                errors.append(f"Epic {epic_id}: gaps[{i}] must be an object")
                continue
            if "id" not in gap or not isinstance(gap.get("id"), str):
                errors.append(f"Epic {epic_id}: gaps[{i}] missing or invalid 'id'")
            elif gap["id"] in gap_ids:
                errors.append(f"Epic {epic_id}: duplicate gap id '{gap['id']}'")
            else:
                gap_ids.add(gap["id"])
            if "capability" not in gap or not isinstance(
                gap.get("capability"), str
            ):
                errors.append(
                    f"Epic {epic_id}: gaps[{i}] missing or invalid 'capability'"
                )
            if gap.get("status") not in ("open", "resolved"):
                errors.append(
                    f"Epic {epic_id}: gaps[{i}] invalid status '{gap.get('status')}'"
                )
            if gap.get("priority") not in ("required", "important", "nice-to-have"):
                errors.append(
                    f"Epic {epic_id}: gaps[{i}] invalid priority '{gap.get('priority')}'"
                )

    # Cycle detection using DFS
    def has_cycle(tid: str, visited: set, rec_stack: set) -> list[str]:
        visited.add(tid)
        rec_stack.add(tid)

        for dep in tasks.get(tid, {}).get("depends_on", []):
            if dep not in visited:
                cycle = has_cycle(dep, visited, rec_stack)
                if cycle:
                    return [tid] + cycle
            elif dep in rec_stack:
                return [tid, dep]

        rec_stack.remove(tid)
        return []

    visited = set()
    for tid in tasks:
        if tid not in visited:
            cycle = has_cycle(tid, visited, set())
            if cycle:
                errors.append(
                    f"Dependency cycle detected: {' -> '.join(cycle)}"
                )
                break

    # Check epic done status consistency
    if epic_data["status"] == "done":
        for tid, task in tasks.items():
            if task["status"] != "done":
                errors.append(
                    f"Epic marked done but task {tid} is {task['status']}"
                )

    return errors, warnings, len(tasks)


# --- Init / Detect / Status commands ---


def cmd_init(args: argparse.Namespace) -> None:
    """Initialize or upgrade .flow/ directory structure (idempotent)."""
    flow_dir = get_flow_dir()
    actions = []

    # Create directories if missing (idempotent, never destroys existing)
    for subdir in [EPICS_DIR, SPECS_DIR, TASKS_DIR, MEMORY_DIR, REVIEWS_DIR]:
        dir_path = flow_dir / subdir
        if not dir_path.exists():
            dir_path.mkdir(parents=True)
            actions.append(f"created {subdir}/")

    # Create meta.json if missing (never overwrite existing)
    meta_path = flow_dir / META_FILE
    if not meta_path.exists():
        meta = {"schema_version": SCHEMA_VERSION, "next_epic": 1}
        atomic_write_json(meta_path, meta)
        actions.append("created meta.json")

    # Config: create or upgrade (merge missing defaults)
    config_path = flow_dir / CONFIG_FILE
    if not config_path.exists():
        atomic_write_json(config_path, get_default_config())
        actions.append("created config.json")
    else:
        # Load raw config, compare with merged (which includes new defaults)
        try:
            raw = json.loads(config_path.read_text(encoding="utf-8"))
            if not isinstance(raw, dict):
                raw = {}
        except (json.JSONDecodeError, Exception):
            raw = {}
        merged = deep_merge(get_default_config(), raw)
        if merged != raw:
            atomic_write_json(config_path, merged)
            actions.append("upgraded config.json (added missing keys)")

    # Auto-detect stack if not already configured
    existing_stack = get_config("stack", {})
    if not existing_stack:
        detected = detect_stack()
        if detected:
            set_config("stack", detected)
            actions.append("auto-detected stack")

    # Output
    if actions:
        message = f".flow/ updated: {', '.join(actions)}"
    else:
        message = ".flow/ already up to date"

    stack_info = get_config("stack", {})
    if args.json:
        result = {
            "success": True,
            "message": message,
            "path": str(flow_dir),
            "actions": actions,
        }
        if stack_info:
            result["stack"] = stack_info
        json_output(result)
    else:
        print(message)
        if stack_info and "auto-detected stack" in actions:
            print(
                "Stack: " + json.dumps(stack_info, indent=2, ensure_ascii=False)
            )


def cmd_detect(args: argparse.Namespace) -> None:
    """Check if .flow/ exists and is valid."""
    flow_dir = get_flow_dir()
    exists = flow_dir.exists()
    valid = False
    issues = []

    if exists:
        meta_path = flow_dir / META_FILE
        if not meta_path.exists():
            issues.append("meta.json missing")
        else:
            try:
                meta = load_json(meta_path)
                if not is_supported_schema(meta.get("schema_version")):
                    issues.append(
                        f"schema_version unsupported "
                        f"(expected {', '.join(map(str, SUPPORTED_SCHEMA_VERSIONS))})"
                    )
            except Exception as e:
                issues.append(f"meta.json parse error: {e}")

        # Check required subdirectories
        for subdir in [EPICS_DIR, SPECS_DIR, TASKS_DIR, MEMORY_DIR, REVIEWS_DIR]:
            if not (flow_dir / subdir).exists():
                issues.append(f"{subdir}/ missing")

        valid = len(issues) == 0

    if args.json:
        result = {
            "exists": exists,
            "valid": valid,
            "path": str(flow_dir) if exists else None,
        }
        if issues:
            result["issues"] = issues
        json_output(result)
    else:
        if exists and valid:
            print(f".flow/ exists and is valid at {flow_dir}")
        elif exists:
            print(f".flow/ exists but has issues at {flow_dir}:")
            for issue in issues:
                print(f"  - {issue}")
        else:
            print(".flow/ does not exist")


def cmd_status(args: argparse.Namespace) -> None:
    """Show .flow state and active Ralph runs."""
    flow_dir = get_flow_dir()
    flow_exists = flow_dir.exists()

    # Count epics and tasks by status
    epic_counts = {"open": 0, "done": 0}
    task_counts = {"todo": 0, "in_progress": 0, "blocked": 0, "done": 0}

    if flow_exists:
        epics_dir = flow_dir / EPICS_DIR
        tasks_dir = flow_dir / TASKS_DIR

        if epics_dir.exists():
            for epic_file in epics_dir.glob("fn-*.json"):
                try:
                    epic_data = load_json(epic_file)
                    status = epic_data.get("status", "open")
                    if status in epic_counts:
                        epic_counts[status] += 1
                except Exception:
                    pass

        if tasks_dir.exists():
            for task_file in tasks_dir.glob("fn-*.json"):
                task_id = task_file.stem
                if not is_task_id(task_id):
                    continue  # Skip non-task files
                try:
                    # Use merged state for accurate status counts
                    task_data = load_task_with_state(task_id, use_json=True)
                    status = task_data.get("status", "todo")
                    if status in task_counts:
                        task_counts[status] += 1
                except Exception:
                    pass

    # Get active runs
    active_runs = find_active_runs()

    if args.json:
        json_output(
            {
                "success": True,
                "flow_exists": flow_exists,
                "epics": epic_counts,
                "tasks": task_counts,
                "runs": [
                    {
                        "id": r["id"],
                        "iteration": r["iteration"],
                        "current_epic": r["current_epic"],
                        "current_task": r["current_task"],
                        "paused": r["paused"],
                        "stopped": r["stopped"],
                    }
                    for r in active_runs
                ],
            }
        )
    else:
        if not flow_exists:
            print(".flow/ not initialized")
        else:
            print(f"Epics: {epic_counts['open']} open, {epic_counts['done']} done")
            print(
                f"Tasks: {task_counts['todo']} todo, "
                f"{task_counts['in_progress']} in_progress, "
                f"{task_counts['done']} done, {task_counts['blocked']} blocked"
            )

        print()
        if active_runs:
            print("Active runs:")
            for r in active_runs:
                state = []
                if r["paused"]:
                    state.append("PAUSED")
                if r["stopped"]:
                    state.append("STOPPED")
                state_str = f" [{', '.join(state)}]" if state else ""
                task_info = ""
                if r["current_task"]:
                    task_info = f", working on {r['current_task']}"
                elif r["current_epic"]:
                    task_info = f", epic {r['current_epic']}"
                iter_info = (
                    f"iteration {r['iteration']}" if r["iteration"] else "starting"
                )
                print(f"  {r['id']} ({iter_info}{task_info}){state_str}")
        else:
            print("No active runs")


# --- Ralph control commands ---


def cmd_ralph_pause(args: argparse.Namespace) -> None:
    """Pause a Ralph run."""
    run_id, run_dir = find_active_run(args.run, use_json=args.json)
    pause_file = run_dir / "PAUSE"
    pause_file.touch()
    if args.json:
        json_output({"success": True, "run": run_id, "action": "paused"})
    else:
        print(f"Paused {run_id}")


def cmd_ralph_resume(args: argparse.Namespace) -> None:
    """Resume a paused Ralph run."""
    run_id, run_dir = find_active_run(args.run, use_json=args.json)
    pause_file = run_dir / "PAUSE"
    pause_file.unlink(missing_ok=True)
    if args.json:
        json_output({"success": True, "run": run_id, "action": "resumed"})
    else:
        print(f"Resumed {run_id}")


def cmd_ralph_stop(args: argparse.Namespace) -> None:
    """Request a Ralph run to stop."""
    run_id, run_dir = find_active_run(args.run, use_json=args.json)
    stop_file = run_dir / "STOP"
    stop_file.touch()
    if args.json:
        json_output({"success": True, "run": run_id, "action": "stop_requested"})
    else:
        print(f"Stop requested for {run_id}")


def cmd_ralph_status(args: argparse.Namespace) -> None:
    """Show Ralph run status."""
    run_id, run_dir = find_active_run(args.run, use_json=args.json)
    paused = (run_dir / "PAUSE").exists()
    stopped = (run_dir / "STOP").exists()

    # Read progress.txt for more info
    progress_file = run_dir / "progress.txt"
    iteration = None
    current_epic = None
    current_task = None

    if progress_file.exists():
        content = progress_file.read_text(encoding="utf-8", errors="replace")
        iter_match = re.search(r"iteration[:\s]+(\d+)", content, re.IGNORECASE)
        if iter_match:
            iteration = int(iter_match.group(1))
        epic_match = re.search(r"epic[:\s]+(fn-[\w-]+)", content, re.IGNORECASE)
        if epic_match:
            current_epic = epic_match.group(1)
        task_match = re.search(
            r"task[:\s]+(fn-[\w.-]+\.\d+)", content, re.IGNORECASE
        )
        if task_match:
            current_task = task_match.group(1)

    if args.json:
        json_output(
            {
                "success": True,
                "run": run_id,
                "iteration": iteration,
                "current_epic": current_epic,
                "current_task": current_task,
                "paused": paused,
                "stopped": stopped,
            }
        )
    else:
        state = []
        if paused:
            state.append("PAUSED")
        if stopped:
            state.append("STOPPED")
        state_str = f" [{', '.join(state)}]" if state else " [running]"
        task_info = ""
        if current_task:
            task_info = f", working on {current_task}"
        elif current_epic:
            task_info = f", epic {current_epic}"
        iter_info = f"iteration {iteration}" if iteration else "starting"
        print(f"{run_id} ({iter_info}{task_info}){state_str}")


# --- Config commands ---


def cmd_config_get(args: argparse.Namespace) -> None:
    """Get a config value."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    value = get_config(args.key)
    if args.json:
        json_output({"key": args.key, "value": value})
    else:
        if value is None:
            print(f"{args.key}: (not set)")
        elif isinstance(value, bool):
            print(f"{args.key}: {'true' if value else 'false'}")
        else:
            print(f"{args.key}: {value}")


def cmd_config_set(args: argparse.Namespace) -> None:
    """Set a config value."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    set_config(args.key, args.value)
    new_value = get_config(args.key)

    if args.json:
        json_output(
            {"key": args.key, "value": new_value, "message": f"{args.key} set"}
        )
    else:
        print(f"{args.key} set to {new_value}")


# --- Review backend command ---


def cmd_review_backend(args: argparse.Namespace) -> None:
    """Get review backend for skill conditionals. Returns ASK if not configured."""
    # Priority: FLOW_REVIEW_BACKEND env > config > ASK
    env_val = os.environ.get("FLOW_REVIEW_BACKEND", "").strip()
    if env_val and env_val in ("rp", "codex", "none"):
        backend = env_val
        source = "env"
    elif ensure_flow_exists():
        cfg_val = get_config("review.backend")
        if cfg_val and cfg_val in ("rp", "codex", "none"):
            backend = cfg_val
            source = "config"
        else:
            backend = "ASK"
            source = "none"
    else:
        backend = "ASK"
        source = "none"

    # --compare mode: compare multiple review receipt files
    # Resolve receipt files: --compare takes paths, --epic scans .flow/reviews/
    compare_arg = getattr(args, "compare", None)
    epic_arg = getattr(args, "epic", None)
    if epic_arg and not compare_arg:
        # Auto-discover receipts for this epic
        if not ensure_flow_exists():
            error_exit(".flow/ does not exist.", use_json=args.json)
        reviews_dir = get_flow_dir() / REVIEWS_DIR
        if not reviews_dir.exists():
            error_exit(
                "No reviews directory found. Complete tasks with review_receipt "
                "in evidence first.",
                use_json=args.json,
            )
        receipt_files = sorted(
            str(f) for f in reviews_dir.glob(f"*-{epic_arg}.*-*.json")
        )
        if not receipt_files:
            error_exit(
                f"No review receipts found for epic {epic_arg}",
                use_json=args.json,
            )
    elif compare_arg:
        receipt_files = [f.strip() for f in compare_arg.split(",")]
    else:
        receipt_files = None

    if receipt_files:
        reviews = []
        for rf in receipt_files:
            rpath = Path(rf)
            if not rpath.exists():
                error_exit(f"Receipt file not found: {rf}", use_json=args.json)
            try:
                rdata = json.loads(rpath.read_text(encoding="utf-8"))
            except (json.JSONDecodeError, Exception) as e:
                error_exit(
                    f"Invalid receipt JSON: {rf}: {e}", use_json=args.json
                )
            reviews.append(
                {
                    "file": rf,
                    "mode": rdata.get("mode", "unknown"),
                    "verdict": rdata.get("verdict", "unknown"),
                    "id": rdata.get("id", "unknown"),
                    "timestamp": rdata.get("timestamp", ""),
                    "review": rdata.get("review", ""),
                }
            )

        # Analyze: agreements, conflicts, verdicts
        verdicts = {r["mode"]: r["verdict"] for r in reviews}
        all_same = len(set(verdicts.values())) <= 1
        consensus_verdict = (
            list(verdicts.values())[0] if all_same and verdicts else None
        )

        result = {
            "reviews": len(reviews),
            "verdicts": verdicts,
            "consensus": consensus_verdict,
            "has_conflict": not all_same,
            "details": reviews,
        }

        if args.json:
            json_output(result)
        else:
            print(f"Review Comparison ({len(reviews)} reviews):\n")
            for r in reviews:
                print(
                    f"  [{r['mode']}] verdict: {r['verdict']}  ({r['file']})"
                )
            print()
            if all_same:
                print(f"Consensus: {consensus_verdict}")
            else:
                print("CONFLICT — reviewers disagree:")
                for mode, verdict in verdicts.items():
                    print(f"  {mode}: {verdict}")
        return

    if args.json:
        json_output({"backend": backend, "source": source})
    else:
        print(backend)


# --- Validate command ---


def cmd_validate(args: argparse.Namespace) -> None:
    """Validate epic structure or all epics."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    # Require either --epic or --all
    if not args.epic and not getattr(args, "all", False):
        error_exit("Must specify --epic or --all", use_json=args.json)

    flow_dir = get_flow_dir()

    # MU-3: Validate all mode
    if getattr(args, "all", False):
        # First validate .flow/ root invariants
        root_errors = validate_flow_root(flow_dir)

        epics_dir = flow_dir / EPICS_DIR

        # Find all epics (if epics dir exists)
        epic_ids = []
        epic_nums: dict[int, list[str]] = {}
        if epics_dir.exists():
            for epic_file in sorted(epics_dir.glob("fn-*.json")):
                match = re.match(
                    r"^fn-(\d+)(?:-[a-z0-9][a-z0-9-]*[a-z0-9]|-[a-z0-9]{1,3})?\.json$",
                    epic_file.name,
                )
                if match:
                    epic_id = epic_file.stem
                    epic_ids.append(epic_id)
                    num = int(match.group(1))
                    if num not in epic_nums:
                        epic_nums[num] = []
                    epic_nums[num].append(epic_id)

        # Start with root errors
        all_errors = list(root_errors)

        # Detect epic ID collisions
        for num, ids in epic_nums.items():
            if len(ids) > 1:
                all_errors.append(
                    f"Epic ID collision: fn-{num} used by multiple epics: "
                    f"{', '.join(sorted(ids))}"
                )

        all_warnings = []

        # Detect orphaned specs
        specs_dir = flow_dir / SPECS_DIR
        if specs_dir.exists():
            pattern = (
                r"^fn-(\d+)(?:-[a-z0-9][a-z0-9-]*[a-z0-9]|-[a-z0-9]{1,3})?\.md$"
            )
            for spec_file in specs_dir.glob("fn-*.md"):
                match = re.match(pattern, spec_file.name)
                if match:
                    spec_id = spec_file.stem
                    if spec_id not in epic_ids:
                        all_warnings.append(
                            f"Orphaned spec: {spec_file.name} has no matching epic JSON"
                        )
        total_tasks = 0
        epic_results = []

        for epic_id in epic_ids:
            errors, warnings, task_count = validate_epic(
                flow_dir, epic_id, use_json=args.json
            )
            all_errors.extend(errors)
            all_warnings.extend(warnings)
            total_tasks += task_count
            epic_results.append(
                {
                    "epic": epic_id,
                    "valid": len(errors) == 0,
                    "errors": errors,
                    "warnings": warnings,
                    "task_count": task_count,
                }
            )

        valid = len(all_errors) == 0

        if args.json:
            json_output(
                {
                    "valid": valid,
                    "root_errors": root_errors,
                    "epics": epic_results,
                    "total_epics": len(epic_ids),
                    "total_tasks": total_tasks,
                    "total_errors": len(all_errors),
                    "total_warnings": len(all_warnings),
                },
                success=valid,
            )
        else:
            print("Validation for all epics:")
            print(f"  Epics: {len(epic_ids)}")
            print(f"  Tasks: {total_tasks}")
            print(f"  Valid: {valid}")
            if all_errors:
                print("  Errors:")
                for e in all_errors:
                    print(f"    - {e}")
            if all_warnings:
                print("  Warnings:")
                for w in all_warnings:
                    print(f"    - {w}")

        # Exit with non-zero if validation failed
        if not valid:
            sys.exit(1)
        return

    # Single epic validation
    if not is_epic_id(args.epic):
        error_exit(
            f"Invalid epic ID: {args.epic}. Expected format: fn-N or fn-N-slug "
            f"(e.g., fn-1, fn-1-add-auth)",
            use_json=args.json,
        )

    errors, warnings, task_count = validate_epic(
        flow_dir, args.epic, use_json=args.json
    )
    valid = len(errors) == 0

    if args.json:
        json_output(
            {
                "epic": args.epic,
                "valid": valid,
                "errors": errors,
                "warnings": warnings,
                "task_count": task_count,
            },
            success=valid,
        )
    else:
        print(f"Validation for {args.epic}:")
        print(f"  Tasks: {task_count}")
        print(f"  Valid: {valid}")
        if errors:
            print("  Errors:")
            for e in errors:
                print(f"    - {e}")
        if warnings:
            print("  Warnings:")
            for w in warnings:
                print(f"    - {w}")

    # Exit with non-zero if validation failed
    if not valid:
        sys.exit(1)


# --- Doctor command ---


def cmd_doctor(args: argparse.Namespace) -> None:
    """Run comprehensive state health diagnostics (superset of validate --all)."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    flow_dir = get_flow_dir()
    checks: list[dict] = []

    def add_check(name: str, status: str, message: str) -> None:
        checks.append({"name": name, "status": status, "message": message})

    # --- Check 1: Run validate --all internally ---
    import io as _io
    import contextlib

    fake_args = argparse.Namespace(epic=None, all=True, json=True)
    validate_output = _io.StringIO()
    validate_passed = True
    try:
        with contextlib.redirect_stdout(validate_output):
            cmd_validate(fake_args)
    except SystemExit as e:
        if e.code != 0:
            validate_passed = False

    if validate_passed:
        add_check("validate", "pass", "All epics and tasks validated successfully")
    else:
        # Parse the validate output for details
        try:
            vdata = json.loads(validate_output.getvalue())
            err_count = vdata.get("total_errors", 0)
            add_check(
                "validate", "fail",
                f"Validation found {err_count} error(s). Run 'flowctl validate --all' for details"
            )
        except (json.JSONDecodeError, ValueError):
            add_check("validate", "fail", "Validation failed (could not parse output)")

    # --- Check 2: State-dir accessibility ---
    try:
        state_dir = get_state_dir()
        state_dir.mkdir(parents=True, exist_ok=True)
        # Test write access
        test_file = state_dir / ".doctor-probe"
        test_file.write_text("probe", encoding="utf-8")
        test_file.unlink()
        add_check("state_dir_access", "pass", f"State dir accessible: {state_dir}")
    except (OSError, PermissionError) as e:
        add_check("state_dir_access", "fail", f"State dir not accessible: {e}")

    # --- Check 3: Orphaned state files ---
    try:
        store = get_state_store()
        runtime_ids = store.list_runtime_files()
        tasks_dir = flow_dir / TASKS_DIR
        orphaned = []
        for rid in runtime_ids:
            task_def_path = tasks_dir / f"{rid}.json"
            if not task_def_path.exists():
                orphaned.append(rid)
        if orphaned:
            add_check(
                "orphaned_state", "warn",
                f"{len(orphaned)} orphaned state file(s): {', '.join(orphaned[:5])}"
                + (f" (+{len(orphaned) - 5} more)" if len(orphaned) > 5 else "")
            )
        else:
            add_check("orphaned_state", "pass", "No orphaned state files")
    except Exception as e:
        add_check("orphaned_state", "warn", f"Could not check orphaned state: {e}")

    # --- Check 4: Stale in_progress tasks (>7 days) ---
    try:
        stale = []
        tasks_dir = flow_dir / TASKS_DIR
        if tasks_dir.exists():
            for task_file in tasks_dir.glob("fn-*.json"):
                task_id = task_file.stem
                if not is_task_id(task_id):
                    continue
                try:
                    task_data = load_task_with_state(task_id, use_json=True)
                except SystemExit:
                    continue
                if task_data.get("status") != "in_progress":
                    continue
                updated = task_data.get("updated_at") or task_data.get("claimed_at")
                if updated:
                    try:
                        # Parse ISO timestamp
                        ts = updated.replace("Z", "+00:00")
                        task_time = datetime.fromisoformat(ts)
                        now = datetime.utcnow().replace(
                            tzinfo=task_time.tzinfo
                        )
                        age_days = (now - task_time).days
                        if age_days > 7:
                            stale.append(f"{task_id} ({age_days}d)")
                    except (ValueError, TypeError):
                        pass
        if stale:
            add_check(
                "stale_tasks", "warn",
                f"{len(stale)} task(s) in_progress for >7 days: {', '.join(stale[:5])}"
                + (f" (+{len(stale) - 5} more)" if len(stale) > 5 else "")
            )
        else:
            add_check("stale_tasks", "pass", "No stale in_progress tasks")
    except Exception as e:
        add_check("stale_tasks", "warn", f"Could not check stale tasks: {e}")

    # --- Check 5: Lock file accumulation ---
    try:
        state_dir = get_state_dir()
        locks_dir = state_dir / "locks"
        lock_count = 0
        if locks_dir.exists():
            lock_count = sum(1 for _ in locks_dir.glob("*.lock"))
        if lock_count > 50:
            add_check(
                "lock_files", "warn",
                f"{lock_count} lock files in state dir (consider cleanup)"
            )
        else:
            add_check(
                "lock_files", "pass",
                f"{lock_count} lock file(s) in state dir"
            )
    except Exception as e:
        add_check("lock_files", "warn", f"Could not check lock files: {e}")

    # --- Check 6: Config validity ---
    try:
        config_path = flow_dir / CONFIG_FILE
        if config_path.exists():
            raw_text = config_path.read_text(encoding="utf-8")
            parsed = json.loads(raw_text)
            if not isinstance(parsed, dict):
                add_check("config", "fail", "config.json is not a JSON object")
            else:
                # Check for known top-level keys
                known_keys = set(get_default_config().keys())
                unknown = set(parsed.keys()) - known_keys
                if unknown:
                    add_check(
                        "config", "warn",
                        f"Unknown config keys: {', '.join(sorted(unknown))}"
                    )
                else:
                    add_check("config", "pass", "config.json valid with known keys")
        else:
            add_check("config", "warn", "config.json missing (run 'flowctl init')")
    except json.JSONDecodeError as e:
        add_check("config", "fail", f"config.json invalid JSON: {e}")
    except Exception as e:
        add_check("config", "warn", f"Could not check config: {e}")

    # --- Check 7: git-common-dir reachability ---
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--git-common-dir", "--path-format=absolute"],
            capture_output=True, text=True, check=True,
        )
        common_dir = Path(result.stdout.strip())
        if common_dir.exists():
            add_check(
                "git_common_dir", "pass",
                f"git common-dir reachable: {common_dir}"
            )
        else:
            add_check(
                "git_common_dir", "warn",
                f"git common-dir path does not exist: {common_dir}"
            )
    except subprocess.CalledProcessError:
        add_check(
            "git_common_dir", "warn",
            "Not in a git repository (git common-dir unavailable)"
        )
    except FileNotFoundError:
        add_check(
            "git_common_dir", "warn",
            "git not found on PATH"
        )

    # --- Build summary ---
    summary = {"pass": 0, "warn": 0, "fail": 0}
    for c in checks:
        summary[c["status"]] += 1

    overall_healthy = summary["fail"] == 0

    if args.json:
        json_output(
            {
                "checks": checks,
                "summary": summary,
                "healthy": overall_healthy,
            },
            success=overall_healthy,
        )
    else:
        print("Doctor diagnostics:")
        for c in checks:
            icon = {"pass": "OK", "warn": "WARN", "fail": "FAIL"}[c["status"]]
            print(f"  [{icon}] {c['name']}: {c['message']}")
        print()
        print(
            f"Summary: {summary['pass']} pass, "
            f"{summary['warn']} warn, {summary['fail']} fail"
        )
        if not overall_healthy:
            print("Health check FAILED — resolve fail items above.")

    if not overall_healthy:
        sys.exit(1)
