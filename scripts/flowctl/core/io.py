"""File I/O helpers: atomic writes, JSON loading, output formatting."""

import json
import os
import sys
import tempfile
from datetime import datetime
from pathlib import Path
from typing import Any

from flowctl.core.constants import SUPPORTED_SCHEMA_VERSIONS


def json_output(data: dict, success: bool = True) -> None:
    """Output JSON response."""
    result = {"success": success, **data}
    print(json.dumps(result, indent=2, default=str))


def error_exit(message: str, code: int = 1, use_json: bool = True) -> None:
    """Output error and exit."""
    if use_json:
        json_output({"error": message}, success=False)
    else:
        print(f"Error: {message}", file=sys.stderr)
    sys.exit(code)


def now_iso() -> str:
    """Current timestamp in ISO format."""
    return datetime.utcnow().isoformat() + "Z"


def is_supported_schema(version: Any) -> bool:
    """Check schema version compatibility."""
    try:
        return int(version) in SUPPORTED_SCHEMA_VERSIONS
    except Exception:
        return False


def atomic_write(path: Path, content: str) -> None:
    """Write file atomically via temp + rename."""
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp_path = tempfile.mkstemp(dir=path.parent, suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            f.write(content)
        os.replace(tmp_path, path)
    except Exception:
        if os.path.exists(tmp_path):
            os.unlink(tmp_path)
        raise


def atomic_write_json(path: Path, data: dict) -> None:
    """Write JSON file atomically with sorted keys."""
    content = json.dumps(data, indent=2, sort_keys=True) + "\n"
    atomic_write(path, content)


def load_json(path: Path) -> dict:
    """Load JSON file."""
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def load_json_or_exit(path: Path, what: str, use_json: bool = True) -> dict:
    """Load JSON file with safe error handling."""
    if not path.exists():
        error_exit(f"{what} missing: {path}", use_json=use_json)
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except json.JSONDecodeError as e:
        error_exit(f"{what} invalid JSON: {path} ({e})", use_json=use_json)
    except Exception as e:
        error_exit(f"{what} unreadable: {path} ({e})", use_json=use_json)


def read_text_or_exit(path: Path, what: str, use_json: bool = True) -> str:
    """Read text file with safe error handling."""
    if not path.exists():
        error_exit(f"{what} missing: {path}", use_json=use_json)
    try:
        return path.read_text(encoding="utf-8")
    except Exception as e:
        error_exit(f"{what} unreadable: {path} ({e})", use_json=use_json)


def read_file_or_stdin(file_arg: str, what: str, use_json: bool = True) -> str:
    """Read from file path or stdin if file_arg is '-'.

    Supports heredoc usage: flowctl ... --file - <<'EOF'
    """
    if file_arg == "-":
        try:
            return sys.stdin.read()
        except Exception as e:
            error_exit(f"Failed to read {what} from stdin: {e}", use_json=use_json)
    return read_text_or_exit(Path(file_arg), what, use_json=use_json)


def require_keys(obj: dict, keys: list[str], what: str, use_json: bool = True) -> None:
    """Validate dict has required keys. Exits on missing keys."""
    missing = [k for k in keys if k not in obj]
    if missing:
        error_exit(
            f"{what} missing required keys: {', '.join(missing)}", use_json=use_json
        )
