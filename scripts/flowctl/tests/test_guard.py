"""Tests for ralph-guard.py hook logic with fixture-based JSON payloads.

ralph-guard.py is a standalone Python script (not a package module), so we
import it via importlib.  Each test function builds a fixture dict that mimics
the JSON Claude Code sends to hooks, then calls the handler directly.

Because handle_pre_tool_use / handle_post_tool_use call sys.exit(), we catch
SystemExit to inspect the exit code (0 = allow, 2 = block).
"""

import importlib.util
import json
import os
import sys
from pathlib import Path
from unittest.mock import patch

import pytest

# ---------------------------------------------------------------------------
# Import ralph-guard.py as a module (filename contains a hyphen)
# ---------------------------------------------------------------------------
_GUARD_PATH = (
    Path(__file__).resolve().parents[2] / "hooks" / "ralph-guard.py"
)


def _load_guard():
    """Load ralph-guard.py as a Python module."""
    spec = importlib.util.spec_from_file_location("ralph_guard", _GUARD_PATH)
    mod = importlib.util.module_from_spec(spec)
    # Patch FLOW_RALPH so module-level code doesn't exit during import
    with patch.dict(os.environ, {"FLOW_RALPH": "1"}):
        spec.loader.exec_module(mod)
    return mod


guard = _load_guard()


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _pre_payload(command: str, *, session_id: str = "test-session") -> dict:
    """Build a PreToolUse fixture for a Bash command."""
    return {
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": command},
        "session_id": session_id,
    }


def _post_payload(
    command: str,
    response: str = "",
    *,
    session_id: str = "test-session",
) -> dict:
    """Build a PostToolUse fixture for a Bash command."""
    return {
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": command},
        "tool_response": {"stdout": response},
        "session_id": session_id,
    }


def _stop_payload(*, session_id: str = "test-session", stop_hook_active: bool = False) -> dict:
    """Build a Stop event fixture."""
    return {
        "hook_event_name": "Stop",
        "session_id": session_id,
        "stop_hook_active": stop_hook_active,
    }


def _edit_payload(file_path: str, *, session_id: str = "test-session") -> dict:
    """Build a PreToolUse fixture for an Edit tool (protected file check)."""
    return {
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "tool_input": {"file_path": file_path},
        "session_id": session_id,
    }


def _reset_state(session_id: str = "test-session") -> None:
    """Remove any leftover state file for the session."""
    state_file = guard.get_state_file(session_id)
    if state_file.exists():
        state_file.unlink()


@pytest.fixture(autouse=True)
def _clean_state():
    """Ensure clean state before and after each test."""
    _reset_state()
    yield
    _reset_state()


# ===================================================================
# PreToolUse — blocked command patterns (acceptance: >= 5 cases)
# ===================================================================


class TestPreToolUseBlocked:
    """Commands that ralph-guard MUST block (exit code 2)."""

    def test_chat_send_json_flag(self):
        """--json on chat-send suppresses review text."""
        data = _pre_payload('rp chat-send --json --message "review"')
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 2

    def test_chat_send_new_chat_on_re_review(self):
        """--new-chat on re-reviews loses reviewer context."""
        # Simulate a first chat already sent
        state = guard.load_state("test-session")
        state["chats_sent"] = 1
        guard.save_state("test-session", state)

        data = _pre_payload('rp chat-send --new-chat --message "re-review"')
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 2

    def test_direct_codex_exec(self):
        """Direct 'codex exec' must be blocked — use flowctl wrappers."""
        data = _pre_payload('codex exec "review this code"')
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 2

    def test_direct_codex_review(self):
        """Direct 'codex review' must be blocked."""
        data = _pre_payload("codex review --diff main..HEAD")
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 2

    def test_codex_last_flag(self):
        """--last flag on codex (even through wrapper) breaks session continuity."""
        data = _pre_payload("flowctl codex impl-review --last")
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 2

    def test_setup_review_missing_repo_root(self):
        """setup-review without --repo-root must be blocked."""
        data = _pre_payload('setup-review --summary "test"')
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 2

    def test_setup_review_missing_summary(self):
        """setup-review without --summary must be blocked."""
        data = _pre_payload("setup-review --repo-root /tmp/repo")
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 2

    def test_select_add_missing_window(self):
        """select-add without --window must be blocked."""
        data = _pre_payload("select-add src/main.py")
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 2

    def test_flowctl_done_missing_evidence(self):
        """flowctl done without --evidence-json must be blocked."""
        data = _pre_payload(
            'flowctl done fn-1.1 --summary-file /tmp/s.md'
        )
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 2

    def test_flowctl_done_missing_summary(self):
        """flowctl done without --summary-file must be blocked."""
        data = _pre_payload(
            "flowctl done fn-1.1 --evidence-json /tmp/e.json"
        )
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 2

    def test_receipt_write_before_review(self, monkeypatch):
        """Cannot write receipt before chat-send or codex review succeeds."""
        monkeypatch.setenv("REVIEW_RECEIPT_PATH", "/tmp/receipts/impl-fn-1.1.json")
        data = _pre_payload(
            'cat > "/tmp/receipts/impl-fn-1.1.json" << \'EOF\'\n'
            '{"type":"impl_review","id":"fn-1.1","verdict":"SHIP"}\n'
            "EOF"
        )
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 2


# ===================================================================
# PreToolUse — allowed command patterns (acceptance: >= 5 cases)
# ===================================================================


class TestPreToolUseAllowed:
    """Commands that ralph-guard must allow (exit code 0)."""

    def test_flowctl_codex_impl_review(self):
        """flowctl codex wrapper calls are allowed."""
        data = _pre_payload("flowctl codex impl-review fn-1.1 --base abc123")
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 0

    def test_flowctl_codex_plan_review(self):
        """flowctl codex plan-review wrapper is allowed."""
        data = _pre_payload("flowctl codex plan-review fn-1")
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 0

    def test_chat_send_no_json(self):
        """chat-send without --json is fine."""
        data = _pre_payload('rp chat-send --message "review this"')
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 0

    def test_chat_send_new_chat_first_review(self):
        """--new-chat on FIRST review is allowed (chats_sent == 0)."""
        data = _pre_payload('rp chat-send --new-chat --message "first review"')
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 0

    def test_flowctl_done_with_all_flags(self):
        """flowctl done with both required flags is allowed."""
        data = _pre_payload(
            "flowctl done fn-1.1 --summary-file /tmp/s.md --evidence-json /tmp/e.json"
        )
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 0

    def test_plain_git_command(self):
        """Regular git commands should pass through."""
        data = _pre_payload("git status")
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 0

    def test_flowctl_show(self):
        """Non-done flowctl commands pass through."""
        data = _pre_payload("flowctl show fn-1.1 --json")
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 0

    def test_setup_review_complete(self):
        """setup-review with all required flags is allowed."""
        data = _pre_payload(
            'setup-review --repo-root /tmp/repo --summary "review this"'
        )
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 0

    def test_select_add_with_window(self):
        """select-add with --window is allowed."""
        data = _pre_payload('select-add --window "1" --tab "abc" src/main.py')
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 0

    def test_FLOWCTL_variable_done(self):
        """$FLOWCTL done with both flags is allowed."""
        data = _pre_payload(
            '"$FLOWCTL" done fn-1.1 --summary-file /tmp/s.md --evidence-json /tmp/e.json'
        )
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 0

    def test_flowctl_done_help(self):
        """flowctl done --help should not be blocked."""
        data = _pre_payload("flowctl done --help")
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 0


# ===================================================================
# PostToolUse — state transitions (acceptance criterion)
# ===================================================================


class TestPostToolUseStateTransitions:
    """Verify state tracking across PostToolUse events."""

    def test_chat_send_success_sets_state(self):
        """Successful chat-send sets chat_send_succeeded and increments chats_sent."""
        data = _post_payload(
            'rp chat-send --message "review"',
            "Chat Send completed\n<verdict>NEEDS_WORK</verdict>\nFix the tests.",
        )
        # handle_post_tool_use calls sys.exit(0) at the end
        with pytest.raises(SystemExit):
            guard.handle_post_tool_use(data)

        state = guard.load_state("test-session")
        assert state["chat_send_succeeded"] is True
        assert state["chats_sent"] == 1

    def test_chat_send_null_clears_state(self):
        """chat-send returning {"chat": null} clears chat_send_succeeded."""
        # First, simulate a previous success
        initial = guard.load_state("test-session")
        initial["chat_send_succeeded"] = True
        guard.save_state("test-session", initial)

        data = _post_payload(
            'rp chat-send --json --message "review"',
            '{"chat": null}',
        )
        with pytest.raises(SystemExit):
            guard.handle_post_tool_use(data)

        state = guard.load_state("test-session")
        assert state["chat_send_succeeded"] is False

    def test_flowctl_done_tracks_task_id(self):
        """flowctl done <task> adds task to flowctl_done_called set."""
        data = _post_payload(
            "flowctl done fn-1.2 --summary-file /tmp/s.md --evidence-json /tmp/e.json",
            '{"status": "done", "task": "fn-1.2"}',
        )
        with pytest.raises(SystemExit):
            guard.handle_post_tool_use(data)

        state = guard.load_state("test-session")
        assert "fn-1.2" in state["flowctl_done_called"]

    def test_flowctl_done_accumulates(self):
        """Multiple flowctl done calls accumulate task IDs."""
        # First task
        data1 = _post_payload(
            "flowctl done fn-1.1 --summary-file /tmp/s.md --evidence-json /tmp/e.json",
            '{"status": "done", "task": "fn-1.1"}',
        )
        with pytest.raises(SystemExit):
            guard.handle_post_tool_use(data1)

        # Second task
        data2 = _post_payload(
            "flowctl done fn-1.2 --summary-file /tmp/s.md --evidence-json /tmp/e.json",
            '{"status": "done", "task": "fn-1.2"}',
        )
        with pytest.raises(SystemExit):
            guard.handle_post_tool_use(data2)

        state = guard.load_state("test-session")
        assert "fn-1.1" in state["flowctl_done_called"]
        assert "fn-1.2" in state["flowctl_done_called"]

    def test_codex_review_success(self):
        """Codex review with verdict sets codex_review_succeeded."""
        data = _post_payload(
            "flowctl codex impl-review fn-1.1 --base abc123",
            "Review output...\n<verdict>SHIP</verdict>\nAll good.",
        )
        with pytest.raises(SystemExit):
            guard.handle_post_tool_use(data)

        state = guard.load_state("test-session")
        assert state["codex_review_succeeded"] is True
        assert state["last_verdict"] == "SHIP"

    def test_codex_needs_work_verdict(self):
        """Codex review with NEEDS_WORK verdict."""
        data = _post_payload(
            "flowctl codex impl-review fn-1.1 --base abc123",
            "Review output...\n<verdict>NEEDS_WORK</verdict>\nFix errors.",
        )
        with pytest.raises(SystemExit):
            guard.handle_post_tool_use(data)

        state = guard.load_state("test-session")
        assert state["codex_review_succeeded"] is True
        assert state["last_verdict"] == "NEEDS_WORK"

    def test_receipt_write_resets_review_state(self, monkeypatch):
        """Writing a receipt resets chat_send_succeeded and codex_review_succeeded."""
        monkeypatch.setenv("REVIEW_RECEIPT_PATH", "/tmp/receipts/impl-fn-1.1.json")

        # Set up state as if review succeeded
        initial = guard.load_state("test-session")
        initial["chat_send_succeeded"] = True
        initial["codex_review_succeeded"] = True
        guard.save_state("test-session", initial)

        data = _post_payload(
            'cat > "/tmp/receipts/impl-fn-1.1.json" <<EOF\n{"type":"impl_review"}\nEOF',
            "receipt written",
        )
        with pytest.raises(SystemExit):
            guard.handle_post_tool_use(data)

        state = guard.load_state("test-session")
        assert state["chat_send_succeeded"] is False
        assert state["codex_review_succeeded"] is False

    def test_setup_review_tracks_window_tab(self):
        """setup-review response tracking W= and T= values."""
        data = _post_payload(
            'setup-review --repo-root /tmp/repo --summary "test"',
            "Window created: W=42 T=ABC-DEF-123",
        )
        with pytest.raises(SystemExit):
            guard.handle_post_tool_use(data)

        state = guard.load_state("test-session")
        assert state["window"] == "42"
        assert state["tab"] == "ABC-DEF-123"

    def test_verdict_in_any_response(self):
        """Verdict tags in any response update last_verdict."""
        data = _post_payload(
            "some command",
            "Output with <verdict>MAJOR_RETHINK</verdict> tag.",
        )
        with pytest.raises(SystemExit):
            guard.handle_post_tool_use(data)

        state = guard.load_state("test-session")
        assert state["last_verdict"] == "MAJOR_RETHINK"


# ===================================================================
# Edge cases (acceptance criterion)
# ===================================================================


class TestEdgeCases:
    """Edge cases: done in comments, quoted strings, multi-line commands."""

    def test_done_in_comment_not_blocked(self):
        """'done' in a shell comment should not trigger flowctl done checks."""
        # This command has "done" but NOT as "flowctl done" or "FLOWCTL done"
        data = _pre_payload("echo 'task is done' # just a comment")
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 0

    def test_done_in_echo_not_blocked(self):
        """'done' as part of an echo statement (no flowctl context) is fine."""
        data = _pre_payload('echo "all tasks done successfully"')
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 0

    def test_codex_in_string_blocked(self):
        """'codex' word boundary match catches standalone usage."""
        # The guard uses \\bcodex\\b, so 'codex' as a standalone word matches
        data = _pre_payload('codex exec "check code"')
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 2

    def test_flowctl_done_in_multiline(self):
        """flowctl done in a multi-line command is still checked."""
        cmd = (
            "# First line\n"
            "flowctl done fn-1.1 --summary-file /tmp/s.md --evidence-json /tmp/e.json\n"
            "# Last line"
        )
        data = _pre_payload(cmd)
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 0  # Allowed because all flags present

    def test_flowctl_done_multiline_missing_evidence(self):
        """flowctl done in multi-line without evidence is blocked."""
        cmd = (
            "# First line\n"
            "flowctl done fn-1.1 --summary-file /tmp/s.md\n"
            "# Last line"
        )
        data = _pre_payload(cmd)
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 2

    def test_FLOWCTL_env_var_done(self):
        """$FLOWCTL done is still validated (environment variable invocation)."""
        data = _pre_payload('"$FLOWCTL" done fn-1.1 --summary-file /tmp/s.md')
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 2  # Missing --evidence-json

    def test_chat_send_json_in_message_body(self):
        """--json flag after chat-send is blocked even with surrounding text."""
        data = _pre_payload('rp chat-send --mode review --json --message "test"')
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 2

    def test_new_chat_allowed_when_zero_chats(self):
        """--new-chat is only blocked on re-reviews (chats_sent > 0)."""
        # chats_sent defaults to 0
        data = _pre_payload('rp chat-send --new-chat --message "first"')
        with pytest.raises(SystemExit) as exc:
            guard.handle_pre_tool_use(data)
        assert exc.value.code == 0


# ===================================================================
# Protected file checks
# ===================================================================


class TestProtectedFiles:
    """Block Edit/Write to protected workflow files."""

    def test_edit_ralph_guard_blocked(self):
        """Cannot edit ralph-guard.py."""
        data = _edit_payload("/path/to/scripts/hooks/ralph-guard.py")
        with pytest.raises(SystemExit) as exc:
            guard.handle_protected_file_check(data)
        assert exc.value.code == 2

    def test_edit_flowctl_py_blocked(self):
        """Cannot edit flowctl.py."""
        data = _edit_payload("/path/to/scripts/flowctl.py")
        with pytest.raises(SystemExit) as exc:
            guard.handle_protected_file_check(data)
        assert exc.value.code == 2

    def test_edit_hooks_json_blocked(self):
        """Cannot edit hooks/hooks.json."""
        data = _edit_payload("/path/to/hooks/hooks.json")
        with pytest.raises(SystemExit) as exc:
            guard.handle_protected_file_check(data)
        assert exc.value.code == 2

    def test_edit_flowctl_dir_blocked(self):
        """Cannot edit the flowctl directory itself (endswith /flowctl/)."""
        # The pattern "/flowctl/" matches paths ending with "/flowctl/"
        # (e.g., a directory path), not files inside it
        data = _edit_payload("/path/to/scripts/flowctl/")
        with pytest.raises(SystemExit) as exc:
            guard.handle_protected_file_check(data)
        assert exc.value.code == 2

    def test_edit_normal_file_allowed(self):
        """Editing a normal file is not blocked (no exit)."""
        data = _edit_payload("/path/to/src/main.py")
        # Should return None (no exit) since file is not protected
        result = guard.handle_protected_file_check(data)
        assert result is None


# ===================================================================
# parse_receipt_path
# ===================================================================


class TestParseReceiptPath:
    """Test receipt path parsing for type/id extraction."""

    def test_plan_receipt_legacy(self):
        """plan-fn-1.json -> (plan_review, fn-1)"""
        assert guard.parse_receipt_path("plan-fn-1.json") == ("plan_review", "fn-1")

    def test_impl_receipt_legacy(self):
        """impl-fn-1.2.json -> (impl_review, fn-1.2)"""
        assert guard.parse_receipt_path("impl-fn-1.2.json") == ("impl_review", "fn-1.2")

    def test_completion_receipt_legacy(self):
        """completion-fn-1.json -> (completion_review, fn-1)"""
        assert guard.parse_receipt_path("completion-fn-1.json") == (
            "completion_review",
            "fn-1",
        )

    def test_impl_receipt_with_slug(self):
        """impl-fn-4-flowctl-comprehensive-optimization-and.3.json -> (impl_review, fn-4-..-.3)"""
        rtype, rid = guard.parse_receipt_path(
            "impl-fn-4-flowctl-comprehensive-optimization-and.3.json"
        )
        assert rtype == "impl_review"
        assert rid.startswith("fn-4")
        assert rid.endswith(".3")

    def test_plan_receipt_with_slug(self):
        """plan-fn-4-flowctl-comprehensive.json"""
        rtype, rid = guard.parse_receipt_path(
            "plan-fn-4-flowctl-comprehensive.json"
        )
        assert rtype == "plan_review"
        assert "fn-4" in rid

    def test_unknown_format_fallback(self):
        """Unknown filename pattern returns fallback."""
        assert guard.parse_receipt_path("random-file.json") == ("impl_review", "UNKNOWN")


# ===================================================================
# State persistence
# ===================================================================


class TestStatePersistence:
    """Test state load/save round-trip and defaults."""

    def test_fresh_state_defaults(self):
        """Fresh state has expected defaults."""
        state = guard.load_state("fresh-session")
        assert state["chats_sent"] == 0
        assert state["last_verdict"] is None
        assert state["chat_send_succeeded"] is False
        assert state["codex_review_succeeded"] is False
        assert isinstance(state["flowctl_done_called"], set)
        assert len(state["flowctl_done_called"]) == 0
        # Clean up
        _reset_state("fresh-session")

    def test_round_trip_with_set(self):
        """State with set survives JSON round-trip."""
        state = guard.load_state("rt-session")
        state["flowctl_done_called"] = {"fn-1.1", "fn-1.2"}
        state["chat_send_succeeded"] = True
        guard.save_state("rt-session", state)

        loaded = guard.load_state("rt-session")
        assert loaded["chat_send_succeeded"] is True
        assert "fn-1.1" in loaded["flowctl_done_called"]
        assert "fn-1.2" in loaded["flowctl_done_called"]
        # Clean up
        sf = guard.get_state_file("rt-session")
        if sf.exists():
            sf.unlink()

    def test_corrupt_state_returns_defaults(self):
        """Corrupt state file returns default state."""
        state_file = guard.get_state_file("corrupt-session")
        state_file.write_text("not valid json{{{")
        state = guard.load_state("corrupt-session")
        assert state["chats_sent"] == 0
        assert state["chat_send_succeeded"] is False
        # Clean up
        if state_file.exists():
            state_file.unlink()


# ===================================================================
# Stop event
# ===================================================================


class TestStopEvent:
    """Test Stop handler behavior."""

    def test_stop_with_no_receipt_path(self):
        """Stop with no REVIEW_RECEIPT_PATH set exits cleanly."""
        data = _stop_payload()
        with patch.dict(os.environ, {}, clear=False):
            os.environ.pop("REVIEW_RECEIPT_PATH", None)
            with pytest.raises(SystemExit) as exc:
                guard.handle_stop(data)
            assert exc.value.code == 0

    def test_stop_hook_active_prevents_loop(self):
        """stop_hook_active=True exits immediately (infinite loop guard)."""
        data = _stop_payload(stop_hook_active=True)
        with pytest.raises(SystemExit) as exc:
            guard.handle_stop(data)
        assert exc.value.code == 0

    def test_stop_cleans_up_state_file(self):
        """Stop cleans up the session state file."""
        # Create state
        state = guard.load_state("test-session")
        state["chats_sent"] = 5
        guard.save_state("test-session", state)
        assert guard.get_state_file("test-session").exists()

        data = _stop_payload()
        with patch.dict(os.environ, {}, clear=False):
            os.environ.pop("REVIEW_RECEIPT_PATH", None)
            with pytest.raises(SystemExit):
                guard.handle_stop(data)

        assert not guard.get_state_file("test-session").exists()
