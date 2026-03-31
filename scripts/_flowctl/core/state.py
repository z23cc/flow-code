"""State management: StateStore, task state operations."""

import json
from abc import ABC, abstractmethod
from contextlib import contextmanager
from pathlib import Path
from typing import ContextManager, Optional

from _flowctl.compat import _flock, LOCK_EX, LOCK_UN
from _flowctl.core.constants import RUNTIME_FIELDS, TASKS_DIR
from _flowctl.core.ids import normalize_task
from _flowctl.core.io import (
    atomic_write,
    atomic_write_json,
    load_json_or_exit,
    now_iso,
)
from _flowctl.core.paths import get_flow_dir, get_state_dir


# --- StateStore (runtime task state) ---


class StateStore(ABC):
    """Abstract interface for runtime task state storage."""

    @abstractmethod
    def load_runtime(self, task_id: str) -> Optional[dict]:
        """Load runtime state for a task. Returns None if no state file."""
        ...

    @abstractmethod
    def save_runtime(self, task_id: str, data: dict) -> None:
        """Save runtime state for a task."""
        ...

    @abstractmethod
    def lock_task(self, task_id: str) -> ContextManager:
        """Context manager for exclusive task lock."""
        ...

    @abstractmethod
    def list_runtime_files(self) -> list[str]:
        """List all task IDs that have runtime state files."""
        ...


class LocalFileStateStore(StateStore):
    """File-based state store with fcntl locking."""

    def __init__(self, state_dir: Path):
        self.state_dir = state_dir
        self.tasks_dir = state_dir / "tasks"
        self.locks_dir = state_dir / "locks"

    def _state_path(self, task_id: str) -> Path:
        return self.tasks_dir / f"{task_id}.state.json"

    def _lock_path(self, task_id: str) -> Path:
        return self.locks_dir / f"{task_id}.lock"

    def load_runtime(self, task_id: str) -> Optional[dict]:
        state_path = self._state_path(task_id)
        if not state_path.exists():
            return None
        try:
            with open(state_path, encoding="utf-8") as f:
                return json.load(f)
        except (json.JSONDecodeError, IOError):
            return None

    def save_runtime(self, task_id: str, data: dict) -> None:
        self.tasks_dir.mkdir(parents=True, exist_ok=True)
        state_path = self._state_path(task_id)
        content = json.dumps(data, indent=2, sort_keys=True) + "\n"
        atomic_write(state_path, content)

    @contextmanager
    def lock_task(self, task_id: str):
        """Acquire exclusive lock for task operations."""
        self.locks_dir.mkdir(parents=True, exist_ok=True)
        lock_path = self._lock_path(task_id)
        with open(lock_path, "w") as f:
            try:
                _flock(f, LOCK_EX)
                yield
            finally:
                _flock(f, LOCK_UN)

    def list_runtime_files(self) -> list[str]:
        if not self.tasks_dir.exists():
            return []
        return [
            f.stem.replace(".state", "")
            for f in self.tasks_dir.glob("*.state.json")
        ]


def get_state_store() -> LocalFileStateStore:
    """Get the state store instance."""
    return LocalFileStateStore(get_state_dir())


# --- Task Loading with State Merge ---


def load_task_definition(task_id: str, use_json: bool = True) -> dict:
    """Load task definition from tracked file (no runtime state)."""
    flow_dir = get_flow_dir()
    def_path = flow_dir / TASKS_DIR / f"{task_id}.json"
    return load_json_or_exit(def_path, f"Task {task_id}", use_json=use_json)


def load_task_with_state(task_id: str, use_json: bool = True) -> dict:
    """Load task definition merged with runtime state.

    Backward compatible: if no state file exists, reads legacy runtime
    fields from definition file.
    """
    definition = load_task_definition(task_id, use_json=use_json)

    # Load runtime state
    store = get_state_store()
    runtime = store.load_runtime(task_id)

    if runtime is None:
        # Backward compat: extract runtime fields from definition
        runtime = {k: definition[k] for k in RUNTIME_FIELDS if k in definition}
        if not runtime:
            runtime = {"status": "todo"}

    # Merge: runtime overwrites definition for runtime fields
    merged = {**definition, **runtime}
    return normalize_task(merged)


def save_task_runtime(task_id: str, updates: dict) -> None:
    """Write runtime state only (merge with existing). Never touch definition file."""
    store = get_state_store()
    with store.lock_task(task_id):
        current = store.load_runtime(task_id) or {"status": "todo"}
        merged = {**current, **updates, "updated_at": now_iso()}
        store.save_runtime(task_id, merged)


def reset_task_runtime(task_id: str) -> None:
    """Reset runtime state to baseline (overwrite, not merge). Used by task reset."""
    store = get_state_store()
    with store.lock_task(task_id):
        # Overwrite with clean baseline state
        store.save_runtime(task_id, {"status": "todo", "updated_at": now_iso()})


def delete_task_runtime(task_id: str) -> None:
    """Delete runtime state file entirely. Used by checkpoint restore when no runtime."""
    store = get_state_store()
    with store.lock_task(task_id):
        state_path = store._state_path(task_id)
        if state_path.exists():
            state_path.unlink()


def save_task_definition(task_id: str, definition: dict) -> None:
    """Write definition to tracked file (filters out runtime fields)."""
    flow_dir = get_flow_dir()
    def_path = flow_dir / TASKS_DIR / f"{task_id}.json"
    # Filter out runtime fields
    clean_def = {k: v for k, v in definition.items() if k not in RUNTIME_FIELDS}
    atomic_write_json(def_path, clean_def)
