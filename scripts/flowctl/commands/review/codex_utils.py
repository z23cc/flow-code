"""Codex CLI utilities: require, run, parse, sandbox resolution."""

"""Codex integration, review prompt building, and checkpoint commands."""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any, Optional

from flowctl.core.constants import EPICS_DIR, SPECS_DIR, TASKS_DIR
from flowctl.core.git import (
    gather_context_hints,
    get_changed_files,
    get_embedded_file_contents,
)
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
from flowctl.core.paths import ensure_flow_exists, get_flow_dir, get_repo_root
from flowctl.core.state import (
    delete_task_runtime,
    get_state_store,
)

# ─────────────────────────────────────────────────────────────────────────────
# Codex CLI helpers
# ─────────────────────────────────────────────────────────────────────────────


def require_codex() -> str:
    """Ensure codex CLI is available. Returns path to codex."""
    codex = shutil.which("codex")
    if not codex:
        error_exit("codex not found in PATH", use_json=False, code=2)
    return codex


def get_codex_version() -> Optional[str]:
    """Get codex version, or None if not available."""
    codex = shutil.which("codex")
    if not codex:
        return None
    try:
        result = subprocess.run(
            [codex, "--version"],
            capture_output=True,
            text=True,
            check=True,
        )
        # Parse version from output like "codex 0.1.2" or "0.1.2"
        output = result.stdout.strip()
        match = re.search(r"(\d+\.\d+\.\d+)", output)
        return match.group(1) if match else output
    except subprocess.CalledProcessError:
        return None


# ─────────────────────────────────────────────────────────────────────────────
# Receipt lifecycle helpers
# ─────────────────────────────────────────────────────────────────────────────


def load_receipt(path: Optional[str]) -> tuple[Optional[str], bool]:
    """Load a review receipt and extract session info for re-reviews.

    Args:
        path: Receipt file path (may be None).

    Returns:
        tuple: (session_id, is_rereview)
        - session_id: Codex thread ID from previous review, or None.
        - is_rereview: True if a valid session was found (indicates re-review).
    """
    if not path:
        return None, False
    receipt_file = Path(path)
    if not receipt_file.exists():
        return None, False
    try:
        receipt_data = json.loads(receipt_file.read_text(encoding="utf-8"))
        session_id = receipt_data.get("session_id")
        return session_id, session_id is not None
    except (json.JSONDecodeError, Exception):
        return None, False


def save_receipt(
    path: str,
    *,
    review_type: str,
    review_id: str,
    mode: str = "codex",
    verdict: str,
    session_id: Optional[str],
    output: str,
    base_branch: Optional[str] = None,
    focus: Optional[str] = None,
) -> None:
    """Write a Ralph-compatible review receipt to *path*.

    Automatically includes the current RALPH_ITERATION env var if set.
    """
    receipt_data: dict[str, Any] = {
        "type": review_type,
        "id": review_id,
        "mode": mode,
        "verdict": verdict,
        "session_id": session_id,
        "timestamp": now_iso(),
        "review": output,
    }
    if base_branch is not None:
        receipt_data["base"] = base_branch
    if focus is not None:
        receipt_data["focus"] = focus

    # Add iteration if running under Ralph
    ralph_iter = os.environ.get("RALPH_ITERATION")
    if ralph_iter:
        try:
            receipt_data["iteration"] = int(ralph_iter)
        except ValueError:
            pass

    Path(path).write_text(
        json.dumps(receipt_data, indent=2) + "\n", encoding="utf-8"
    )


def delete_stale_receipt(path: Optional[str]) -> None:
    """Delete a receipt file if it exists (best-effort).

    Used to clear stale receipts on review failure so they don't
    falsely satisfy downstream gates.
    """
    if not path:
        return
    try:
        Path(path).unlink(missing_ok=True)
    except OSError:
        pass


CODEX_SANDBOX_MODES = {"read-only", "workspace-write", "danger-full-access", "auto"}


def resolve_codex_sandbox(sandbox: str) -> str:
    """Resolve sandbox mode, handling 'auto' based on platform.

    Priority: CLI --sandbox (if not 'auto') > CODEX_SANDBOX env var > platform default.
    'auto' resolves to 'danger-full-access' on Windows (where sandbox blocks reads),
    and 'read-only' on Unix.

    Returns the resolved sandbox value (never returns 'auto').
    Raises ValueError if invalid mode specified.
    """
    # Normalize input
    sandbox = sandbox.strip() if sandbox else "auto"

    # CLI --sandbox takes priority over env var if explicitly set (not auto)
    if sandbox and sandbox != "auto":
        if sandbox not in CODEX_SANDBOX_MODES:
            raise ValueError(
                f"Invalid sandbox value: {sandbox!r}. "
                f"Valid options: {', '.join(sorted(CODEX_SANDBOX_MODES))}"
            )
        return sandbox

    # Check CODEX_SANDBOX env var (Ralph config) when CLI is 'auto' or not specified
    env_sandbox = os.environ.get("CODEX_SANDBOX", "").strip()
    if env_sandbox:
        if env_sandbox not in CODEX_SANDBOX_MODES:
            raise ValueError(
                f"Invalid CODEX_SANDBOX value: {env_sandbox!r}. "
                f"Valid options: {', '.join(sorted(CODEX_SANDBOX_MODES))}"
            )
        if env_sandbox != "auto":
            return env_sandbox

    # Both CLI and env are 'auto' or unset - resolve based on platform
    return "danger-full-access" if os.name == "nt" else "read-only"


CODEX_EFFORT_LEVELS = ("medium", "high", "xhigh")


def run_codex_exec(
    prompt: str,
    session_id: Optional[str] = None,
    sandbox: str = "read-only",
    model: Optional[str] = None,
    effort: str = "high",
) -> tuple[str, Optional[str], int, str]:
    """Run codex exec and return (stdout, thread_id, exit_code, stderr).

    If session_id provided, tries to resume. Falls back to new session if resume fails.
    Model: FLOW_CODEX_MODEL env > parameter > default (gpt-5.4 + high reasoning).

    Note: Prompt is passed via stdin (using '-') to avoid Windows command-line
    length limits (~8191 chars) and special character escaping issues. (GH-35)

    Returns:
        tuple: (stdout, thread_id, exit_code, stderr)
        - exit_code is 0 for success, non-zero for failure
        - stderr contains error output from the process
    """
    codex = require_codex()
    # Model priority: env > parameter > default (gpt-5.4 + high reasoning = GPT 5.4 High)
    effective_model = os.environ.get("FLOW_CODEX_MODEL") or model or "gpt-5.4"

    if session_id:
        # Try resume first - use stdin for prompt (model already set in original session)
        cmd = [codex, "exec", "resume", session_id, "-"]
        try:
            result = subprocess.run(
                cmd,
                input=prompt,
                capture_output=True,
                text=True,
                check=True,
                timeout=600,
            )
            output = result.stdout
            # For resumed sessions, thread_id stays the same
            return output, session_id, 0, result.stderr
        except subprocess.CalledProcessError:
            # Resume failed - fall through to new session
            pass
        except subprocess.TimeoutExpired:
            # Resume failed - fall through to new session
            pass

    # New session with model + high reasoning effort
    # --skip-git-repo-check: safe with read-only sandbox, allows reviews from /tmp etc (GH-33)
    # Use '-' to read prompt from stdin - avoids Windows CLI length limits (GH-35)
    cmd = [
        codex,
        "exec",
        "--model",
        effective_model,
        "-c",
        f'model_reasoning_effort="{effort}"',
        "--sandbox",
        sandbox,
        "--skip-git-repo-check",
        "--json",
        "-",
    ]
    try:
        result = subprocess.run(
            cmd,
            input=prompt,
            capture_output=True,
            text=True,
            check=False,  # Don't raise on non-zero exit
            timeout=600,
        )
        output = result.stdout
        thread_id = parse_codex_thread_id(output)
        return output, thread_id, result.returncode, result.stderr
    except subprocess.TimeoutExpired:
        return "", None, 2, "codex exec timed out (600s)"


def parse_codex_thread_id(output: str) -> Optional[str]:
    """Extract thread_id from codex --json output.

    Looks for: {"type":"thread.started","thread_id":"019baa19-..."}
    """
    for line in output.split("\n"):
        if not line.strip():
            continue
        try:
            data = json.loads(line)
            if data.get("type") == "thread.started" and "thread_id" in data:
                return data["thread_id"]
        except json.JSONDecodeError:
            continue
    return None


def parse_codex_verdict(output: str) -> Optional[str]:
    """Extract verdict from codex output.

    Looks for <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>
    """
    match = re.search(r"<verdict>(SHIP|NEEDS_WORK|MAJOR_RETHINK)</verdict>", output)
    return match.group(1) if match else None


def is_sandbox_failure(exit_code: int, stdout: str, stderr: str) -> bool:
    """Detect if codex failure is due to sandbox restrictions.

    Returns True if the failure appears to be caused by sandbox policy blocking
    operations rather than actual code issues. Checks:
    1. exit_code != 0 (must be a failure)
    2. Error patterns in stderr or JSON item failures in stdout

    Only matches error patterns in actual error contexts (stderr, failed items),
    not in regular output that might mention these phrases.
    """
    if exit_code == 0:
        return False

    # Patterns that indicate Codex sandbox policy blocking operations
    # Keep these specific to avoid false positives on unrelated failures
    sandbox_patterns = [
        r"blocked by policy",
        r"rejected by policy",
        r"rejected:.*policy",
        r"filesystem read is blocked",
        r"filesystem write is blocked",
        r"shell command.*blocked",
        r"AppContainer",  # Windows sandbox container
    ]

    # Check stderr for sandbox patterns
    stderr_lower = stderr.lower()
    for pattern in sandbox_patterns:
        if re.search(pattern, stderr_lower, re.IGNORECASE):
            return True

    # Check JSON output for failed items with rejection messages
    # Codex JSON streaming includes items like:
    # {"type":"item.completed","item":{"status":"failed","aggregated_output":"...rejected..."}}
    for line in stdout.split("\n"):
        if not line.strip():
            continue
        try:
            data = json.loads(line)
            # Look for failed items
            if data.get("type") == "item.completed":
                item = data.get("item", {})
                if item.get("status") == "failed":
                    # Check aggregated_output for sandbox patterns
                    aggregated = item.get("aggregated_output", "")
                    if aggregated:
                        aggregated_lower = aggregated.lower()
                        for pattern in sandbox_patterns:
                            if re.search(pattern, aggregated_lower, re.IGNORECASE):
                                return True
        except json.JSONDecodeError:
            continue

    return False

