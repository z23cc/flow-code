"""Tests for flowctl.commands.review.codex_utils — pure function tests."""

import os

import pytest

from flowctl.commands.review.codex_utils import (
    is_sandbox_failure,
    parse_codex_verdict,
    resolve_codex_sandbox,
)


# --- parse_codex_verdict ---


class TestParseCodexVerdict:
    """Tests for parse_codex_verdict()."""

    def test_ship_verdict(self):
        output = "Review looks good.\n<verdict>SHIP</verdict>\n"
        assert parse_codex_verdict(output) == "SHIP"

    def test_needs_work_verdict(self):
        output = "Found issues.\n<verdict>NEEDS_WORK</verdict>\n"
        assert parse_codex_verdict(output) == "NEEDS_WORK"

    def test_major_rethink_verdict(self):
        output = "Fundamental problems.\n<verdict>MAJOR_RETHINK</verdict>\n"
        assert parse_codex_verdict(output) == "MAJOR_RETHINK"

    def test_no_verdict_returns_none(self):
        output = "Some review output without a verdict tag."
        assert parse_codex_verdict(output) is None

    def test_empty_output_returns_none(self):
        assert parse_codex_verdict("") is None

    def test_malformed_verdict_tag(self):
        """Verdict tag with wrong content should return None."""
        output = "<verdict>APPROVE</verdict>"
        assert parse_codex_verdict(output) is None

    def test_verdict_in_middle_of_text(self):
        """Verdict embedded in longer output."""
        output = (
            "## Review\n\nLooks great.\n\n"
            "### Summary\nAll checks pass.\n\n"
            "<verdict>SHIP</verdict>\n\n"
            "End of review."
        )
        assert parse_codex_verdict(output) == "SHIP"

    def test_multiple_verdicts_returns_first(self):
        """If multiple verdict tags exist, return the first one."""
        output = "<verdict>NEEDS_WORK</verdict>\nAfter fixes:\n<verdict>SHIP</verdict>"
        # re.search returns the first match
        assert parse_codex_verdict(output) == "NEEDS_WORK"

    def test_verdict_case_sensitive(self):
        """Verdict values are case-sensitive — lowercase should not match."""
        output = "<verdict>ship</verdict>"
        assert parse_codex_verdict(output) is None

    def test_partial_verdict_tag(self):
        """Incomplete verdict tag should not match."""
        output = "<verdict>SHIP"
        assert parse_codex_verdict(output) is None


# --- resolve_codex_sandbox ---


class TestResolveCodexSandbox:
    """Tests for resolve_codex_sandbox()."""

    def test_explicit_read_only(self):
        assert resolve_codex_sandbox("read-only") == "read-only"

    def test_explicit_danger_full_access(self):
        assert resolve_codex_sandbox("danger-full-access") == "danger-full-access"

    def test_explicit_workspace_write(self):
        assert resolve_codex_sandbox("workspace-write") == "workspace-write"

    def test_invalid_mode_raises(self):
        with pytest.raises(ValueError, match="Invalid sandbox value"):
            resolve_codex_sandbox("invalid-mode")

    def test_auto_resolves_on_unix(self, monkeypatch):
        """On unix (os.name != 'nt'), auto resolves to read-only."""
        monkeypatch.setattr(os, "name", "posix")
        monkeypatch.delenv("CODEX_SANDBOX", raising=False)
        assert resolve_codex_sandbox("auto") == "read-only"

    def test_auto_resolves_on_windows(self, monkeypatch):
        """On Windows (os.name == 'nt'), auto resolves to danger-full-access."""
        monkeypatch.setattr(os, "name", "nt")
        monkeypatch.delenv("CODEX_SANDBOX", raising=False)
        assert resolve_codex_sandbox("auto") == "danger-full-access"

    def test_env_var_overrides_auto(self, monkeypatch):
        """CODEX_SANDBOX env var should override auto resolution."""
        monkeypatch.setenv("CODEX_SANDBOX", "workspace-write")
        assert resolve_codex_sandbox("auto") == "workspace-write"

    def test_explicit_overrides_env(self, monkeypatch):
        """Explicit CLI value should override env var."""
        monkeypatch.setenv("CODEX_SANDBOX", "workspace-write")
        assert resolve_codex_sandbox("read-only") == "read-only"

    def test_invalid_env_var_raises(self, monkeypatch):
        """Invalid CODEX_SANDBOX env value should raise ValueError."""
        monkeypatch.setenv("CODEX_SANDBOX", "bad-value")
        with pytest.raises(ValueError, match="Invalid CODEX_SANDBOX value"):
            resolve_codex_sandbox("auto")

    def test_empty_string_treated_as_auto(self, monkeypatch):
        """Empty string should resolve like auto."""
        monkeypatch.setattr(os, "name", "posix")
        monkeypatch.delenv("CODEX_SANDBOX", raising=False)
        assert resolve_codex_sandbox("") == "read-only"

    def test_whitespace_stripped(self, monkeypatch):
        """Leading/trailing whitespace should be stripped."""
        assert resolve_codex_sandbox("  read-only  ") == "read-only"


# --- is_sandbox_failure ---


class TestIsSandboxFailure:
    """Tests for is_sandbox_failure()."""

    def test_success_exit_code_never_sandbox_failure(self):
        """Exit code 0 should never be detected as sandbox failure."""
        assert is_sandbox_failure(0, "", "blocked by policy") is False

    def test_blocked_by_policy_in_stderr(self):
        assert is_sandbox_failure(1, "", "Error: blocked by policy") is True

    def test_rejected_by_policy_in_stderr(self):
        assert is_sandbox_failure(1, "", "rejected by policy: read") is True

    def test_filesystem_read_blocked_in_stderr(self):
        assert is_sandbox_failure(1, "", "filesystem read is blocked") is True

    def test_filesystem_write_blocked_in_stderr(self):
        assert is_sandbox_failure(1, "", "filesystem write is blocked") is True

    def test_shell_command_blocked_in_stderr(self):
        assert is_sandbox_failure(1, "", "shell command was blocked") is True

    def test_appcontainer_in_stderr(self):
        assert is_sandbox_failure(1, "", "AppContainer restriction") is True

    def test_unrelated_error_not_sandbox(self):
        """Non-sandbox errors should return False."""
        assert is_sandbox_failure(1, "", "connection refused") is False

    def test_empty_stderr_not_sandbox(self):
        assert is_sandbox_failure(1, "", "") is False

    def test_json_stdout_failed_item_with_policy(self):
        """Failed item in JSON stdout with rejection message."""
        import json

        item = {
            "type": "item.completed",
            "item": {
                "status": "failed",
                "aggregated_output": "rejected by policy: filesystem read",
            },
        }
        stdout = json.dumps(item)
        assert is_sandbox_failure(1, stdout, "") is True

    def test_json_stdout_success_item_not_sandbox(self):
        """Successful item in JSON stdout should not be sandbox failure."""
        import json

        item = {
            "type": "item.completed",
            "item": {
                "status": "completed",
                "aggregated_output": "all good",
            },
        }
        stdout = json.dumps(item)
        assert is_sandbox_failure(1, stdout, "") is False

    def test_case_insensitive_pattern_matching(self):
        """Patterns should match case-insensitively."""
        assert is_sandbox_failure(1, "", "BLOCKED BY POLICY") is True
        assert is_sandbox_failure(1, "", "Blocked By Policy") is True
