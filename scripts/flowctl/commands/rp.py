"""RepoPrompt CLI integration: helpers and rp subcommands."""

import argparse
import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any, Optional

from flowctl.core.io import atomic_write, error_exit, read_text_or_exit


# ─────────────────────────────────────────────────────────────────────────────
# RP CLI helpers
# ─────────────────────────────────────────────────────────────────────────────


def require_rp_cli() -> str:
    """Ensure rp-cli is available."""
    rp = shutil.which("rp-cli")
    if not rp:
        error_exit("rp-cli not found in PATH", use_json=False, code=2)
    return rp


def run_rp_cli(
    args: list[str], timeout: Optional[int] = None
) -> subprocess.CompletedProcess:
    """Run rp-cli with safe error handling and timeout.

    Args:
        args: Command arguments to pass to rp-cli
        timeout: Max seconds to wait. Default from FLOW_RP_TIMEOUT env or 1200s (20min).
    """
    if timeout is None:
        timeout = int(os.environ.get("FLOW_RP_TIMEOUT", "1200"))
    rp = require_rp_cli()
    cmd = [rp] + args
    try:
        return subprocess.run(
            cmd, capture_output=True, text=True, check=True, timeout=timeout
        )
    except subprocess.TimeoutExpired:
        error_exit(f"rp-cli timed out after {timeout}s", use_json=False, code=3)
    except subprocess.CalledProcessError as e:
        msg = (e.stderr or e.stdout or str(e)).strip()
        error_exit(f"rp-cli failed: {msg}", use_json=False, code=2)


def normalize_repo_root(path: str) -> list[str]:
    """Normalize repo root for window matching.

    Handles:
    - /tmp ↔ /private/tmp symlink on macOS
    - Git worktree → main repo resolution (worktree paths differ from main repo)
    """
    root = os.path.realpath(path)
    roots = [root]

    # macOS /tmp symlink
    if root.startswith("/private/tmp/"):
        roots.append("/tmp/" + root[len("/private/tmp/"):])
    elif root.startswith("/tmp/"):
        roots.append("/private/tmp/" + root[len("/tmp/"):])

    # Git worktree → main repo resolution
    # Worktrees live at .claude/worktrees/<name>/ but RP tracks the main repo
    try:
        import subprocess
        git_common = subprocess.run(
            ["git", "-C", root, "rev-parse", "--git-common-dir"],
            capture_output=True, text=True, timeout=5,
        )
        if git_common.returncode == 0:
            common_dir = git_common.stdout.strip()
            # --git-common-dir returns the .git dir of the main repo
            # e.g., /Users/x/project/.git → main repo is /Users/x/project
            if common_dir and not common_dir.startswith("."):
                main_repo = os.path.dirname(os.path.realpath(common_dir))
                if main_repo != root:
                    roots.append(main_repo)
                    # Also add /tmp variants of main repo
                    if main_repo.startswith("/private/tmp/"):
                        roots.append("/tmp/" + main_repo[len("/private/tmp/"):])
    except Exception:
        pass  # Git not available or not a repo — skip

    return list(dict.fromkeys(roots))


def parse_windows(raw: str) -> list[dict[str, Any]]:
    """Parse rp-cli windows JSON."""
    try:
        data = json.loads(raw)
        if isinstance(data, list):
            return data
        if (
            isinstance(data, dict)
            and "windows" in data
            and isinstance(data["windows"], list)
        ):
            return data["windows"]
    except json.JSONDecodeError as e:
        if "single-window mode" in raw:
            return [{"windowID": 1, "rootFolderPaths": []}]
        error_exit(f"windows JSON parse failed: {e}", use_json=False, code=2)
    error_exit("windows JSON has unexpected shape", use_json=False, code=2)


def extract_window_id(win: dict[str, Any]) -> Optional[int]:
    for key in ("windowID", "windowId", "id"):
        if key in win:
            try:
                return int(win[key])
            except Exception:
                return None
    return None


def extract_root_paths(win: dict[str, Any]) -> list[str]:
    for key in ("rootFolderPaths", "rootFolders", "rootFolderPath"):
        if key in win:
            val = win[key]
            if isinstance(val, list):
                return [str(v) for v in val]
            if isinstance(val, str):
                return [val]
    return []


def parse_builder_tab(output: str) -> str:
    match = re.search(r"Tab:\s*([A-Za-z0-9-]+)", output)
    if not match:
        error_exit("builder output missing Tab id", use_json=False, code=2)
    return match.group(1)


def parse_chat_id(output: str) -> Optional[str]:
    match = re.search(r"Chat\s*:\s*`([^`]+)`", output)
    if match:
        return match.group(1)
    match = re.search(r"\"chat_id\"\s*:\s*\"([^\"]+)\"", output)
    if match:
        return match.group(1)
    return None


def build_chat_payload(
    message: str,
    mode: str,
    new_chat: bool = False,
    chat_name: Optional[str] = None,
    chat_id: Optional[str] = None,
    selected_paths: Optional[list[str]] = None,
) -> str:
    payload: dict[str, Any] = {
        "message": message,
        "mode": mode,
    }
    if new_chat:
        payload["new_chat"] = True
    if chat_name:
        payload["chat_name"] = chat_name
    if chat_id:
        payload["chat_id"] = chat_id
    if selected_paths:
        payload["selected_paths"] = selected_paths
    return json.dumps(payload, ensure_ascii=False, separators=(",", ":"))


# ─────────────────────────────────────────────────────────────────────────────
# RP subcommands
# ─────────────────────────────────────────────────────────────────────────────


def cmd_prep_chat(args: argparse.Namespace) -> None:
    """Prepare JSON payload for rp-cli chat_send. Handles escaping safely."""
    # Read message from file
    message = read_text_or_exit(Path(args.message_file), "Message file", use_json=False)
    json_str = build_chat_payload(
        message=message,
        mode=args.mode,
        new_chat=args.new_chat,
        chat_name=args.chat_name,
        selected_paths=args.selected_paths,
    )

    if args.output:
        atomic_write(Path(args.output), json_str)
        print(f"Wrote {args.output}", file=sys.stderr)
    else:
        print(json_str)


def cmd_rp_windows(args: argparse.Namespace) -> None:
    result = run_rp_cli(["--raw-json", "-e", "windows"])
    raw = result.stdout or ""
    if args.json:
        windows = parse_windows(raw)
        print(json.dumps(windows))
    else:
        print(raw, end="")


def cmd_rp_pick_window(args: argparse.Namespace) -> None:
    repo_root = args.repo_root
    roots = normalize_repo_root(repo_root)
    result = run_rp_cli(["--raw-json", "-e", "windows"])
    windows = parse_windows(result.stdout or "")
    if len(windows) == 1 and not extract_root_paths(windows[0]):
        win_id = extract_window_id(windows[0])
        if win_id is None:
            error_exit("No window matches repo root", use_json=False, code=2)
        if args.json:
            print(json.dumps({"window": win_id}))
        else:
            print(win_id)
        return
    for win in windows:
        win_id = extract_window_id(win)
        if win_id is None:
            continue
        for path in extract_root_paths(win):
            if path in roots:
                if args.json:
                    print(json.dumps({"window": win_id}))
                else:
                    print(win_id)
                return
    error_exit("No window matches repo root", use_json=False, code=2)


def cmd_rp_ensure_workspace(args: argparse.Namespace) -> None:
    window = args.window
    repo_root = os.path.realpath(args.repo_root)
    ws_name = os.path.basename(repo_root)

    list_cmd = [
        "--raw-json",
        "-w",
        str(window),
        "-e",
        f"call manage_workspaces {json.dumps({'action': 'list'})}",
    ]
    list_res = run_rp_cli(list_cmd)
    try:
        data = json.loads(list_res.stdout)
    except json.JSONDecodeError as e:
        error_exit(f"workspace list JSON parse failed: {e}", use_json=False, code=2)

    def extract_names(obj: Any) -> set[str]:
        names: set[str] = set()
        if isinstance(obj, dict):
            if "workspaces" in obj:
                obj = obj["workspaces"]
            elif "result" in obj:
                obj = obj["result"]
        if isinstance(obj, list):
            for item in obj:
                if isinstance(item, str):
                    names.add(item)
                elif isinstance(item, dict):
                    for key in ("name", "workspace", "title"):
                        if key in item:
                            names.add(str(item[key]))
        return names

    names = extract_names(data)

    if ws_name not in names:
        create_cmd = [
            "-w",
            str(window),
            "-e",
            f"call manage_workspaces {json.dumps({'action': 'create', 'name': ws_name, 'folder_path': repo_root})}",
        ]
        run_rp_cli(create_cmd)

    switch_cmd = [
        "-w",
        str(window),
        "-e",
        f"call manage_workspaces {json.dumps({'action': 'switch', 'workspace': ws_name, 'window_id': window})}",
    ]
    run_rp_cli(switch_cmd)


def cmd_rp_builder(args: argparse.Namespace) -> None:
    window = args.window
    summary = args.summary
    response_type = getattr(args, "response_type", None)

    # Build builder command with optional --type flag (shorthand for response_type)
    builder_expr = f"builder {json.dumps(summary)}"
    if response_type:
        builder_expr += f" --type {response_type}"

    cmd = [
        "-w",
        str(window),
        "--raw-json" if response_type else "",
        "-e",
        builder_expr,
    ]
    cmd = [c for c in cmd if c]  # Remove empty strings
    res = run_rp_cli(cmd)
    output = (res.stdout or "") + ("\n" + res.stderr if res.stderr else "")

    # For review response-type, parse the full JSON response
    if response_type == "review":
        try:
            data = json.loads(res.stdout or "{}")
            tab = data.get("tab_id", "")
            chat_id = data.get("review", {}).get("chat_id", "")
            review_response = data.get("review", {}).get("response", "")
            if args.json:
                print(
                    json.dumps(
                        {
                            "window": window,
                            "tab": tab,
                            "chat_id": chat_id,
                            "review": review_response,
                            "file_count": data.get("file_count", 0),
                            "total_tokens": data.get("total_tokens", 0),
                        }
                    )
                )
            else:
                print(f"T={tab} CHAT_ID={chat_id}")
                if review_response:
                    print(review_response)
        except json.JSONDecodeError:
            tab = parse_builder_tab(output)
            if args.json:
                print(json.dumps({"window": window, "tab": tab, "error": "parse_failed"}))
            else:
                print(tab)
    else:
        tab = parse_builder_tab(output)
        if args.json:
            print(json.dumps({"window": window, "tab": tab}))
        else:
            print(tab)


def cmd_rp_prompt_get(args: argparse.Namespace) -> None:
    cmd = ["-w", str(args.window), "-t", args.tab, "-e", "prompt get"]
    res = run_rp_cli(cmd)
    print(res.stdout, end="")


def cmd_rp_prompt_set(args: argparse.Namespace) -> None:
    message = read_text_or_exit(Path(args.message_file), "Message file", use_json=False)
    payload = json.dumps({"op": "set", "text": message})
    cmd = [
        "-w",
        str(args.window),
        "-t",
        args.tab,
        "-e",
        f"call prompt {payload}",
    ]
    res = run_rp_cli(cmd)
    print(res.stdout, end="")


def cmd_rp_select_get(args: argparse.Namespace) -> None:
    cmd = ["-w", str(args.window), "-t", args.tab, "-e", "select get"]
    res = run_rp_cli(cmd)
    print(res.stdout, end="")


def cmd_rp_select_add(args: argparse.Namespace) -> None:
    if not args.paths:
        error_exit("select-add requires at least one path", use_json=False, code=2)
    quoted = " ".join(shlex.quote(p) for p in args.paths)
    cmd = ["-w", str(args.window), "-t", args.tab, "-e", f"select add {quoted}"]
    res = run_rp_cli(cmd)
    print(res.stdout, end="")


def cmd_rp_chat_send(args: argparse.Namespace) -> None:
    message = read_text_or_exit(Path(args.message_file), "Message file", use_json=False)
    chat_id_arg = getattr(args, "chat_id", None)
    mode = getattr(args, "mode", "chat") or "chat"
    payload = build_chat_payload(
        message=message,
        mode=mode,
        new_chat=args.new_chat,
        chat_name=args.chat_name,
        chat_id=chat_id_arg,
        selected_paths=args.selected_paths,
    )
    cmd = [
        "-w",
        str(args.window),
        "-t",
        args.tab,
        "-e",
        f"call chat_send {payload}",
    ]
    res = run_rp_cli(cmd)
    output = (res.stdout or "") + ("\n" + res.stderr if res.stderr else "")
    chat_id = parse_chat_id(output)
    if args.json:
        print(json.dumps({"chat": chat_id}))
    else:
        print(res.stdout, end="")


def cmd_rp_prompt_export(args: argparse.Namespace) -> None:
    cmd = [
        "-w",
        str(args.window),
        "-t",
        args.tab,
        "-e",
        f"prompt export {shlex.quote(args.out)}",
    ]
    res = run_rp_cli(cmd)
    print(res.stdout, end="")


def cmd_rp_setup_review(args: argparse.Namespace) -> None:
    """Atomic setup: pick-window + builder.

    Returns W=<window> T=<tab> on success, exits non-zero on failure.
    With --response-type review, also returns CHAT_ID and review findings.
    Writes state file for ralph-guard to verify pick-window ran.

    Note: ensure-workspace removed - if user opens RP on a folder, workspace
    already exists. pick-window matches by folder path.

    Requires RepoPrompt 1.6.0+ for --response-type review.
    """
    repo_root = os.path.realpath(args.repo_root)
    summary = args.summary
    response_type = getattr(args, "response_type", None)

    # Step 1: pick-window
    roots = normalize_repo_root(repo_root)
    result = run_rp_cli(["--raw-json", "-e", "windows"])
    windows = parse_windows(result.stdout or "")

    win_id: Optional[int] = None

    # Single window with no root paths - use it
    if len(windows) == 1 and not extract_root_paths(windows[0]):
        win_id = extract_window_id(windows[0])

    # Otherwise match by root
    if win_id is None:
        for win in windows:
            wid = extract_window_id(win)
            if wid is None:
                continue
            for path in extract_root_paths(win):
                if path in roots:
                    win_id = wid
                    break
            if win_id is not None:
                break

    if win_id is None:
        if getattr(args, "create", False):
            # Auto-create window via workspace create --new-window (RP 1.5.68+)
            ws_name = os.path.basename(repo_root)
            create_cmd = f"workspace create {shlex.quote(ws_name)} --new-window --folder-path {shlex.quote(repo_root)}"
            create_res = run_rp_cli(["--raw-json", "-e", create_cmd])
            try:
                data = json.loads(create_res.stdout or "{}")
                win_id = data.get("window_id")
            except json.JSONDecodeError:
                pass
            if not win_id:
                error_exit(
                    f"Failed to create RP window: {create_res.stderr or create_res.stdout}",
                    use_json=False,
                    code=2,
                )
        else:
            error_exit("No RepoPrompt window matches repo root", use_json=False, code=2)

    # Write state file for ralph-guard verification
    repo_hash = hashlib.sha256(repo_root.encode()).hexdigest()[:16]
    state_file = Path(f"/tmp/.ralph-pick-window-{repo_hash}")
    state_file.write_text(f"{win_id}\n{repo_root}\n")

    # Step 2: builder (with optional --type flag for RP 1.6.0+)
    builder_expr = f"builder {json.dumps(summary)}"
    if response_type:
        builder_expr += f" --type {response_type}"

    builder_cmd = [
        "-w",
        str(win_id),
        "--raw-json" if response_type else "",
        "-e",
        builder_expr,
    ]
    builder_cmd = [c for c in builder_cmd if c]  # Remove empty strings
    builder_res = run_rp_cli(builder_cmd)
    output = (builder_res.stdout or "") + (
        "\n" + builder_res.stderr if builder_res.stderr else ""
    )

    # Parse response based on response-type
    if response_type == "review":
        try:
            data = json.loads(builder_res.stdout or "{}")
            tab = data.get("tab_id", "")
            chat_id = data.get("review", {}).get("chat_id", "")
            review_response = data.get("review", {}).get("response", "")

            if not tab:
                error_exit("Builder did not return a tab id", use_json=False, code=2)

            if args.json:
                print(
                    json.dumps(
                        {
                            "window": win_id,
                            "tab": tab,
                            "chat_id": chat_id,
                            "review": review_response,
                            "repo_root": repo_root,
                            "file_count": data.get("file_count", 0),
                            "total_tokens": data.get("total_tokens", 0),
                        }
                    )
                )
            else:
                print(f"W={win_id} T={tab} CHAT_ID={chat_id}")
                if review_response:
                    print(review_response)
        except json.JSONDecodeError:
            error_exit("Failed to parse builder review response", use_json=False, code=2)
    else:
        tab = parse_builder_tab(output)
        if not tab:
            error_exit("Builder did not return a tab id", use_json=False, code=2)

        if args.json:
            print(json.dumps({"window": win_id, "tab": tab, "repo_root": repo_root}))
        else:
            print(f"W={win_id} T={tab}")
