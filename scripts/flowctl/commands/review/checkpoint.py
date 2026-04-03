"""Checkpoint commands: save, restore, delete for epic state recovery."""

import argparse
import json
from pathlib import Path

from flowctl.core.constants import EPICS_DIR, SPECS_DIR, TASKS_DIR
from flowctl.core.ids import is_epic_id, is_task_id
from flowctl.core.io import (
    atomic_write,
    atomic_write_json,
    error_exit,
    json_output,
    load_json,
    load_json_or_exit,
    now_iso,
)
from flowctl.core.paths import ensure_flow_exists, get_flow_dir
from flowctl.core.state import delete_task_runtime, get_state_store

def cmd_checkpoint_save(args: argparse.Namespace) -> None:
    """Save full epic + tasks state to checkpoint file.

    Creates .flow/.checkpoint-fn-N.json with complete state snapshot.
    Use before plan-review or other long operations to enable recovery
    if context compaction occurs.
    """
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    epic_id = args.epic
    if not is_epic_id(epic_id):
        error_exit(
            f"Invalid epic ID: {epic_id}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)",
            use_json=args.json,
        )

    flow_dir = get_flow_dir()
    epic_path = flow_dir / EPICS_DIR / f"{epic_id}.json"
    spec_path = flow_dir / SPECS_DIR / f"{epic_id}.md"

    if not epic_path.exists():
        error_exit(f"Epic {epic_id} not found", use_json=args.json)

    # Load epic data
    epic_data = load_json_or_exit(epic_path, f"Epic {epic_id}", use_json=args.json)

    # Load epic spec
    epic_spec = ""
    if spec_path.exists():
        epic_spec = spec_path.read_text(encoding="utf-8")

    # Load all tasks for this epic (including runtime state)
    tasks_dir = flow_dir / TASKS_DIR
    store = get_state_store()
    tasks = []
    if tasks_dir.exists():
        for task_file in sorted(tasks_dir.glob(f"{epic_id}.*.json")):
            task_id = task_file.stem
            if not is_task_id(task_id):
                continue  # Skip non-task files (e.g., fn-1.2-review.json)
            task_data = load_json(task_file)
            task_spec_path = tasks_dir / f"{task_id}.md"
            task_spec = ""
            if task_spec_path.exists():
                task_spec = task_spec_path.read_text(encoding="utf-8")
            # Include runtime state in checkpoint
            runtime_state = store.load_runtime(task_id)
            tasks.append({
                "id": task_id,
                "data": task_data,
                "spec": task_spec,
                "runtime": runtime_state,  # May be None if no state file
            })

    # Build checkpoint
    checkpoint = {
        "schema_version": 2,  # Bumped for runtime state support
        "created_at": now_iso(),
        "epic_id": epic_id,
        "epic": {
            "data": epic_data,
            "spec": epic_spec,
        },
        "tasks": tasks,
    }

    # Write checkpoint
    checkpoint_path = flow_dir / f".checkpoint-{epic_id}.json"
    atomic_write_json(checkpoint_path, checkpoint)

    if args.json:
        json_output({
            "epic_id": epic_id,
            "checkpoint_path": str(checkpoint_path),
            "task_count": len(tasks),
            "message": f"Checkpoint saved: {checkpoint_path}",
        })
    else:
        print(f"Checkpoint saved: {checkpoint_path} ({len(tasks)} tasks)")


def cmd_checkpoint_restore(args: argparse.Namespace) -> None:
    """Restore epic + tasks state from checkpoint file.

    Reads .flow/.checkpoint-fn-N.json and overwrites current state.
    Use to recover after context compaction or to rollback changes.
    """
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    epic_id = args.epic
    if not is_epic_id(epic_id):
        error_exit(
            f"Invalid epic ID: {epic_id}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)",
            use_json=args.json,
        )

    flow_dir = get_flow_dir()
    checkpoint_path = flow_dir / f".checkpoint-{epic_id}.json"

    if not checkpoint_path.exists():
        error_exit(f"No checkpoint found for {epic_id}", use_json=args.json)

    # Load checkpoint
    checkpoint = load_json_or_exit(
        checkpoint_path, f"Checkpoint {epic_id}", use_json=args.json
    )

    # Validate checkpoint structure
    if "epic" not in checkpoint or "tasks" not in checkpoint:
        error_exit("Invalid checkpoint format", use_json=args.json)

    # Restore epic
    epic_path = flow_dir / EPICS_DIR / f"{epic_id}.json"
    spec_path = flow_dir / SPECS_DIR / f"{epic_id}.md"

    epic_data = checkpoint["epic"]["data"]
    epic_data["updated_at"] = now_iso()
    atomic_write_json(epic_path, epic_data)

    if checkpoint["epic"]["spec"]:
        atomic_write(spec_path, checkpoint["epic"]["spec"])

    # Restore tasks (including runtime state)
    tasks_dir = flow_dir / TASKS_DIR
    store = get_state_store()
    restored_tasks = []
    for task in checkpoint["tasks"]:
        task_id = task["id"]
        task_json_path = tasks_dir / f"{task_id}.json"
        task_spec_path = tasks_dir / f"{task_id}.md"

        task_data = task["data"]
        task_data["updated_at"] = now_iso()
        atomic_write_json(task_json_path, task_data)

        if task["spec"]:
            atomic_write(task_spec_path, task["spec"])

        # Restore runtime state from checkpoint (schema_version >= 2)
        runtime = task.get("runtime")
        if runtime is not None:
            # Restore saved runtime state
            with store.lock_task(task_id):
                store.save_runtime(task_id, runtime)
        else:
            # No runtime in checkpoint - delete any existing runtime state
            delete_task_runtime(task_id)

        restored_tasks.append(task_id)

    if args.json:
        json_output({
            "epic_id": epic_id,
            "checkpoint_created_at": checkpoint.get("created_at"),
            "tasks_restored": restored_tasks,
            "message": f"Restored {epic_id} from checkpoint ({len(restored_tasks)} tasks)",
        })
    else:
        print(f"Restored {epic_id} from checkpoint ({len(restored_tasks)} tasks)")
        print(f"Checkpoint was created at: {checkpoint.get('created_at', 'unknown')}")


def cmd_checkpoint_delete(args: argparse.Namespace) -> None:
    """Delete checkpoint file for an epic."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    epic_id = args.epic
    if not is_epic_id(epic_id):
        error_exit(
            f"Invalid epic ID: {epic_id}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)",
            use_json=args.json,
        )

    flow_dir = get_flow_dir()
    checkpoint_path = flow_dir / f".checkpoint-{epic_id}.json"

    if not checkpoint_path.exists():
        if args.json:
            json_output({
                "epic_id": epic_id,
                "deleted": False,
                "message": f"No checkpoint found for {epic_id}",
            })
        else:
            print(f"No checkpoint found for {epic_id}")
        return

    checkpoint_path.unlink()

    if args.json:
        json_output({
            "epic_id": epic_id,
            "deleted": True,
            "message": f"Deleted checkpoint for {epic_id}",
        })
    else:
        print(f"Deleted checkpoint for {epic_id}")


# ---------------------------------------------------------------------------
# Adversarial review via Codex
