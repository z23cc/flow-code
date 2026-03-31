"""Path resolution: repo root, .flow directory, state directory."""

import os
import subprocess
from pathlib import Path

from flowctl.core.constants import FLOW_DIR


def get_repo_root() -> Path:
    """Find git repo root."""
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
        )
        return Path(result.stdout.strip())
    except subprocess.CalledProcessError:
        # Fallback to current directory
        return Path.cwd()


def get_flow_dir() -> Path:
    """Get .flow/ directory path."""
    return get_repo_root() / FLOW_DIR


def ensure_flow_exists() -> bool:
    """Check if .flow/ exists."""
    return get_flow_dir().exists()


def get_state_dir() -> Path:
    """Get state directory for runtime task state.

    Resolution order:
    1. FLOW_STATE_DIR env var (explicit override for orchestrators)
    2. git common-dir (shared across all worktrees automatically)
    3. Fallback to .flow/state for non-git repos
    """
    # 1. Explicit override
    if state_dir := os.environ.get("FLOW_STATE_DIR"):
        return Path(state_dir).resolve()

    # 2. Git common-dir (shared across worktrees)
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--git-common-dir", "--path-format=absolute"],
            capture_output=True,
            text=True,
            check=True,
        )
        common = result.stdout.strip()
        return Path(common) / "flow-state"
    except subprocess.CalledProcessError:
        pass

    # 3. Fallback for non-git repos
    return get_flow_dir() / "state"
