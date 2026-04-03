"""Epic lifecycle commands: create, close, archive, set-*, deps."""

import argparse
import contextlib
import io
import json
import re
import shutil
import sys
from pathlib import Path

from flowctl.core.constants import (
    EPICS_DIR,
    FLOW_DIR,
    REVIEWS_DIR,
    SPECS_DIR,
    TASKS_DIR,
)
from flowctl.core.config import get_config
from flowctl.core.ids import (
    generate_epic_suffix,
    is_epic_id,
    is_task_id,
    normalize_epic,
    normalize_task,
    parse_id,
    slugify,
)
from flowctl.core.io import (
    atomic_write,
    atomic_write_json,
    error_exit,
    json_output,
    load_json,
    load_json_or_exit,
    now_iso,
    read_file_or_stdin,
)
from flowctl.core.paths import ensure_flow_exists, get_flow_dir
from flowctl.core.state import (
    delete_task_runtime,
    get_state_store,
    load_task_with_state,
)
from flowctl.commands.gap import GAP_BLOCKING_PRIORITIES


# --- Epic helpers ---


def scan_max_epic_id(flow_dir: Path) -> int:
    """Scan .flow/epics/ and .flow/specs/ to find max epic number. Returns 0 if none exist.

    Handles legacy (fn-N.json), short suffix (fn-N-xxx.json), and slug (fn-N-slug.json) formats.
    Also scans specs/*.md as safety net for orphaned specs created without flowctl.
    """
    max_n = 0
    pattern = r"^fn-(\d+)(?:-[a-z0-9][a-z0-9-]*[a-z0-9]|-[a-z0-9]{1,3})?\.(json|md)$"

    # Scan epics/*.json
    epics_dir = flow_dir / EPICS_DIR
    if epics_dir.exists():
        for epic_file in epics_dir.glob("fn-*.json"):
            match = re.match(pattern, epic_file.name)
            if match:
                n = int(match.group(1))
                max_n = max(max_n, n)

    # Scan specs/*.md as safety net (catches orphaned specs)
    specs_dir = flow_dir / SPECS_DIR
    if specs_dir.exists():
        for spec_file in specs_dir.glob("fn-*.md"):
            match = re.match(pattern, spec_file.name)
            if match:
                n = int(match.group(1))
                max_n = max(max_n, n)

    return max_n


def create_epic_spec(id_str: str, title: str) -> str:
    """Create epic spec markdown content."""
    return f"""# {id_str} {title}

## Overview
TBD

## Scope
TBD

## Approach
TBD

## Quick commands
<!-- Required: at least one smoke command for the repo -->
- `# e.g., npm test, bun test, make test`

## Acceptance
- [ ] TBD

## References
- TBD
"""


# --- Epic lifecycle commands ---


def cmd_epic_create(args: argparse.Namespace) -> None:
    """Create a new epic."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    from flowctl.core.constants import META_FILE

    flow_dir = get_flow_dir()
    meta_path = flow_dir / META_FILE
    load_json_or_exit(meta_path, "meta.json", use_json=args.json)

    # MU-1: Scan-based allocation for merge safety
    # Scan existing epics to determine next ID (don't rely on counter)
    max_epic = scan_max_epic_id(flow_dir)
    epic_num = max_epic + 1
    # Use slugified title as suffix, fallback to random if empty/invalid
    slug = slugify(args.title)
    suffix = slug if slug else generate_epic_suffix()
    epic_id = f"fn-{epic_num}-{suffix}"

    # Double-check no collision (shouldn't happen with scan-based allocation)
    epic_json_path = flow_dir / EPICS_DIR / f"{epic_id}.json"
    epic_spec_path = flow_dir / SPECS_DIR / f"{epic_id}.md"
    if epic_json_path.exists() or epic_spec_path.exists():
        error_exit(
            f"Refusing to overwrite existing epic {epic_id}. "
            f"This shouldn't happen - check for orphaned files.",
            use_json=args.json,
        )

    # Create epic JSON
    epic_data = {
        "id": epic_id,
        "title": args.title,
        "status": "open",
        "plan_review_status": "unknown",
        "plan_reviewed_at": None,
        "branch_name": args.branch if args.branch else epic_id,
        "depends_on_epics": [],
        "spec_path": f"{FLOW_DIR}/{SPECS_DIR}/{epic_id}.md",
        "next_task": 1,
        "created_at": now_iso(),
        "updated_at": now_iso(),
    }
    atomic_write_json(flow_dir / EPICS_DIR / f"{epic_id}.json", epic_data)

    # Create epic spec
    spec_content = create_epic_spec(epic_id, args.title)
    atomic_write(flow_dir / SPECS_DIR / f"{epic_id}.md", spec_content)

    # NOTE: We no longer update meta["next_epic"] since scan-based allocation
    # is the source of truth. This reduces merge conflicts.

    if args.json:
        json_output(
            {
                "id": epic_id,
                "title": args.title,
                "spec_path": epic_data["spec_path"],
                "message": f"Epic {epic_id} created",
            }
        )
    else:
        print(f"Epic {epic_id} created: {args.title}")


def cmd_epic_set_plan(args: argparse.Namespace) -> None:
    """Set/overwrite entire epic spec from file."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    if not is_epic_id(args.id):
        error_exit(
            f"Invalid epic ID: {args.id}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)", use_json=args.json
        )

    flow_dir = get_flow_dir()
    epic_path = flow_dir / EPICS_DIR / f"{args.id}.json"

    # Verify epic exists (will be loaded later for timestamp update)
    if not epic_path.exists():
        error_exit(f"Epic {args.id} not found", use_json=args.json)

    # Read content from file or stdin
    content = read_file_or_stdin(args.file, "Input file", use_json=args.json)

    # Validate spec headings: reject duplicate headings
    headings = re.findall(r"^(##\s+.+?)\s*$", content, flags=re.MULTILINE)
    seen = {}
    duplicates = []
    for h in headings:
        seen[h] = seen.get(h, 0) + 1
    for h, count in seen.items():
        if count > 1:
            duplicates.append(f"Duplicate heading: {h} (found {count} times)")
    if duplicates:
        error_exit(
            f"Spec validation failed: {'; '.join(duplicates)}",
            use_json=args.json,
        )

    # Write spec
    spec_path = flow_dir / SPECS_DIR / f"{args.id}.md"
    atomic_write(spec_path, content)

    # Update epic timestamp
    epic_data = load_json_or_exit(epic_path, f"Epic {args.id}", use_json=args.json)
    epic_data["updated_at"] = now_iso()
    atomic_write_json(epic_path, epic_data)

    if args.json:
        json_output(
            {
                "id": args.id,
                "spec_path": str(spec_path),
                "message": f"Epic {args.id} spec updated",
            }
        )
    else:
        print(f"Epic {args.id} spec updated")


def cmd_epic_set_plan_review_status(args: argparse.Namespace) -> None:
    """Set plan review status for an epic."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    if not is_epic_id(args.id):
        error_exit(
            f"Invalid epic ID: {args.id}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)", use_json=args.json
        )

    flow_dir = get_flow_dir()
    epic_path = flow_dir / EPICS_DIR / f"{args.id}.json"

    if not epic_path.exists():
        error_exit(f"Epic {args.id} not found", use_json=args.json)

    epic_data = normalize_epic(
        load_json_or_exit(epic_path, f"Epic {args.id}", use_json=args.json)
    )
    epic_data["plan_review_status"] = args.status
    epic_data["plan_reviewed_at"] = now_iso()
    epic_data["updated_at"] = now_iso()
    atomic_write_json(epic_path, epic_data)

    if args.json:
        json_output(
            {
                "id": args.id,
                "plan_review_status": epic_data["plan_review_status"],
                "plan_reviewed_at": epic_data["plan_reviewed_at"],
                "message": f"Epic {args.id} plan review status set to {args.status}",
            }
        )
    else:
        print(f"Epic {args.id} plan review status set to {args.status}")


def cmd_epic_set_completion_review_status(args: argparse.Namespace) -> None:
    """Set completion review status for an epic."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    if not is_epic_id(args.id):
        error_exit(
            f"Invalid epic ID: {args.id}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)", use_json=args.json
        )

    flow_dir = get_flow_dir()
    epic_path = flow_dir / EPICS_DIR / f"{args.id}.json"

    if not epic_path.exists():
        error_exit(f"Epic {args.id} not found", use_json=args.json)

    epic_data = normalize_epic(
        load_json_or_exit(epic_path, f"Epic {args.id}", use_json=args.json)
    )
    epic_data["completion_review_status"] = args.status
    epic_data["completion_reviewed_at"] = now_iso()
    epic_data["updated_at"] = now_iso()
    atomic_write_json(epic_path, epic_data)

    if args.json:
        json_output(
            {
                "id": args.id,
                "completion_review_status": epic_data["completion_review_status"],
                "completion_reviewed_at": epic_data["completion_reviewed_at"],
                "message": f"Epic {args.id} completion review status set to {args.status}",
            }
        )
    else:
        print(f"Epic {args.id} completion review status set to {args.status}")


def cmd_epic_set_branch(args: argparse.Namespace) -> None:
    """Set epic branch name."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    if not is_epic_id(args.id):
        error_exit(
            f"Invalid epic ID: {args.id}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)", use_json=args.json
        )

    flow_dir = get_flow_dir()
    epic_path = flow_dir / EPICS_DIR / f"{args.id}.json"

    if not epic_path.exists():
        error_exit(f"Epic {args.id} not found", use_json=args.json)

    epic_data = normalize_epic(
        load_json_or_exit(epic_path, f"Epic {args.id}", use_json=args.json)
    )
    epic_data["branch_name"] = args.branch
    epic_data["updated_at"] = now_iso()
    atomic_write_json(epic_path, epic_data)

    if args.json:
        json_output(
            {
                "id": args.id,
                "branch_name": epic_data["branch_name"],
                "message": f"Epic {args.id} branch_name set to {args.branch}",
            }
        )
    else:
        print(f"Epic {args.id} branch_name set to {args.branch}")


def cmd_epic_set_title(args: argparse.Namespace) -> None:
    """Rename epic by setting a new title (updates slug in ID, renames all files)."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    old_id = args.id
    if not is_epic_id(old_id):
        error_exit(
            f"Invalid epic ID: {old_id}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)",
            use_json=args.json,
        )

    flow_dir = get_flow_dir()
    old_epic_path = flow_dir / EPICS_DIR / f"{old_id}.json"

    if not old_epic_path.exists():
        error_exit(f"Epic {old_id} not found", use_json=args.json)

    epic_data = normalize_epic(
        load_json_or_exit(old_epic_path, f"Epic {old_id}", use_json=args.json)
    )

    # Extract epic number from old ID
    epic_num, _ = parse_id(old_id)
    if epic_num is None:
        error_exit(f"Could not parse epic number from {old_id}", use_json=args.json)

    # Generate new ID with slugified title
    new_slug = slugify(args.title)
    new_suffix = new_slug if new_slug else generate_epic_suffix()
    new_id = f"fn-{epic_num}-{new_suffix}"

    # Check if new ID already exists (and isn't same as old)
    if new_id != old_id:
        new_epic_path = flow_dir / EPICS_DIR / f"{new_id}.json"
        if new_epic_path.exists():
            error_exit(
                f"Epic {new_id} already exists. Choose a different title.",
                use_json=args.json,
            )

    # Collect files to rename
    renames: list[tuple[Path, Path]] = []
    specs_dir = flow_dir / SPECS_DIR
    tasks_dir = flow_dir / TASKS_DIR
    epics_dir = flow_dir / EPICS_DIR

    # Epic JSON
    renames.append((old_epic_path, epics_dir / f"{new_id}.json"))

    # Epic spec
    old_spec = specs_dir / f"{old_id}.md"
    if old_spec.exists():
        renames.append((old_spec, specs_dir / f"{new_id}.md"))

    # Task files (JSON and MD)
    task_files: list[tuple[str, str]] = []  # (old_task_id, new_task_id)
    if tasks_dir.exists():
        for task_file in tasks_dir.glob(f"{old_id}.*.json"):
            task_id = task_file.stem
            if not is_task_id(task_id):
                continue
            # Extract task number
            _, task_num = parse_id(task_id)
            if task_num is not None:
                new_task_id = f"{new_id}.{task_num}"
                task_files.append((task_id, new_task_id))
                # JSON file
                renames.append((task_file, tasks_dir / f"{new_task_id}.json"))
                # MD file
                old_task_md = tasks_dir / f"{task_id}.md"
                if old_task_md.exists():
                    renames.append((old_task_md, tasks_dir / f"{new_task_id}.md"))

    # Checkpoint file
    old_checkpoint = flow_dir / f".checkpoint-{old_id}.json"
    if old_checkpoint.exists():
        renames.append((old_checkpoint, flow_dir / f".checkpoint-{new_id}.json"))

    # Perform renames (collect errors but continue)
    rename_errors: list[str] = []
    for old_path, new_path in renames:
        try:
            old_path.rename(new_path)
        except OSError as e:
            rename_errors.append(f"{old_path.name} -> {new_path.name}: {e}")

    if rename_errors:
        error_exit(
            f"Failed to rename some files: {'; '.join(rename_errors)}",
            use_json=args.json,
        )

    # Update epic JSON content
    epic_data["id"] = new_id
    epic_data["title"] = args.title
    epic_data["spec_path"] = f"{FLOW_DIR}/{SPECS_DIR}/{new_id}.md"
    epic_data["updated_at"] = now_iso()
    atomic_write_json(epics_dir / f"{new_id}.json", epic_data)

    # Update task JSON content
    task_id_map = dict(task_files)  # old_task_id -> new_task_id
    for old_task_id, new_task_id in task_files:
        task_path = tasks_dir / f"{new_task_id}.json"
        if task_path.exists():
            task_data = normalize_task(load_json(task_path))
            task_data["id"] = new_task_id
            task_data["epic"] = new_id
            task_data["spec_path"] = f"{FLOW_DIR}/{TASKS_DIR}/{new_task_id}.md"
            # Update depends_on references within same epic
            if task_data.get("depends_on"):
                task_data["depends_on"] = [
                    task_id_map.get(dep, dep) for dep in task_data["depends_on"]
                ]
            task_data["updated_at"] = now_iso()
            atomic_write_json(task_path, task_data)

    # Update depends_on_epics in other epics that reference this one
    updated_deps_in: list[str] = []
    if epics_dir.exists():
        for other_epic_file in epics_dir.glob("fn-*.json"):
            if other_epic_file.name == f"{new_id}.json":
                continue  # Skip self
            try:
                other_data = load_json(other_epic_file)
                deps = other_data.get("depends_on_epics", [])
                if old_id in deps:
                    other_data["depends_on_epics"] = [
                        new_id if d == old_id else d for d in deps
                    ]
                    other_data["updated_at"] = now_iso()
                    atomic_write_json(other_epic_file, other_data)
                    updated_deps_in.append(other_data.get("id", other_epic_file.stem))
            except (json.JSONDecodeError, OSError):
                pass  # Skip files that can't be parsed

    # Update state files if they exist
    state_store = get_state_store()
    state_tasks_dir = state_store.tasks_dir
    if state_tasks_dir.exists():
        for old_task_id, new_task_id in task_files:
            old_state = state_tasks_dir / f"{old_task_id}.state.json"
            new_state = state_tasks_dir / f"{new_task_id}.state.json"
            if old_state.exists():
                try:
                    old_state.rename(new_state)
                except OSError:
                    pass  # Non-critical

    result = {
        "old_id": old_id,
        "new_id": new_id,
        "title": args.title,
        "files_renamed": len(renames),
        "tasks_updated": len(task_files),
        "message": f"Epic renamed: {old_id} -> {new_id}",
    }
    if updated_deps_in:
        result["updated_deps_in"] = updated_deps_in

    if args.json:
        json_output(result)
    else:
        print(f"Epic renamed: {old_id} -> {new_id}")
        print(f"  Title: {args.title}")
        print(f"  Files renamed: {len(renames)}")
        print(f"  Tasks updated: {len(task_files)}")
        if updated_deps_in:
            print(f"  Updated deps in: {', '.join(updated_deps_in)}")


def cmd_epic_add_dep(args: argparse.Namespace) -> None:
    """Add epic-level dependency."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    epic_id = args.epic
    dep_id = args.depends_on

    if not is_epic_id(epic_id):
        error_exit(
            f"Invalid epic ID: {epic_id}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)",
            use_json=args.json,
        )
    if not is_epic_id(dep_id):
        error_exit(
            f"Invalid epic ID: {dep_id}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)",
            use_json=args.json,
        )
    if epic_id == dep_id:
        error_exit("Epic cannot depend on itself", use_json=args.json)

    flow_dir = get_flow_dir()
    epic_path = flow_dir / EPICS_DIR / f"{epic_id}.json"
    dep_path = flow_dir / EPICS_DIR / f"{dep_id}.json"

    if not epic_path.exists():
        error_exit(f"Epic {epic_id} not found", use_json=args.json)
    if not dep_path.exists():
        error_exit(f"Epic {dep_id} not found", use_json=args.json)

    epic_data = load_json_or_exit(epic_path, f"Epic {epic_id}", use_json=args.json)
    deps = epic_data.get("depends_on_epics", [])

    if dep_id in deps:
        # Already exists, no-op success
        if args.json:
            json_output(
                {
                    "success": True,
                    "id": epic_id,
                    "depends_on_epics": deps,
                    "message": f"{dep_id} already in dependencies",
                }
            )
        else:
            print(f"{dep_id} already in {epic_id} dependencies")
        return

    deps.append(dep_id)
    epic_data["depends_on_epics"] = deps
    epic_data["updated_at"] = now_iso()
    atomic_write_json(epic_path, epic_data)

    if args.json:
        json_output(
            {
                "success": True,
                "id": epic_id,
                "depends_on_epics": deps,
                "message": f"Added {dep_id} to {epic_id} dependencies",
            }
        )
    else:
        print(f"Added {dep_id} to {epic_id} dependencies")


def cmd_epic_rm_dep(args: argparse.Namespace) -> None:
    """Remove epic-level dependency."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    epic_id = args.epic
    dep_id = args.depends_on

    if not is_epic_id(epic_id):
        error_exit(
            f"Invalid epic ID: {epic_id}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)",
            use_json=args.json,
        )

    flow_dir = get_flow_dir()
    epic_path = flow_dir / EPICS_DIR / f"{epic_id}.json"

    if not epic_path.exists():
        error_exit(f"Epic {epic_id} not found", use_json=args.json)

    epic_data = load_json_or_exit(epic_path, f"Epic {epic_id}", use_json=args.json)
    deps = epic_data.get("depends_on_epics", [])

    if dep_id not in deps:
        # Not in deps, no-op success
        if args.json:
            json_output(
                {
                    "success": True,
                    "id": epic_id,
                    "depends_on_epics": deps,
                    "message": f"{dep_id} not in dependencies",
                }
            )
        else:
            print(f"{dep_id} not in {epic_id} dependencies")
        return

    deps.remove(dep_id)
    epic_data["depends_on_epics"] = deps
    epic_data["updated_at"] = now_iso()
    atomic_write_json(epic_path, epic_data)

    if args.json:
        json_output(
            {
                "success": True,
                "id": epic_id,
                "depends_on_epics": deps,
                "message": f"Removed {dep_id} from {epic_id} dependencies",
            }
        )
    else:
        print(f"Removed {dep_id} from {epic_id} dependencies")


def cmd_epic_set_backend(args: argparse.Namespace) -> None:
    """Set epic default backend specs for impl/review/sync."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    if not is_epic_id(args.id):
        error_exit(
            f"Invalid epic ID: {args.id}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)",
            use_json=args.json,
        )

    # At least one of impl/review/sync must be provided
    if args.impl is None and args.review is None and args.sync is None:
        error_exit(
            "At least one of --impl, --review, or --sync must be provided",
            use_json=args.json,
        )

    flow_dir = get_flow_dir()
    epic_path = flow_dir / EPICS_DIR / f"{args.id}.json"

    if not epic_path.exists():
        error_exit(f"Epic {args.id} not found", use_json=args.json)

    epic_data = normalize_epic(
        load_json_or_exit(epic_path, f"Epic {args.id}", use_json=args.json)
    )

    # Update fields (empty string means clear)
    updated = []
    if args.impl is not None:
        epic_data["default_impl"] = args.impl if args.impl else None
        updated.append(f"default_impl={args.impl or 'null'}")
    if args.review is not None:
        epic_data["default_review"] = args.review if args.review else None
        updated.append(f"default_review={args.review or 'null'}")
    if args.sync is not None:
        epic_data["default_sync"] = args.sync if args.sync else None
        updated.append(f"default_sync={args.sync or 'null'}")

    epic_data["updated_at"] = now_iso()
    atomic_write_json(epic_path, epic_data)

    if args.json:
        json_output(
            {
                "id": args.id,
                "default_impl": epic_data["default_impl"],
                "default_review": epic_data["default_review"],
                "default_sync": epic_data["default_sync"],
                "message": f"Epic {args.id} backend specs updated: {', '.join(updated)}",
            }
        )
    else:
        print(f"Epic {args.id} backend specs updated: {', '.join(updated)}")


def cmd_epic_close(args: argparse.Namespace) -> None:
    """Close an epic (all tasks must be done)."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    if not is_epic_id(args.id):
        error_exit(
            f"Invalid epic ID: {args.id}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)", use_json=args.json
        )

    flow_dir = get_flow_dir()
    epic_path = flow_dir / EPICS_DIR / f"{args.id}.json"

    if not epic_path.exists():
        error_exit(f"Epic {args.id} not found", use_json=args.json)

    # Check all tasks are done (with merged runtime state)
    tasks_dir = flow_dir / TASKS_DIR
    if not tasks_dir.exists():
        error_exit(
            f"{TASKS_DIR}/ missing. Run 'flowctl init' or fix repo state.",
            use_json=args.json,
        )
    incomplete = []
    for task_file in tasks_dir.glob(f"{args.id}.*.json"):
        task_id = task_file.stem
        if not is_task_id(task_id):
            continue  # Skip non-task files (e.g., fn-1.2-review.json)
        task_data = load_task_with_state(task_id, use_json=args.json)
        if task_data["status"] not in ("done", "skipped"):
            incomplete.append(f"{task_data['id']} ({task_data['status']})")

    if incomplete:
        error_exit(
            f"Cannot close epic: incomplete tasks - {', '.join(incomplete)}",
            use_json=args.json,
        )

    epic_data = normalize_epic(
        load_json_or_exit(epic_path, f"Epic {args.id}", use_json=args.json)
    )

    # Gap registry gate
    skip_gap = getattr(args, "skip_gap_check", False)
    open_blocking = [
        g for g in epic_data.get("gaps", [])
        if g["status"] == "open" and g.get("priority") in GAP_BLOCKING_PRIORITIES
    ]
    if open_blocking and not skip_gap:
        gap_list = ", ".join(f"[{g['priority']}] {g['capability']}" for g in open_blocking)
        error_exit(
            f"Cannot close epic: {len(open_blocking)} unresolved blocking gap(s): {gap_list}. "
            f"Use --skip-gap-check to bypass.",
            use_json=args.json,
        )
    if open_blocking and skip_gap:
        msg = f"WARNING: Bypassing {len(open_blocking)} unresolved blocking gap(s)"
        if not args.json:
            print(msg, file=sys.stderr)

    epic_data["status"] = "done"
    epic_data["updated_at"] = now_iso()
    atomic_write_json(epic_path, epic_data)

    # Check if memory is enabled — suggest retro for learning loop
    memory_enabled = False
    if ensure_flow_exists():
        mem_cfg = get_config("memory.enabled")
        memory_enabled = mem_cfg in (True, "true", "True")

    if args.json:
        result = {
            "id": args.id, "status": "done", "message": f"Epic {args.id} closed",
            "gaps_skipped": len(open_blocking) if skip_gap else 0,
            "retro_suggested": True,
        }
        if memory_enabled:
            result["retro_hint"] = "Run /flow-code:retro to capture lessons learned"
        json_output(result)
    else:
        print(f"Epic {args.id} closed")
        print(f"\n  Tip: Run /flow-code:retro to capture lessons learned before archiving.")


def cmd_epic_archive(args: argparse.Namespace) -> None:
    """Archive a closed epic -- move its files to .flow/.archive/<epic-id>/."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    epic_id = args.id
    if not is_epic_id(epic_id):
        error_exit(
            f"Invalid epic ID: {epic_id}. Expected format: fn-N or fn-N-slug",
            use_json=args.json,
        )

    flow_dir = get_flow_dir()
    epic_path = flow_dir / EPICS_DIR / f"{epic_id}.json"

    if not epic_path.exists():
        error_exit(f"Epic {epic_id} not found", use_json=args.json)

    epic_data = load_json_or_exit(epic_path, f"Epic {epic_id}", use_json=args.json)
    if epic_data.get("status") != "done" and not args.force:
        error_exit(
            f"Cannot archive epic {epic_id}: status is '{epic_data.get('status')}', not 'done'. "
            f"Close it first or use --force.",
            use_json=args.json,
        )

    # Build archive directory
    archive_dir = flow_dir / ".archive" / epic_id
    archive_dir.mkdir(parents=True, exist_ok=True)

    moved: list[str] = []

    # Move epic JSON
    shutil.move(str(epic_path), str(archive_dir / epic_path.name))
    moved.append(f"epics/{epic_path.name}")

    # Move epic spec
    spec_path = flow_dir / SPECS_DIR / f"{epic_id}.md"
    if spec_path.exists():
        shutil.move(str(spec_path), str(archive_dir / spec_path.name))
        moved.append(f"specs/{spec_path.name}")

    # Move all task files (JSON + spec)
    tasks_dir = flow_dir / TASKS_DIR
    if tasks_dir.exists():
        for task_file in sorted(tasks_dir.glob(f"{epic_id}.*")):
            shutil.move(str(task_file), str(archive_dir / task_file.name))
            moved.append(f"tasks/{task_file.name}")

    # Move review receipts
    reviews_dir = flow_dir / REVIEWS_DIR
    if reviews_dir.exists():
        for review_file in sorted(reviews_dir.glob(f"*-{epic_id}.*")):
            shutil.move(str(review_file), str(archive_dir / review_file.name))
            moved.append(f"reviews/{review_file.name}")

    # Clean up runtime state for archived tasks
    for f in archive_dir.glob(f"{epic_id}.*.json"):
        tid = f.stem
        if is_task_id(tid):
            try:
                delete_task_runtime(tid)
            except Exception:
                pass

    if args.json:
        json_output({
            "success": True,
            "epic": epic_id,
            "archive_dir": str(archive_dir),
            "moved": moved,
            "count": len(moved),
        })
    else:
        print(f"Archived epic {epic_id} ({len(moved)} files) \u2192 .flow/.archive/{epic_id}/")
        for f in moved:
            print(f"  {f}")


def cmd_epic_reopen(args: argparse.Namespace) -> None:
    """Reopen a closed epic (sets status back to open)."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    epic_id = args.id
    if not is_epic_id(epic_id):
        error_exit(
            f"Invalid epic ID: {epic_id}. Expected format: fn-N or fn-N-slug "
            f"(e.g., fn-1, fn-1-add-auth)",
            use_json=args.json,
        )

    flow_dir = get_flow_dir()
    epic_path = flow_dir / EPICS_DIR / f"{epic_id}.json"

    if not epic_path.exists():
        # Check if archived
        archive_path = flow_dir / ".archive" / epic_id
        if archive_path.exists():
            error_exit(
                f"Epic {epic_id} is archived. Unarchive it first before reopening.",
                use_json=args.json,
            )
        error_exit(f"Epic {epic_id} not found", use_json=args.json)

    epic_data = normalize_epic(
        load_json_or_exit(epic_path, f"Epic {epic_id}", use_json=args.json)
    )

    previous_status = epic_data.get("status", "unknown")

    if previous_status == "open":
        error_exit(
            f"Epic {epic_id} is already open (no-op protection)",
            use_json=args.json,
        )

    # Set status back to open and reset review metadata
    epic_data["status"] = "open"
    epic_data["completion_review_status"] = "unknown"
    epic_data["plan_review_status"] = "unknown"
    epic_data.pop("plan_reviewed_at", None)
    epic_data["updated_at"] = now_iso()
    atomic_write_json(epic_path, epic_data)

    if args.json:
        json_output(
            {
                "id": epic_id,
                "previous_status": previous_status,
                "new_status": "open",
                "message": f"Epic {epic_id} reopened",
            }
        )
    else:
        print(f"Epic {epic_id} reopened (was: {previous_status})")


def cmd_epic_clean(args: argparse.Namespace) -> None:
    """Archive all closed epics at once."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    flow_dir = get_flow_dir()
    epics_dir = flow_dir / EPICS_DIR

    archived = []
    if epics_dir.exists():
        for epic_file in sorted(epics_dir.glob("fn-*.json")):
            try:
                epic_data = load_json(epic_file)
            except Exception:
                continue
            if epic_data.get("status") != "done":
                continue

            epic_id = epic_data.get("id", epic_file.stem)
            # Archive silently (suppress inner output)
            fake_args = argparse.Namespace(
                id=epic_id, force=False, json=True
            )
            with contextlib.redirect_stdout(io.StringIO()):
                cmd_epic_archive(fake_args)
            archived.append(epic_id)

    if args.json:
        json_output({
            "success": True,
            "archived": archived,
            "count": len(archived),
        })
    else:
        if archived:
            print(f"Archived {len(archived)} closed epic(s): {', '.join(archived)}")
        else:
            print("No closed epics to archive.")
