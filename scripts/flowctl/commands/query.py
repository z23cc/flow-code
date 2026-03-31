"""Cross-cutting query commands: show, epics, files, tasks, list, cat."""

import argparse
import re
from pathlib import Path

from flowctl.core.constants import (
    EPICS_DIR,
    SPECS_DIR,
    TASKS_DIR,
)
from flowctl.core.ids import (
    is_epic_id,
    is_task_id,
    normalize_epic,
    parse_id,
)
from flowctl.core.io import (
    error_exit,
    json_output,
    load_json_or_exit,
    read_text_or_exit,
)
from flowctl.core.paths import ensure_flow_exists, get_flow_dir
from flowctl.core.state import load_task_with_state


def cmd_show(args: argparse.Namespace) -> None:
    """Show epic or task details."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    flow_dir = get_flow_dir()

    if is_epic_id(args.id):
        epic_path = flow_dir / EPICS_DIR / f"{args.id}.json"
        epic_data = normalize_epic(
            load_json_or_exit(epic_path, f"Epic {args.id}", use_json=args.json)
        )

        # Get tasks for this epic (with merged runtime state)
        tasks = []
        tasks_dir = flow_dir / TASKS_DIR
        if tasks_dir.exists():
            for task_file in sorted(tasks_dir.glob(f"{args.id}.*.json")):
                task_id = task_file.stem
                if not is_task_id(task_id):
                    continue  # Skip non-task files (e.g., fn-1.2-review.json)
                task_data = load_task_with_state(task_id, use_json=args.json)
                if "id" not in task_data:
                    continue  # Skip artifact files (GH-21)
                tasks.append(
                    {
                        "id": task_data["id"],
                        "title": task_data["title"],
                        "status": task_data["status"],
                        "priority": task_data.get("priority"),
                        "depends_on": task_data.get("depends_on", task_data.get("deps", [])),
                    }
                )

        # Sort tasks by numeric suffix (safe via parse_id)
        def task_sort_key(t):
            _, task_num = parse_id(t["id"])
            return task_num if task_num is not None else 0

        tasks.sort(key=task_sort_key)

        result = {**epic_data, "tasks": tasks}

        if args.json:
            json_output(result)
        else:
            print(f"Epic: {epic_data['id']}")
            print(f"Title: {epic_data['title']}")
            print(f"Status: {epic_data['status']}")
            print(f"Spec: {epic_data['spec_path']}")
            print(f"\nTasks ({len(tasks)}):")
            for t in tasks:
                deps = (
                    f" (deps: {', '.join(t['depends_on'])})" if t["depends_on"] else ""
                )
                print(f"  [{t['status']}] {t['id']}: {t['title']}{deps}")

    elif is_task_id(args.id):
        # Load task with merged runtime state
        task_data = load_task_with_state(args.id, use_json=args.json)

        if args.json:
            json_output(task_data)
        else:
            print(f"Task: {task_data['id']}")
            print(f"Epic: {task_data['epic']}")
            print(f"Title: {task_data['title']}")
            print(f"Status: {task_data['status']}")
            if task_data.get("domain"):
                print(f"Domain: {task_data['domain']}")
            print(f"Depends on: {', '.join(task_data['depends_on']) or 'none'}")
            print(f"Spec: {task_data['spec_path']}")

    else:
        error_exit(
            f"Invalid ID: {args.id}. Expected format: fn-N or fn-N-slug (epic), fn-N.M or fn-N-slug.M (task)",
            use_json=args.json,
        )


def cmd_epics(args: argparse.Namespace) -> None:
    """List all epics."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    flow_dir = get_flow_dir()
    epics_dir = flow_dir / EPICS_DIR

    epics = []
    if epics_dir.exists():
        for epic_file in sorted(epics_dir.glob("fn-*.json")):
            epic_data = normalize_epic(
                load_json_or_exit(
                    epic_file, f"Epic {epic_file.stem}", use_json=args.json
                )
            )
            # Count tasks (with merged runtime state)
            tasks_dir = flow_dir / TASKS_DIR
            task_count = 0
            done_count = 0
            if tasks_dir.exists():
                for task_file in tasks_dir.glob(f"{epic_data['id']}.*.json"):
                    task_id = task_file.stem
                    if not is_task_id(task_id):
                        continue  # Skip non-task files (e.g., fn-1.2-review.json)
                    task_data = load_task_with_state(task_id, use_json=args.json)
                    task_count += 1
                    if task_data.get("status") == "done":
                        done_count += 1

            epics.append(
                {
                    "id": epic_data["id"],
                    "title": epic_data["title"],
                    "status": epic_data["status"],
                    "tasks": task_count,
                    "done": done_count,
                }
            )

    # Sort by epic number
    def epic_sort_key(e):
        epic_num, _ = parse_id(e["id"])
        return epic_num if epic_num is not None else 0

    epics.sort(key=epic_sort_key)

    if args.json:
        json_output({"success": True, "epics": epics, "count": len(epics)})
    else:
        if not epics:
            print("No epics found.")
        else:
            print(f"Epics ({len(epics)}):\n")
            for e in epics:
                progress = f"{e['done']}/{e['tasks']}" if e["tasks"] > 0 else "0/0"
                print(
                    f"  [{e['status']}] {e['id']}: {e['title']} ({progress} tasks done)"
                )


def cmd_files(args: argparse.Namespace) -> None:
    """Show file ownership map for an epic -- which task owns which files."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    epic_id = args.epic
    if not is_epic_id(epic_id):
        error_exit(f"Invalid epic ID: {epic_id}", use_json=args.json)

    flow_dir = get_flow_dir()
    tasks_dir = flow_dir / TASKS_DIR

    # Collect files from task JSON + fallback to spec markdown
    ownership: dict[str, list[str]] = {}

    if tasks_dir.exists():
        for task_file in sorted(tasks_dir.glob(f"{epic_id}.*.json")):
            task_id = task_file.stem
            if not is_task_id(task_id):
                continue
            task_data = load_task_with_state(task_id, use_json=args.json)

            # Source 1: structured files field
            task_files = task_data.get("files", [])

            # Source 2: fallback -- parse **Files:** from spec markdown
            if not task_files:
                spec_path = flow_dir / TASKS_DIR / f"{task_id}.md"
                if spec_path.exists():
                    spec_text = spec_path.read_text(encoding="utf-8")
                    import re as _re
                    for line in spec_text.splitlines():
                        m = _re.match(r"\*\*Files:\*\*\s*(.*)", line)
                        if m:
                            task_files = [f.strip().strip("`") for f in m.group(1).split(",") if f.strip()]
                            break

            for fp in task_files:
                ownership.setdefault(fp, []).append(task_id)

    # Split into clean ownership vs conflicts
    clean = {f: tasks[0] for f, tasks in ownership.items() if len(tasks) == 1}
    conflicts = {f: tasks for f, tasks in ownership.items() if len(tasks) > 1}

    if args.json:
        json_output({
            "epic": epic_id,
            "ownership": {f: tasks for f, tasks in ownership.items()},
            "conflicts": conflicts,
            "file_count": len(ownership),
            "conflict_count": len(conflicts),
        })
    else:
        print(f"File ownership for {epic_id}:\n")
        if not ownership:
            print("  No files declared.")
        else:
            for f, tasks in sorted(ownership.items()):
                if len(tasks) == 1:
                    print(f"  {f} \u2192 {tasks[0]}")
                else:
                    print(f"  {f} \u2192 CONFLICT: {', '.join(tasks)}")
            if conflicts:
                print(f"\n  \u26a0 {len(conflicts)} file conflict(s) \u2014 tasks sharing files cannot run in parallel")


def cmd_tasks(args: argparse.Namespace) -> None:
    """List tasks."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    flow_dir = get_flow_dir()
    tasks_dir = flow_dir / TASKS_DIR

    tasks = []
    if tasks_dir.exists():
        pattern = f"{args.epic}.*.json" if args.epic else "fn-*.json"
        for task_file in sorted(tasks_dir.glob(pattern)):
            task_id = task_file.stem
            if not is_task_id(task_id):
                continue  # Skip non-task files (e.g., fn-1.2-review.json)
            # Load task with merged runtime state
            task_data = load_task_with_state(task_id, use_json=args.json)
            if "id" not in task_data:
                continue  # Skip artifact files (GH-21)
            # Filter by status if requested
            if args.status and task_data["status"] != args.status:
                continue
            # Filter by domain if requested
            if hasattr(args, "domain") and args.domain and task_data.get("domain") != args.domain:
                continue
            tasks.append(
                {
                    "id": task_data["id"],
                    "epic": task_data["epic"],
                    "title": task_data["title"],
                    "status": task_data["status"],
                    "priority": task_data.get("priority"),
                    "domain": task_data.get("domain"),
                    "depends_on": task_data.get("depends_on", task_data.get("deps", [])),
                }
            )

    # Sort tasks by epic number then task number
    def task_sort_key(t):
        epic_num, task_num = parse_id(t["id"])
        return (
            epic_num if epic_num is not None else 0,
            task_num if task_num is not None else 0,
        )

    tasks.sort(key=task_sort_key)

    if args.json:
        json_output({"success": True, "tasks": tasks, "count": len(tasks)})
    else:
        if not tasks:
            scope = f" for epic {args.epic}" if args.epic else ""
            status_filter = f" with status '{args.status}'" if args.status else ""
            print(f"No tasks found{scope}{status_filter}.")
        else:
            scope = f" for {args.epic}" if args.epic else ""
            print(f"Tasks{scope} ({len(tasks)}):\n")
            for t in tasks:
                deps = (
                    f" (deps: {', '.join(t['depends_on'])})" if t["depends_on"] else ""
                )
                domain_tag = f" [{t['domain']}]" if t.get("domain") else ""
                print(f"  [{t['status']}] {t['id']}: {t['title']}{domain_tag}{deps}")


def cmd_list(args: argparse.Namespace) -> None:
    """List all epics and their tasks."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    flow_dir = get_flow_dir()
    epics_dir = flow_dir / EPICS_DIR
    tasks_dir = flow_dir / TASKS_DIR

    # Load all epics
    epics = []
    if epics_dir.exists():
        for epic_file in sorted(epics_dir.glob("fn-*.json")):
            epic_data = normalize_epic(
                load_json_or_exit(
                    epic_file, f"Epic {epic_file.stem}", use_json=args.json
                )
            )
            epics.append(epic_data)

    # Sort epics by number
    def epic_sort_key(e):
        epic_num, _ = parse_id(e["id"])
        return epic_num if epic_num is not None else 0

    epics.sort(key=epic_sort_key)

    # Load all tasks grouped by epic (with merged runtime state)
    tasks_by_epic = {}
    all_tasks = []
    if tasks_dir.exists():
        for task_file in sorted(tasks_dir.glob("fn-*.json")):
            task_id = task_file.stem
            if not is_task_id(task_id):
                continue  # Skip non-task files (e.g., fn-1.2-review.json)
            task_data = load_task_with_state(task_id, use_json=args.json)
            if "id" not in task_data or "epic" not in task_data:
                continue  # Skip artifact files (GH-21)
            epic_id = task_data["epic"]
            if epic_id not in tasks_by_epic:
                tasks_by_epic[epic_id] = []
            tasks_by_epic[epic_id].append(task_data)
            all_tasks.append(
                {
                    "id": task_data["id"],
                    "epic": task_data["epic"],
                    "title": task_data["title"],
                    "status": task_data["status"],
                    "priority": task_data.get("priority"),
                    "depends_on": task_data.get("depends_on", task_data.get("deps", [])),
                }
            )

    # Sort tasks within each epic
    for epic_id in tasks_by_epic:
        tasks_by_epic[epic_id].sort(key=lambda t: parse_id(t["id"])[1] or 0)

    if args.json:
        epics_out = []
        for e in epics:
            task_list = tasks_by_epic.get(e["id"], [])
            done_count = sum(1 for t in task_list if t["status"] == "done")
            epics_out.append(
                {
                    "id": e["id"],
                    "title": e["title"],
                    "status": e["status"],
                    "tasks": len(task_list),
                    "done": done_count,
                }
            )
        json_output(
            {
                "success": True,
                "epics": epics_out,
                "tasks": all_tasks,
                "epic_count": len(epics),
                "task_count": len(all_tasks),
            }
        )
    else:
        if not epics:
            print("No epics or tasks found.")
            return

        total_tasks = len(all_tasks)
        total_done = sum(1 for t in all_tasks if t["status"] == "done")
        print(
            f"Flow Status: {len(epics)} epics, {total_tasks} tasks ({total_done} done)\n"
        )

        for e in epics:
            task_list = tasks_by_epic.get(e["id"], [])
            done_count = sum(1 for t in task_list if t["status"] == "done")
            progress = f"{done_count}/{len(task_list)}" if task_list else "0/0"
            print(f"[{e['status']}] {e['id']}: {e['title']} ({progress} done)")

            for t in task_list:
                deps = (
                    f" (deps: {', '.join(t['depends_on'])})" if t["depends_on"] else ""
                )
                print(f"    [{t['status']}] {t['id']}: {t['title']}{deps}")
            print()


def cmd_cat(args: argparse.Namespace) -> None:
    """Print markdown spec for epic or task."""
    if not ensure_flow_exists():
        error_exit(".flow/ does not exist. Run 'flowctl init' first.", use_json=False)

    flow_dir = get_flow_dir()

    if is_epic_id(args.id):
        spec_path = flow_dir / SPECS_DIR / f"{args.id}.md"
    elif is_task_id(args.id):
        spec_path = flow_dir / TASKS_DIR / f"{args.id}.md"
    else:
        error_exit(
            f"Invalid ID: {args.id}. Expected format: fn-N or fn-N-slug (epic), fn-N.M or fn-N-slug.M (task)",
            use_json=False,
        )
        return

    content = read_text_or_exit(spec_path, f"Spec {args.id}", use_json=False)
    print(content)
