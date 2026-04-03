"""Tests for flowctl.core.state — StateStore and task loading with state merge."""

import json
import os

import pytest

from flowctl.core.state import LocalFileStateStore, load_task_with_state


class TestLocalFileStateStore:
    """Tests for LocalFileStateStore — file-based state with fcntl locking."""

    def test_save_and_load_runtime(self, tmp_path):
        """save_runtime creates file, load_runtime reads it back."""
        store = LocalFileStateStore(tmp_path)
        store.save_runtime("fn-1.1", {"status": "in_progress", "assignee": "alice"})

        result = store.load_runtime("fn-1.1")
        assert result is not None
        assert result["status"] == "in_progress"
        assert result["assignee"] == "alice"

    def test_load_missing_returns_none(self, tmp_path):
        """load_runtime returns None for non-existent task."""
        store = LocalFileStateStore(tmp_path)
        assert store.load_runtime("fn-999.1") is None

    def test_load_corrupt_returns_none(self, tmp_path):
        """load_runtime returns None for corrupt JSON."""
        store = LocalFileStateStore(tmp_path)
        tasks_dir = tmp_path / "tasks"
        tasks_dir.mkdir(parents=True)
        corrupt_path = tasks_dir / "fn-1.1.state.json"
        corrupt_path.write_text("not valid json{{{", encoding="utf-8")

        assert store.load_runtime("fn-1.1") is None

    def test_save_overwrites(self, tmp_path):
        """save_runtime overwrites existing state."""
        store = LocalFileStateStore(tmp_path)
        store.save_runtime("fn-1.1", {"status": "todo"})
        store.save_runtime("fn-1.1", {"status": "done"})

        result = store.load_runtime("fn-1.1")
        assert result["status"] == "done"

    def test_list_runtime_files_empty(self, tmp_path):
        """list_runtime_files returns empty list when no state files."""
        store = LocalFileStateStore(tmp_path)
        assert store.list_runtime_files() == []

    def test_list_runtime_files(self, tmp_path):
        """list_runtime_files returns task IDs that have state."""
        store = LocalFileStateStore(tmp_path)
        store.save_runtime("fn-1.1", {"status": "todo"})
        store.save_runtime("fn-1.2", {"status": "done"})

        ids = store.list_runtime_files()
        assert sorted(ids) == ["fn-1.1", "fn-1.2"]

    def test_lock_task_context_manager(self, tmp_path):
        """lock_task should work as a context manager without errors."""
        store = LocalFileStateStore(tmp_path)
        with store.lock_task("fn-1.1"):
            # Should be able to do operations inside the lock
            store.save_runtime("fn-1.1", {"status": "in_progress"})

        result = store.load_runtime("fn-1.1")
        assert result["status"] == "in_progress"

    def test_state_path(self, tmp_path):
        """Internal _state_path should use correct naming convention."""
        store = LocalFileStateStore(tmp_path)
        path = store._state_path("fn-1.1")
        assert path.name == "fn-1.1.state.json"
        assert path.parent.name == "tasks"


class TestLoadTaskWithState:
    """Tests for load_task_with_state — merges definition with runtime state."""

    def _setup_task(self, git_repo, task_id, definition, runtime=None):
        """Helper to create task definition and optional runtime state."""
        flow_dir = git_repo / ".flow"
        tasks_dir = flow_dir / "tasks"
        tasks_dir.mkdir(parents=True, exist_ok=True)

        # Write definition file
        def_path = tasks_dir / f"{task_id}.json"
        def_path.write_text(
            json.dumps(definition, indent=2) + "\n", encoding="utf-8"
        )

        if runtime is not None:
            # Write runtime state to git state dir
            # Use FLOW_STATE_DIR env var to point to a known location
            state_dir = git_repo / ".git" / "flow-state"
            state_tasks = state_dir / "tasks"
            state_tasks.mkdir(parents=True, exist_ok=True)
            state_path = state_tasks / f"{task_id}.state.json"
            state_path.write_text(
                json.dumps(runtime, indent=2) + "\n", encoding="utf-8"
            )
            os.environ["FLOW_STATE_DIR"] = str(state_dir)

    def test_merges_correctly(self, git_repo, monkeypatch):
        """Runtime state should override definition for runtime fields."""
        definition = {
            "id": "fn-1.1",
            "title": "Test task",
            "epic": "fn-1",
            "status": "todo",
        }
        runtime = {"status": "in_progress", "assignee": "bob"}
        self._setup_task(git_repo, "fn-1.1", definition, runtime)

        result = load_task_with_state("fn-1.1")
        assert result["status"] == "in_progress"
        assert result["assignee"] == "bob"
        assert result["title"] == "Test task"

    def test_handles_missing_state(self, git_repo, monkeypatch):
        """When no runtime state exists, should use definition fields or defaults."""
        definition = {
            "id": "fn-1.1",
            "title": "Test task",
            "epic": "fn-1",
            "status": "todo",
        }
        self._setup_task(git_repo, "fn-1.1", definition)

        # Point FLOW_STATE_DIR to an empty directory
        empty_state = git_repo / "empty-state"
        empty_state.mkdir()
        os.environ["FLOW_STATE_DIR"] = str(empty_state)

        result = load_task_with_state("fn-1.1")
        # Should fall back to definition's status
        assert result["status"] == "todo"
        assert result["title"] == "Test task"

    def test_normalizes_task(self, git_repo, monkeypatch):
        """Result should have all normalized fields (priority, depends_on, etc.)."""
        definition = {
            "id": "fn-1.1",
            "title": "Test task",
            "epic": "fn-1",
        }
        self._setup_task(git_repo, "fn-1.1", definition)

        empty_state = git_repo / "empty-state"
        empty_state.mkdir(exist_ok=True)
        os.environ["FLOW_STATE_DIR"] = str(empty_state)

        result = load_task_with_state("fn-1.1")
        # normalize_task should have added defaults
        assert result["priority"] is None
        assert result["depends_on"] == []
        assert result["impl"] is None
        assert result["review"] is None
        assert result["sync"] is None

    def test_definition_without_status(self, git_repo, monkeypatch):
        """When definition has no status and no runtime state, defaults to 'todo'."""
        definition = {
            "id": "fn-1.1",
            "title": "No status task",
            "epic": "fn-1",
        }
        self._setup_task(git_repo, "fn-1.1", definition)

        empty_state = git_repo / "empty-state"
        empty_state.mkdir(exist_ok=True)
        os.environ["FLOW_STATE_DIR"] = str(empty_state)

        result = load_task_with_state("fn-1.1")
        assert result["status"] == "todo"
