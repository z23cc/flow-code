"""Shared pytest fixtures for flowctl tests."""

import json
import os
import subprocess

import pytest


@pytest.fixture
def flow_dir(tmp_path, monkeypatch):
    """Set up a minimal .flow/ directory structure in a temp directory.

    Creates:
      .flow/meta.json          — schema version 2
      .flow/epics/             — empty
      .flow/tasks/             — empty
      .flow/specs/             — empty
      .flow/config.json        — default config

    Changes cwd to tmp_path so flowctl path resolution finds .flow/.
    """
    flow = tmp_path / ".flow"
    flow.mkdir()
    (flow / "epics").mkdir()
    (flow / "tasks").mkdir()
    (flow / "specs").mkdir()

    meta = {"schema_version": 2, "next_epic": 1}
    (flow / "meta.json").write_text(json.dumps(meta, indent=2) + "\n", encoding="utf-8")

    config = {"memory": {"enabled": True}, "stack": {}}
    (flow / "config.json").write_text(
        json.dumps(config, indent=2) + "\n", encoding="utf-8"
    )

    monkeypatch.chdir(tmp_path)
    return flow


@pytest.fixture
def git_repo(tmp_path, monkeypatch):
    """Initialize a git repo in tmp_path with a .flow/ directory.

    Needed by state.py's get_state_dir() which calls git rev-parse.
    Returns the tmp_path (repo root).
    """
    monkeypatch.chdir(tmp_path)

    subprocess.run(
        ["git", "init"],
        cwd=str(tmp_path),
        capture_output=True,
        check=True,
    )
    subprocess.run(
        ["git", "config", "user.email", "test@test.com"],
        cwd=str(tmp_path),
        capture_output=True,
        check=True,
    )
    subprocess.run(
        ["git", "config", "user.name", "Test"],
        cwd=str(tmp_path),
        capture_output=True,
        check=True,
    )

    # Create .flow/ structure (same as flow_dir fixture but inside git repo)
    flow = tmp_path / ".flow"
    flow.mkdir()
    (flow / "epics").mkdir()
    (flow / "tasks").mkdir()
    (flow / "specs").mkdir()

    meta = {"schema_version": 2, "next_epic": 1}
    (flow / "meta.json").write_text(json.dumps(meta, indent=2) + "\n", encoding="utf-8")

    config = {"memory": {"enabled": True}, "stack": {}}
    (flow / "config.json").write_text(
        json.dumps(config, indent=2) + "\n", encoding="utf-8"
    )

    # Initial commit so git rev-parse works
    subprocess.run(
        ["git", "add", "."],
        cwd=str(tmp_path),
        capture_output=True,
        check=True,
    )
    subprocess.run(
        ["git", "commit", "-m", "init"],
        cwd=str(tmp_path),
        capture_output=True,
        check=True,
    )

    return tmp_path
