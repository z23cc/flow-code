"""ID parsing, generation, and validation utilities."""

import re
import secrets
import string
import unicodedata
from typing import Optional


def generate_epic_suffix(length: int = 3) -> str:
    """Generate random alphanumeric suffix for epic IDs (a-z0-9)."""
    alphabet = string.ascii_lowercase + string.digits
    return "".join(secrets.choice(alphabet) for _ in range(length))


def slugify(text: str, max_length: int = 40) -> Optional[str]:
    """Convert text to URL-safe slug for epic IDs.

    Uses Django pattern (stdlib only): normalize unicode, strip non-alphanumeric,
    collapse whitespace/hyphens. Returns None if result is empty (for fallback).

    Output contains only [a-z0-9-] to match parse_id() regex.

    Args:
        text: Input text to slugify
        max_length: Maximum length (40 default, leaves room for fn-XXX- prefix)

    Returns:
        Slugified string or None if empty
    """
    text = str(text)
    # Normalize unicode and convert to ASCII
    text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode("ascii")
    # Remove non-word chars (except spaces and hyphens), lowercase
    text = re.sub(r"[^\w\s-]", "", text.lower())
    # Convert underscores to spaces (will be collapsed to hyphens)
    text = text.replace("_", " ")
    # Collapse whitespace and hyphens to single hyphen, strip leading/trailing
    text = re.sub(r"[-\s]+", "-", text).strip("-")
    # Truncate at word boundary if too long
    if max_length and len(text) > max_length:
        truncated = text[:max_length]
        if "-" in truncated:
            truncated = truncated.rsplit("-", 1)[0]
        text = truncated.strip("-")
    return text if text else None


def parse_id(id_str: str) -> tuple[Optional[int], Optional[int]]:
    """Parse ID into (epic_num, task_num). Returns (epic, None) for epic IDs.

    Supports formats:
    - Legacy: fn-N, fn-N.M
    - Short suffix: fn-N-xxx, fn-N-xxx.M (3-char random)
    - Slug suffix: fn-N-longer-slug, fn-N-longer-slug.M (slugified title)
    """
    # Pattern supports: fn-N, fn-N-x (1-3 char), fn-N-xx-yy (multi-segment slug)
    match = re.match(
        r"^fn-(\d+)(?:-[a-z0-9][a-z0-9-]*[a-z0-9]|-[a-z0-9]{1,3})?(?:\.(\d+))?$",
        id_str,
    )
    if not match:
        return None, None
    epic = int(match.group(1))
    task = int(match.group(2)) if match.group(2) else None
    return epic, task


def normalize_epic(epic_data: dict) -> dict:
    """Apply defaults for optional epic fields."""
    if "plan_review_status" not in epic_data:
        epic_data["plan_review_status"] = "unknown"
    if "plan_reviewed_at" not in epic_data:
        epic_data["plan_reviewed_at"] = None
    if "completion_review_status" not in epic_data:
        epic_data["completion_review_status"] = "unknown"
    if "completion_reviewed_at" not in epic_data:
        epic_data["completion_reviewed_at"] = None
    if "branch_name" not in epic_data:
        epic_data["branch_name"] = None
    if "depends_on_epics" not in epic_data:
        epic_data["depends_on_epics"] = []
    # Backend spec defaults (for orchestration products like flow-swarm)
    if "default_impl" not in epic_data:
        epic_data["default_impl"] = None
    if "default_review" not in epic_data:
        epic_data["default_review"] = None
    if "default_sync" not in epic_data:
        epic_data["default_sync"] = None
    if "gaps" not in epic_data:
        epic_data["gaps"] = []
    return epic_data


def normalize_task(task_data: dict) -> dict:
    """Apply defaults for optional task fields and migrate legacy keys."""
    if "priority" not in task_data:
        task_data["priority"] = None
    # Migrate legacy 'deps' key to 'depends_on'
    if "depends_on" not in task_data:
        task_data["depends_on"] = task_data.get("deps", [])
    # Backend spec defaults (for orchestration products like flow-swarm)
    if "impl" not in task_data:
        task_data["impl"] = None
    if "review" not in task_data:
        task_data["review"] = None
    if "sync" not in task_data:
        task_data["sync"] = None
    return task_data


def task_priority(task_data: dict) -> int:
    """Priority for sorting (None -> 999)."""
    try:
        if task_data.get("priority") is None:
            return 999
        return int(task_data.get("priority"))
    except Exception:
        return 999


def is_epic_id(id_str: str) -> bool:
    """Check if ID is an epic ID (fn-N)."""
    epic, task = parse_id(id_str)
    return epic is not None and task is None


def is_task_id(id_str: str) -> bool:
    """Check if ID is a task ID (fn-N.M)."""
    epic, task = parse_id(id_str)
    return epic is not None and task is not None


def epic_id_from_task(task_id: str) -> str:
    """Extract epic ID from task ID. Raises ValueError if invalid.

    Preserves suffix: fn-5-x7k.3 -> fn-5-x7k
    """
    epic, task = parse_id(task_id)
    if epic is None or task is None:
        raise ValueError(f"Invalid task ID: {task_id}")
    # Split on '.' and take epic part (preserves suffix if present)
    return task_id.rsplit(".", 1)[0]
