"""Task CRUD commands: create, set-*, reset, and helpers."""

import argparse
import re
from pathlib import Path
from typing import Optional

from flowctl.core.constants import (
    EPICS_DIR,
    FLOW_DIR,
    TASKS_DIR,
)
from flowctl.core.ids import (
    epic_id_from_task,
    is_epic_id,
    is_task_id,
    normalize_epic,
    normalize_task,
    parse_id,
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
    read_text_or_exit,
)
from flowctl.core.paths import ensure_flow_exists, get_flow_dir
from flowctl.core.state import (
    load_task_with_state,
    reset_task_runtime,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def scan_max_task_id(flow_dir: Path, epic_id: str) -> int:
    """Scan .flow/tasks/ to find max task number for an epic. Returns 0 if none exist."""
    tasks_dir = flow_dir / TASKS_DIR
    if not tasks_dir.exists():
        return 0

    max_m = 0
    for task_file in tasks_dir.glob(f"{epic_id}.*.json"):
        match = re.match(rf"^{re.escape(epic_id)}\.(\d+)\.json$", task_file.name)
        if match:
            m = int(match.group(1))
            max_m = max(max_m, m)
    return max_m


def create_task_spec(id_str: str, title: str, acceptance: Optional[str] = None) -> str:
    """Create task spec markdown content."""
    acceptance_content = acceptance if acceptance else "- [ ] TBD"
    return f"""# {id_str} {title}

## Description
TBD

## Acceptance
{acceptance_content}

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
"""


def patch_task_section(content: str, section: str, new_content: str) -> str:
    """Patch a specific section in task spec. Preserves other sections.

    Raises ValueError on invalid content (duplicate/missing headings).
    """
    # Check for duplicate headings first (defensive)
    pattern = rf"^{re.escape(section)}\s*$"
    matches = len(re.findall(pattern, content, flags=re.MULTILINE))
    if matches > 1:
        raise ValueError(
            f"Cannot patch: duplicate heading '{section}' found ({matches} times)"
        )

    # Strip leading section heading from new_content if present (defensive)
    # Handles case where agent includes "## Description" in temp file
    new_lines = new_content.lstrip().split("\n")
    if new_lines and new_lines[0].strip() == section:
        new_content = "\n".join(new_lines[1:]).lstrip()

    lines = content.split("\n")
    result = []
    in_target_section = False
    section_found = False

    for i, line in enumerate(lines):
        if line.startswith("## "):
            if line.strip() == section:
                in_target_section = True
                section_found = True
                result.append(line)
                # Add new content
                result.append(new_content.rstrip())
                continue
            else:
                in_target_section = False

        if not in_target_section:
            result.append(line)

    if not section_found:
        # Auto-append missing section instead of failing
        result.append("")
        result.append(section)
        result.append(new_content.rstrip())

    return "\n".join(result)


def get_task_section(content: str, section: str) -> str:
    """Get content under a task section heading."""
    lines = content.split("\n")
    in_target = False
    collected = []
    for line in lines:
        if line.startswith("## "):
            if line.strip() == section:
                in_target = True
                continue
            if in_target:
                break
        if in_target:
            collected.append(line)
    return "\n".join(collected).strip()


def clear_task_evidence(task_id: str) -> None:
    """Clear ## Evidence section contents but keep the heading with empty template."""
    flow_dir = get_flow_dir()
    spec_path = flow_dir / TASKS_DIR / f"{task_id}.md"
    if not spec_path.exists():
        return
    content = spec_path.read_text(encoding="utf-8")

    # Replace contents under ## Evidence with empty template, keeping heading
    pattern = r"(## Evidence\s*\r?\n).*?(?=\r?\n## |\Z)"
    replacement = r"\g<1>- Commits:\n- Tests:\n- PRs:\n"
    new_content = re.sub(pattern, replacement, content, flags=re.DOTALL)

    if new_content != content:
        atomic_write(spec_path, new_content)


def find_dependents(task_id: str, same_epic: bool = False) -> list[str]:
    """Find tasks that depend on task_id (recursive). Returns list of dependent task IDs."""
    flow_dir = get_flow_dir()
    tasks_dir = flow_dir / TASKS_DIR
    if not tasks_dir.exists():
        return []

    epic_id = epic_id_from_task(task_id) if same_epic else None
    dependents: set[str] = set()  # Use set to avoid duplicates
    to_check = [task_id]
    checked = set()

    while to_check:
        checking = to_check.pop(0)
        if checking in checked:
            continue
        checked.add(checking)

        for task_file in tasks_dir.glob("fn-*.json"):
            if not is_task_id(task_file.stem):
                continue  # Skip non-task files (e.g., fn-1.2-review.json)
            try:
                task_data = load_json(task_file)
                tid = task_data.get("id", task_file.stem)
                if tid in checked or tid in dependents:
                    continue
                # Skip if same_epic filter and different epic
                if same_epic and epic_id_from_task(tid) != epic_id:
                    continue
                # Support both legacy "deps" and current "depends_on"
                deps = task_data.get("depends_on", task_data.get("deps", []))
                if checking in deps:
                    dependents.add(tid)
                    to_check.append(tid)
            except Exception:
                pass

    return sorted(dependents)


# ---------------------------------------------------------------------------
# Task set-section helper
# ---------------------------------------------------------------------------

def _task_set_section(
    task_id: str, section: str, file_path: str, use_json: bool
) -> None:
    """Helper to set a task spec section."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=use_json
        )

    if not is_task_id(task_id):
        error_exit(
            f"Invalid task ID: {task_id}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)", use_json=use_json
        )

    flow_dir = get_flow_dir()
    task_json_path = flow_dir / TASKS_DIR / f"{task_id}.json"
    task_spec_path = flow_dir / TASKS_DIR / f"{task_id}.md"

    # Verify task exists
    if not task_json_path.exists():
        error_exit(f"Task {task_id} not found", use_json=use_json)

    # Read new content from file or stdin
    new_content = read_file_or_stdin(file_path, "Input file", use_json=use_json)

    # Load task JSON first (fail early before any writes)
    task_data = load_json_or_exit(task_json_path, f"Task {task_id}", use_json=use_json)

    # Read current spec
    current_spec = read_text_or_exit(
        task_spec_path, f"Task {task_id} spec", use_json=use_json
    )

    # Patch section
    try:
        updated_spec = patch_task_section(current_spec, section, new_content)
    except ValueError as e:
        error_exit(str(e), use_json=use_json)

    # Write spec then JSON (both validated above)
    atomic_write(task_spec_path, updated_spec)
    task_data["updated_at"] = now_iso()
    atomic_write_json(task_json_path, task_data)

    if use_json:
        json_output(
            {
                "id": task_id,
                "section": section,
                "message": f"Task {task_id} {section} updated",
            }
        )
    else:
        print(f"Task {task_id} {section} updated")


# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------

def cmd_task_create(args: argparse.Namespace) -> None:
    """Create a new task under an epic."""
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

    load_json_or_exit(epic_path, f"Epic {args.epic}", use_json=args.json)

    # MU-1: Scan-based allocation for merge safety
    # Scan existing tasks to determine next ID (don't rely on counter)
    max_task = scan_max_task_id(flow_dir, args.epic)
    task_num = max_task + 1
    task_id = f"{args.epic}.{task_num}"

    # Double-check no collision (shouldn't happen with scan-based allocation)
    task_json_path = flow_dir / TASKS_DIR / f"{task_id}.json"
    task_spec_path = flow_dir / TASKS_DIR / f"{task_id}.md"
    if task_json_path.exists() or task_spec_path.exists():
        error_exit(
            f"Refusing to overwrite existing task {task_id}. "
            f"This shouldn't happen - check for orphaned files.",
            use_json=args.json,
        )

    # Parse dependencies
    deps = []
    if args.deps:
        deps = [d.strip() for d in args.deps.split(",")]
        # Validate deps are valid task IDs within same epic
        for dep in deps:
            if not is_task_id(dep):
                error_exit(
                    f"Invalid dependency ID: {dep}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
                    use_json=args.json,
                )
            if epic_id_from_task(dep) != args.epic:
                error_exit(
                    f"Dependency {dep} must be within the same epic ({args.epic})",
                    use_json=args.json,
                )

    # Read acceptance from file if provided
    acceptance = None
    if args.acceptance_file:
        acceptance = read_text_or_exit(
            Path(args.acceptance_file), "Acceptance file", use_json=args.json
        )

    # Validate domain if provided
    valid_domains = ["frontend", "backend", "architecture", "testing", "docs", "ops", "general"]
    domain = getattr(args, "domain", None)
    if domain and domain not in valid_domains:
        error_exit(
            f"Invalid domain: {domain}. Valid: {', '.join(valid_domains)}",
            use_json=args.json,
        )

    # Parse files if provided
    files = []
    if getattr(args, "files", None):
        files = [f.strip() for f in args.files.split(",") if f.strip()]

    # Create task JSON (MU-2: includes soft-claim fields)
    task_data = {
        "id": task_id,
        "epic": args.epic,
        "title": args.title,
        "status": "todo",
        "priority": args.priority,
        "depends_on": deps,
        "domain": domain,
        "files": files,
        "assignee": None,
        "claimed_at": None,
        "claim_note": "",
        "spec_path": f"{FLOW_DIR}/{TASKS_DIR}/{task_id}.md",
        "created_at": now_iso(),
        "updated_at": now_iso(),
    }
    atomic_write_json(flow_dir / TASKS_DIR / f"{task_id}.json", task_data)

    # Create task spec
    spec_content = create_task_spec(task_id, args.title, acceptance)
    atomic_write(flow_dir / TASKS_DIR / f"{task_id}.md", spec_content)

    # NOTE: We no longer update epic["next_task"] since scan-based allocation
    # is the source of truth. This reduces merge conflicts.

    if args.json:
        json_output(
            {
                "id": task_id,
                "epic": args.epic,
                "title": args.title,
                "depends_on": deps,
                "spec_path": task_data["spec_path"],
                "message": f"Task {task_id} created",
            }
        )
    else:
        print(f"Task {task_id} created: {args.title}")


def cmd_dep_add(args: argparse.Namespace) -> None:
    """Add a dependency to a task."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    if not is_task_id(args.task):
        error_exit(
            f"Invalid task ID: {args.task}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)", use_json=args.json
        )

    if not is_task_id(args.depends_on):
        error_exit(
            f"Invalid dependency ID: {args.depends_on}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
            use_json=args.json,
        )

    # Validate same epic
    task_epic = epic_id_from_task(args.task)
    dep_epic = epic_id_from_task(args.depends_on)
    if task_epic != dep_epic:
        error_exit(
            f"Dependencies must be within the same epic. Task {args.task} is in {task_epic}, dependency {args.depends_on} is in {dep_epic}",
            use_json=args.json,
        )

    flow_dir = get_flow_dir()
    task_path = flow_dir / TASKS_DIR / f"{args.task}.json"

    task_data = load_json_or_exit(task_path, f"Task {args.task}", use_json=args.json)

    # Migrate old 'deps' key to 'depends_on' if needed
    if "depends_on" not in task_data:
        task_data["depends_on"] = task_data.pop("deps", [])

    if args.depends_on not in task_data["depends_on"]:
        task_data["depends_on"].append(args.depends_on)
        task_data["updated_at"] = now_iso()
        atomic_write_json(task_path, task_data)

    if args.json:
        json_output(
            {
                "task": args.task,
                "depends_on": task_data["depends_on"],
                "message": f"Dependency {args.depends_on} added to {args.task}",
            }
        )
    else:
        print(f"Dependency {args.depends_on} added to {args.task}")


def cmd_task_set_deps(args: argparse.Namespace) -> None:
    """Set dependencies for a task (convenience wrapper for dep add)."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    if not is_task_id(args.task_id):
        error_exit(
            f"Invalid task ID: {args.task_id}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
            use_json=args.json,
        )

    if not args.deps:
        error_exit("--deps is required", use_json=args.json)

    # Parse comma-separated deps
    dep_ids = [d.strip() for d in args.deps.split(",") if d.strip()]
    if not dep_ids:
        error_exit("--deps cannot be empty", use_json=args.json)

    task_epic = epic_id_from_task(args.task_id)
    flow_dir = get_flow_dir()
    task_path = flow_dir / TASKS_DIR / f"{args.task_id}.json"

    task_data = load_json_or_exit(
        task_path, f"Task {args.task_id}", use_json=args.json
    )

    # Migrate old 'deps' key if needed
    if "depends_on" not in task_data:
        task_data["depends_on"] = task_data.pop("deps", [])

    added = []
    for dep_id in dep_ids:
        if not is_task_id(dep_id):
            error_exit(
                f"Invalid dependency ID: {dep_id}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
                use_json=args.json,
            )
        dep_epic = epic_id_from_task(dep_id)
        if dep_epic != task_epic:
            error_exit(
                f"Dependencies must be within same epic. Task {args.task_id} is in {task_epic}, dependency {dep_id} is in {dep_epic}",
                use_json=args.json,
            )
        if dep_id not in task_data["depends_on"]:
            task_data["depends_on"].append(dep_id)
            added.append(dep_id)

    if added:
        task_data["updated_at"] = now_iso()
        atomic_write_json(task_path, task_data)

    if args.json:
        json_output(
            {
                "success": True,
                "task": args.task_id,
                "depends_on": task_data["depends_on"],
                "added": added,
                "message": f"Dependencies set for {args.task_id}",
            }
        )
    else:
        if added:
            print(f"Added dependencies to {args.task_id}: {', '.join(added)}")
        else:
            print(f"No new dependencies added (already set)")


def cmd_task_set_backend(args: argparse.Namespace) -> None:
    """Set task backend specs for impl/review/sync."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    task_id = args.id
    if not is_task_id(task_id):
        error_exit(
            f"Invalid task ID: {task_id}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
            use_json=args.json,
        )

    # At least one of impl/review/sync must be provided
    if args.impl is None and args.review is None and args.sync is None:
        error_exit(
            "At least one of --impl, --review, or --sync must be provided",
            use_json=args.json,
        )

    flow_dir = get_flow_dir()
    task_path = flow_dir / TASKS_DIR / f"{task_id}.json"

    if not task_path.exists():
        error_exit(f"Task {task_id} not found", use_json=args.json)

    task_data = load_json_or_exit(task_path, f"Task {task_id}", use_json=args.json)

    # Update fields (empty string means clear)
    updated = []
    if args.impl is not None:
        task_data["impl"] = args.impl if args.impl else None
        updated.append(f"impl={args.impl or 'null'}")
    if args.review is not None:
        task_data["review"] = args.review if args.review else None
        updated.append(f"review={args.review or 'null'}")
    if args.sync is not None:
        task_data["sync"] = args.sync if args.sync else None
        updated.append(f"sync={args.sync or 'null'}")

    atomic_write_json(task_path, task_data)

    if args.json:
        json_output(
            {
                "id": task_id,
                "impl": task_data.get("impl"),
                "review": task_data.get("review"),
                "sync": task_data.get("sync"),
                "message": f"Task {task_id} backend specs updated: {', '.join(updated)}",
            }
        )
    else:
        print(f"Task {task_id} backend specs updated: {', '.join(updated)}")


def cmd_task_show_backend(args: argparse.Namespace) -> None:
    """Show effective backend specs for a task (task + epic levels only)."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    task_id = args.id
    if not is_task_id(task_id):
        error_exit(
            f"Invalid task ID: {task_id}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
            use_json=args.json,
        )

    flow_dir = get_flow_dir()
    task_path = flow_dir / TASKS_DIR / f"{task_id}.json"

    if not task_path.exists():
        error_exit(f"Task {task_id} not found", use_json=args.json)

    task_data = normalize_task(
        load_json_or_exit(task_path, f"Task {task_id}", use_json=args.json)
    )

    # Get epic data for defaults
    epic_id = task_data.get("epic")
    epic_data = None
    if epic_id:
        epic_path = flow_dir / EPICS_DIR / f"{epic_id}.json"
        if epic_path.exists():
            epic_data = normalize_epic(
                load_json_or_exit(epic_path, f"Epic {epic_id}", use_json=args.json)
            )

    # Compute effective values with source tracking
    def resolve_spec(task_key: str, epic_key: str) -> tuple:
        """Return (spec, source) tuple."""
        task_val = task_data.get(task_key)
        if task_val:
            return (task_val, "task")
        if epic_data:
            epic_val = epic_data.get(epic_key)
            if epic_val:
                return (epic_val, "epic")
        return (None, None)

    impl_spec, impl_source = resolve_spec("impl", "default_impl")
    review_spec, review_source = resolve_spec("review", "default_review")
    sync_spec, sync_source = resolve_spec("sync", "default_sync")

    if args.json:
        json_output(
            {
                "id": task_id,
                "epic": epic_id,
                "impl": {"spec": impl_spec, "source": impl_source},
                "review": {"spec": review_spec, "source": review_source},
                "sync": {"spec": sync_spec, "source": sync_source},
            }
        )
    else:
        def fmt(spec, source):
            if spec:
                return f"{spec} ({source})"
            return "null"

        print(f"impl: {fmt(impl_spec, impl_source)}")
        print(f"review: {fmt(review_spec, review_source)}")
        print(f"sync: {fmt(sync_spec, sync_source)}")


def cmd_task_set_description(args: argparse.Namespace) -> None:
    """Set task description section."""
    _task_set_section(args.id, "## Description", args.file, args.json)


def cmd_task_set_acceptance(args: argparse.Namespace) -> None:
    """Set task acceptance section."""
    _task_set_section(args.id, "## Acceptance", args.file, args.json)


def cmd_task_set_spec(args: argparse.Namespace) -> None:
    """Set task spec - full replacement (--file) or section patches.

    Full replacement mode: --file replaces entire spec content (like epic set-plan).
    Section patch mode: --description and/or --acceptance update specific sections.
    """
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    task_id = args.id
    if not is_task_id(task_id):
        error_exit(
            f"Invalid task ID: {task_id}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
            use_json=args.json,
        )

    # Need at least one of file, description, or acceptance
    has_file = hasattr(args, "file") and args.file
    if not has_file and not args.description and not args.acceptance:
        error_exit(
            "Requires --file, --description, or --acceptance",
            use_json=args.json,
        )

    flow_dir = get_flow_dir()
    task_json_path = flow_dir / TASKS_DIR / f"{task_id}.json"
    task_spec_path = flow_dir / TASKS_DIR / f"{task_id}.md"

    # Verify task exists
    if not task_json_path.exists():
        error_exit(f"Task {task_id} not found", use_json=args.json)

    # Load task JSON first (fail early)
    task_data = load_json_or_exit(task_json_path, f"Task {task_id}", use_json=args.json)

    # Full file replacement mode (like epic set-plan)
    if has_file:
        content = read_file_or_stdin(args.file, "Spec file", use_json=args.json)
        # Validate spec headings before writing: reject duplicates
        from flowctl.commands.admin import validate_task_spec_headings
        heading_errors = validate_task_spec_headings(content)
        # Only reject on duplicate headings, not missing ones
        dup_errors = [e for e in heading_errors if e.startswith("Duplicate")]
        if dup_errors:
            error_exit(
                f"Spec validation failed: {'; '.join(dup_errors)}",
                use_json=args.json,
            )
        atomic_write(task_spec_path, content)
        task_data["updated_at"] = now_iso()
        atomic_write_json(task_json_path, task_data)

        if args.json:
            json_output({"id": task_id, "message": f"Task {task_id} spec replaced"})
        else:
            print(f"Task {task_id} spec replaced")
        return

    # Section patch mode (existing behavior)
    # Read current spec
    current_spec = read_text_or_exit(
        task_spec_path, f"Task {task_id} spec", use_json=args.json
    )

    updated_spec = current_spec
    sections_updated = []

    # Apply description if provided
    if args.description:
        desc_content = read_file_or_stdin(args.description, "Description file", use_json=args.json)
        try:
            updated_spec = patch_task_section(updated_spec, "## Description", desc_content)
            sections_updated.append("## Description")
        except ValueError as e:
            error_exit(str(e), use_json=args.json)

    # Apply acceptance if provided
    if args.acceptance:
        acc_content = read_file_or_stdin(args.acceptance, "Acceptance file", use_json=args.json)
        try:
            updated_spec = patch_task_section(updated_spec, "## Acceptance", acc_content)
            sections_updated.append("## Acceptance")
        except ValueError as e:
            error_exit(str(e), use_json=args.json)

    # Validate final spec headings before writing: reject duplicates
    from flowctl.commands.admin import validate_task_spec_headings
    heading_errors = validate_task_spec_headings(updated_spec)
    # Only reject on duplicate headings, not missing ones
    dup_errors = [e for e in heading_errors if e.startswith("Duplicate")]
    if dup_errors:
        error_exit(
            f"Spec validation failed after patching: {'; '.join(dup_errors)}",
            use_json=args.json,
        )

    # Single atomic write for spec, single for JSON
    atomic_write(task_spec_path, updated_spec)
    task_data["updated_at"] = now_iso()
    atomic_write_json(task_json_path, task_data)

    if args.json:
        json_output(
            {
                "id": task_id,
                "sections": sections_updated,
                "message": f"Task {task_id} updated: {', '.join(sections_updated)}",
            }
        )
    else:
        print(f"Task {task_id} updated: {', '.join(sections_updated)}")


def cmd_task_reset(args: argparse.Namespace) -> None:
    """Reset task status to todo."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    task_id = args.task_id
    if not is_task_id(task_id):
        error_exit(
            f"Invalid task ID: {task_id}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
            use_json=args.json,
        )

    flow_dir = get_flow_dir()
    task_json_path = flow_dir / TASKS_DIR / f"{task_id}.json"

    if not task_json_path.exists():
        error_exit(f"Task {task_id} not found", use_json=args.json)

    # Load task with merged runtime state
    task_data = load_task_with_state(task_id, use_json=args.json)

    # Load epic to check if closed
    epic_id = epic_id_from_task(task_id)
    epic_path = flow_dir / EPICS_DIR / f"{epic_id}.json"
    if epic_path.exists():
        epic_data = load_json_or_exit(epic_path, f"Epic {epic_id}", use_json=args.json)
        if epic_data.get("status") == "done":
            error_exit(
                f"Cannot reset task in closed epic {epic_id}", use_json=args.json
            )

    # Check status validations (use merged state)
    current_status = task_data.get("status", "todo")
    if current_status == "in_progress":
        error_exit(
            f"Cannot reset in_progress task {task_id}. Complete or block it first.",
            use_json=args.json,
        )
    if current_status == "todo":
        # Already todo - no-op success
        if args.json:
            json_output(
                {"success": True, "reset": [], "message": f"{task_id} already todo"}
            )
        else:
            print(f"{task_id} already todo")
        return

    # Reset runtime state to baseline (overwrite, not merge - clears all runtime fields)
    reset_task_runtime(task_id)

    # Also clear legacy runtime fields from definition file (for backward compat cleanup)
    def_data = load_json_or_exit(task_json_path, f"Task {task_id}", use_json=args.json)
    def_data.pop("blocked_reason", None)
    def_data.pop("completed_at", None)
    def_data.pop("assignee", None)
    def_data.pop("claimed_at", None)
    def_data.pop("claim_note", None)
    def_data.pop("evidence", None)
    def_data["status"] = "todo"  # Keep in sync for backward compat
    def_data["updated_at"] = now_iso()
    atomic_write_json(task_json_path, def_data)

    # Clear evidence section from spec markdown
    clear_task_evidence(task_id)

    reset_ids = [task_id]

    # Handle cascade
    if args.cascade:
        dependents = find_dependents(task_id, same_epic=True)
        for dep_id in dependents:
            dep_path = flow_dir / TASKS_DIR / f"{dep_id}.json"
            if not dep_path.exists():
                continue

            # Load merged state for dependent
            dep_data = load_task_with_state(dep_id, use_json=args.json)
            dep_status = dep_data.get("status", "todo")

            # Skip in_progress and already todo
            if dep_status == "in_progress" or dep_status == "todo":
                continue

            # Reset runtime state for dependent (overwrite, not merge)
            reset_task_runtime(dep_id)

            # Also clear legacy fields from definition
            dep_def = load_json(dep_path)
            dep_def.pop("blocked_reason", None)
            dep_def.pop("completed_at", None)
            dep_def.pop("assignee", None)
            dep_def.pop("claimed_at", None)
            dep_def.pop("claim_note", None)
            dep_def.pop("evidence", None)
            dep_def["status"] = "todo"
            dep_def["updated_at"] = now_iso()
            atomic_write_json(dep_path, dep_def)

            clear_task_evidence(dep_id)
            reset_ids.append(dep_id)

    if args.json:
        json_output({"success": True, "reset": reset_ids})
    else:
        print(f"Reset: {', '.join(reset_ids)}")


# ---------------------------------------------------------------------------
# Runtime DAG mutation commands
# ---------------------------------------------------------------------------


def cmd_dep_rm(args: argparse.Namespace) -> None:
    """Remove a dependency from a task."""
    if not ensure_flow_exists():
        error_exit(".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json)

    if not is_task_id(args.task):
        error_exit(f"Invalid task ID: {args.task}", use_json=args.json)
    if not is_task_id(args.depends_on):
        error_exit(f"Invalid dependency ID: {args.depends_on}", use_json=args.json)

    flow_dir = get_flow_dir()
    task_path = flow_dir / TASKS_DIR / f"{args.task}.json"
    task_data = load_json_or_exit(task_path, f"Task {args.task}", use_json=args.json)

    if "depends_on" not in task_data:
        task_data["depends_on"] = task_data.pop("deps", [])

    if args.depends_on not in task_data["depends_on"]:
        if args.json:
            json_output({"task": args.task, "depends_on": task_data["depends_on"],
                         "removed": False, "message": f"{args.depends_on} not in dependencies"})
        else:
            print(f"{args.depends_on} is not a dependency of {args.task}")
        return

    task_data["depends_on"].remove(args.depends_on)
    task_data["updated_at"] = now_iso()
    atomic_write_json(task_path, task_data)

    if args.json:
        json_output({"task": args.task, "depends_on": task_data["depends_on"],
                     "removed": True, "message": f"Dependency {args.depends_on} removed from {args.task}"})
    else:
        print(f"Dependency {args.depends_on} removed from {args.task}")


def cmd_task_skip(args: argparse.Namespace) -> None:
    """Skip a task (mark as permanently skipped without deleting). Downstream deps treat skipped as done."""
    if not ensure_flow_exists():
        error_exit(".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json)

    task_id = args.task_id
    if not is_task_id(task_id):
        error_exit(f"Invalid task ID: {task_id}", use_json=args.json)

    flow_dir = get_flow_dir()
    task_path = flow_dir / TASKS_DIR / f"{task_id}.json"
    if not task_path.exists():
        error_exit(f"Task {task_id} not found", use_json=args.json)

    task_data = load_task_with_state(task_id, use_json=args.json)
    status = task_data.get("status", "todo")

    if status == "done":
        error_exit(f"Cannot skip already-done task {task_id}", use_json=args.json)

    # Update definition
    def_data = load_json_or_exit(task_path, f"Task {task_id}", use_json=args.json)
    def_data["status"] = "skipped"
    def_data["skipped_reason"] = args.reason or ""
    def_data["skipped_at"] = now_iso()
    def_data["updated_at"] = now_iso()
    atomic_write_json(task_path, def_data)

    # Update runtime state
    from flowctl.core.state import save_task_runtime
    save_task_runtime(task_id, {"status": "skipped", "skipped_reason": args.reason or ""})

    if args.json:
        json_output({"success": True, "id": task_id, "status": "skipped",
                     "reason": args.reason or "",
                     "message": f"Task {task_id} skipped"})
    else:
        print(f"Task {task_id} skipped" + (f": {args.reason}" if args.reason else ""))


def cmd_task_split(args: argparse.Namespace) -> None:
    """Split a task into N sub-tasks. Original task becomes a meta-task depending on all sub-tasks."""
    if not ensure_flow_exists():
        error_exit(".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json)

    task_id = args.task_id
    if not is_task_id(task_id):
        error_exit(f"Invalid task ID: {task_id}", use_json=args.json)

    flow_dir = get_flow_dir()
    task_path = flow_dir / TASKS_DIR / f"{task_id}.json"
    if not task_path.exists():
        error_exit(f"Task {task_id} not found", use_json=args.json)

    task_data = load_task_with_state(task_id, use_json=args.json)
    status = task_data.get("status", "todo")

    if status in ("done", "skipped"):
        error_exit(f"Cannot split task {task_id} with status '{status}'", use_json=args.json)

    epic_id = epic_id_from_task(task_id)
    titles = [t.strip() for t in args.titles.split("|") if t.strip()]
    if len(titles) < 2:
        error_exit("Need at least 2 sub-task titles separated by '|'", use_json=args.json)

    # Find next available task number
    max_task = scan_max_task_id(flow_dir, epic_id)
    created = []

    # Original task's dependencies become the first sub-task's dependencies
    original_deps = task_data.get("depends_on", [])

    for i, title in enumerate(titles):
        sub_num = max_task + 1 + i
        sub_id = f"{epic_id}.{sub_num}"

        # First sub-task inherits original deps; subsequent depend on previous
        if i == 0:
            sub_deps = original_deps
        else:
            prev_id = f"{epic_id}.{max_task + i}"
            sub_deps = [prev_id] if args.chain else []

        sub_data = {
            "id": sub_id,
            "epic": epic_id,
            "title": title,
            "status": "todo",
            "priority": task_data.get("priority"),
            "depends_on": sub_deps,
            "domain": task_data.get("domain"),
            "files": [],
            "assignee": None,
            "claimed_at": None,
            "claim_note": "",
            "split_from": task_id,
            "spec_path": f"{FLOW_DIR}/{TASKS_DIR}/{sub_id}.md",
            "created_at": now_iso(),
            "updated_at": now_iso(),
        }
        atomic_write_json(flow_dir / TASKS_DIR / f"{sub_id}.json", sub_data)

        spec_content = create_task_spec(sub_id, title, None)
        atomic_write(flow_dir / TASKS_DIR / f"{sub_id}.md", spec_content)
        created.append(sub_id)

    # Mark original task as skipped with split reference
    def_data = load_json_or_exit(task_path, f"Task {task_id}", use_json=args.json)
    def_data["status"] = "skipped"
    def_data["skipped_reason"] = f"Split into: {', '.join(created)}"
    def_data["split_into"] = created
    def_data["updated_at"] = now_iso()
    atomic_write_json(task_path, def_data)

    from flowctl.core.state import save_task_runtime
    save_task_runtime(task_id, {"status": "skipped", "skipped_reason": f"Split into: {', '.join(created)}"})

    # Update any tasks that depended on the original to depend on the LAST sub-task
    last_sub = created[-1]
    tasks_dir = flow_dir / TASKS_DIR
    for other_file in sorted(tasks_dir.glob(f"{epic_id}.*.json")):
        other_id = other_file.stem
        if other_id == task_id or other_id in created:
            continue
        other_data = load_json(other_file)
        if not other_data:
            continue
        deps = other_data.get("depends_on", other_data.get("deps", []))
        if task_id in deps:
            deps = [last_sub if d == task_id else d for d in deps]
            other_data["depends_on"] = deps
            other_data["updated_at"] = now_iso()
            atomic_write_json(other_file, other_data)

    if args.json:
        json_output({
            "success": True,
            "original": task_id,
            "split_into": created,
            "chain": args.chain,
            "message": f"Task {task_id} split into {len(created)} sub-tasks",
        })
    else:
        print(f"Task {task_id} split into:")
        for sub_id in created:
            print(f"  {sub_id}")
        print(f"Original task marked as skipped. Downstream deps updated to {last_sub}.")
