"""Gap registry commands: add, list, resolve, check."""

import argparse
import hashlib
import sys

from flowctl.core.constants import EPICS_DIR
from flowctl.core.ids import is_epic_id, normalize_epic
from flowctl.core.io import (
    atomic_write_json,
    error_exit,
    json_output,
    load_json_or_exit,
    now_iso,
)
from flowctl.core.paths import ensure_flow_exists, get_flow_dir


GAP_PRIORITIES = ("required", "important", "nice-to-have")
GAP_BLOCKING_PRIORITIES = ("required", "important")


def _gap_id(epic_id: str, capability: str) -> str:
    """Compute deterministic gap ID from epic + capability (content-hash)."""
    key = f"{epic_id}:{capability.strip().lower()}"
    return "gap-" + hashlib.sha256(key.encode()).hexdigest()[:8]


def _load_epic_for_gap(epic_id: str, use_json: bool) -> tuple:
    """Load and normalize epic, return (flow_dir, epic_path, epic_data)."""
    flow_dir = get_flow_dir()
    epic_path = flow_dir / EPICS_DIR / f"{epic_id}.json"
    epic_data = normalize_epic(
        load_json_or_exit(epic_path, f"Epic {epic_id}", use_json=use_json)
    )
    return flow_dir, epic_path, epic_data


def cmd_gap_add(args: argparse.Namespace) -> None:
    """Register a requirement gap on an epic (idempotent)."""
    if not ensure_flow_exists():
        error_exit(".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json)
    if not is_epic_id(args.epic):
        error_exit(f"Invalid epic ID: {args.epic}", use_json=args.json)

    _, epic_path, epic_data = _load_epic_for_gap(args.epic, args.json)

    gap_id = _gap_id(args.epic, args.capability)
    existing = next((g for g in epic_data["gaps"] if g["id"] == gap_id), None)

    if existing:
        if args.json:
            json_output({"id": gap_id, "created": False, "gap": existing,
                         "message": f"Gap already exists: {gap_id}"})
        else:
            print(f"Gap already exists: {gap_id} — {existing['capability']}")
        return

    gap = {
        "id": gap_id,
        "capability": args.capability.strip(),
        "priority": args.priority,
        "status": "open",
        "source": args.source,
        "task": getattr(args, "task", None),
        "added_at": now_iso(),
        "resolved_at": None,
        "evidence": None,
    }
    epic_data["gaps"].append(gap)
    epic_data["updated_at"] = now_iso()
    atomic_write_json(epic_path, epic_data)

    if args.json:
        json_output({"id": gap_id, "created": True, "gap": gap,
                     "message": f"Gap {gap_id} added to {args.epic}"})
    else:
        print(f"Gap {gap_id} added: [{args.priority}] {args.capability}")


def cmd_gap_list(args: argparse.Namespace) -> None:
    """List gaps for an epic, with optional status filter."""
    if not ensure_flow_exists():
        error_exit(".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json)
    if not is_epic_id(args.epic):
        error_exit(f"Invalid epic ID: {args.epic}", use_json=args.json)

    _, _, epic_data = _load_epic_for_gap(args.epic, args.json)

    gaps = epic_data["gaps"]
    if args.status:
        gaps = [g for g in gaps if g["status"] == args.status]

    if args.json:
        json_output({"epic": args.epic, "count": len(gaps), "gaps": gaps})
    else:
        if not gaps:
            print(f"No gaps for {args.epic}" + (f" (status={args.status})" if args.status else ""))
            return
        for g in gaps:
            marker = "\u2713" if g["status"] == "resolved" else "\u2717"
            print(f"  {marker} {g['id']} [{g['priority']}] {g['capability']}")


def cmd_gap_resolve(args: argparse.Namespace) -> None:
    """Mark a gap as resolved with evidence (idempotent)."""
    if not ensure_flow_exists():
        error_exit(".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json)
    if not is_epic_id(args.epic):
        error_exit(f"Invalid epic ID: {args.epic}", use_json=args.json)

    _, epic_path, epic_data = _load_epic_for_gap(args.epic, args.json)

    gap_id = _gap_id(args.epic, args.capability)
    gap = next((g for g in epic_data["gaps"] if g["id"] == gap_id), None)

    if not gap:
        error_exit(f"Gap not found: capability '{args.capability}' (computed id: {gap_id})", use_json=args.json)

    if gap["status"] == "resolved":
        if args.json:
            json_output({"id": gap_id, "changed": False, "gap": gap,
                         "message": f"Gap {gap_id} already resolved"})
        else:
            print(f"Gap {gap_id} already resolved")
        return

    gap["status"] = "resolved"
    gap["resolved_at"] = now_iso()
    gap["evidence"] = args.evidence
    epic_data["updated_at"] = now_iso()
    atomic_write_json(epic_path, epic_data)

    if args.json:
        json_output({"id": gap_id, "changed": True, "gap": gap,
                     "message": f"Gap {gap_id} resolved"})
    else:
        print(f"Gap {gap_id} resolved: {args.evidence}")


def cmd_gap_check(args: argparse.Namespace) -> None:
    """Gate check: fail if unresolved required/important gaps exist."""
    if not ensure_flow_exists():
        error_exit(".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json)
    if not is_epic_id(args.epic):
        error_exit(f"Invalid epic ID: {args.epic}", use_json=args.json)

    _, _, epic_data = _load_epic_for_gap(args.epic, args.json)

    gaps = epic_data["gaps"]
    open_blocking = [g for g in gaps if g["status"] == "open" and g.get("priority") in GAP_BLOCKING_PRIORITIES]
    open_non_blocking = [g for g in gaps if g["status"] == "open" and g.get("priority") not in GAP_BLOCKING_PRIORITIES]
    resolved = [g for g in gaps if g["status"] == "resolved"]

    gate = "fail" if open_blocking else "pass"

    if args.json:
        json_output({
            "epic": args.epic,
            "gate": gate,
            "total": len(gaps),
            "open_blocking": open_blocking,
            "open_non_blocking": open_non_blocking,
            "resolved": resolved,
        })
    else:
        if gate == "pass":
            print(f"Gap check PASS for {args.epic} ({len(resolved)} resolved, {len(open_non_blocking)} non-blocking)")
        else:
            print(f"Gap check FAIL for {args.epic} — {len(open_blocking)} blocking gap(s):")
            for g in open_blocking:
                print(f"  \u2717 [{g['priority']}] {g['capability']}")

    if gate == "fail":
        sys.exit(1)
