"""Tests for flowctl.core.io — atomic writes, JSON loading, output formatting."""

import json
import os

import pytest

from flowctl.core.io import (
    atomic_write,
    atomic_write_json,
    is_supported_schema,
    load_json,
    now_iso,
)


class TestAtomicWrite:
    """Tests for atomic_write() — temp + rename pattern."""

    def test_creates_file(self, tmp_path):
        """atomic_write should create a new file with correct content."""
        target = tmp_path / "output.txt"
        atomic_write(target, "hello world\n")
        assert target.exists()
        assert target.read_text(encoding="utf-8") == "hello world\n"

    def test_overwrites_existing(self, tmp_path):
        """atomic_write should overwrite existing file."""
        target = tmp_path / "output.txt"
        target.write_text("old content", encoding="utf-8")
        atomic_write(target, "new content")
        assert target.read_text(encoding="utf-8") == "new content"

    def test_creates_parent_dirs(self, tmp_path):
        """atomic_write should create parent directories if missing."""
        target = tmp_path / "a" / "b" / "c" / "output.txt"
        atomic_write(target, "deep file")
        assert target.read_text(encoding="utf-8") == "deep file"

    def test_cleans_up_on_error(self, tmp_path):
        """atomic_write should remove temp file if write fails."""
        target = tmp_path / "output.txt"

        class BadStr:
            """Object that raises on write."""

            def __str__(self):
                raise RuntimeError("boom")

        # This should raise because we can't write a non-string
        with pytest.raises(TypeError):
            atomic_write(target, BadStr())  # type: ignore

        # No temp files should remain
        remaining = list(tmp_path.glob("*.tmp"))
        assert remaining == [], f"Temp files not cleaned up: {remaining}"

    def test_atomic_write_unicode(self, tmp_path):
        """atomic_write should handle unicode content."""
        target = tmp_path / "unicode.txt"
        text = "Hello \u00e9\u00e8\u00ea \u2603 \U0001f600"
        atomic_write(target, text)
        content = target.read_text(encoding="utf-8")
        assert "\u00e9" in content
        assert "\u2603" in content
        assert "\U0001f600" in content


class TestAtomicWriteJson:
    """Tests for atomic_write_json() — sorted keys, trailing newline."""

    def test_roundtrip(self, tmp_path):
        """Write JSON and load it back."""
        target = tmp_path / "data.json"
        data = {"b": 2, "a": 1, "nested": {"x": True}}
        atomic_write_json(target, data)

        loaded = load_json(target)
        assert loaded == data

    def test_sorted_keys(self, tmp_path):
        """Keys should be sorted in output."""
        target = tmp_path / "data.json"
        atomic_write_json(target, {"z": 1, "a": 2, "m": 3})
        raw = target.read_text(encoding="utf-8")
        # 'a' should appear before 'm' which should appear before 'z'
        assert raw.index('"a"') < raw.index('"m"') < raw.index('"z"')

    def test_trailing_newline(self, tmp_path):
        """JSON output should end with newline."""
        target = tmp_path / "data.json"
        atomic_write_json(target, {"key": "value"})
        raw = target.read_text(encoding="utf-8")
        assert raw.endswith("\n")


class TestLoadJson:
    """Tests for load_json() — basic JSON file loading."""

    def test_load_valid(self, tmp_path):
        target = tmp_path / "valid.json"
        target.write_text('{"key": "value"}', encoding="utf-8")
        result = load_json(target)
        assert result == {"key": "value"}

    def test_load_invalid_raises(self, tmp_path):
        target = tmp_path / "invalid.json"
        target.write_text("not json", encoding="utf-8")
        with pytest.raises(json.JSONDecodeError):
            load_json(target)

    def test_load_missing_raises(self, tmp_path):
        target = tmp_path / "missing.json"
        with pytest.raises(FileNotFoundError):
            load_json(target)


class TestNowIso:
    """Tests for now_iso() — ISO timestamp generation."""

    def test_format(self):
        result = now_iso()
        assert result.endswith("Z")
        # Should be parseable as ISO format
        assert "T" in result

    def test_contains_date_parts(self):
        result = now_iso()
        # Should contain year-month-day
        parts = result.split("T")
        assert len(parts) == 2
        date_parts = parts[0].split("-")
        assert len(date_parts) == 3  # year, month, day


class TestIsSupportedSchema:
    """Tests for is_supported_schema() — version checking."""

    def test_version_1(self):
        assert is_supported_schema(1) is True

    def test_version_2(self):
        assert is_supported_schema(2) is True

    def test_version_string(self):
        """String versions should also work via int conversion."""
        assert is_supported_schema("1") is True
        assert is_supported_schema("2") is True

    def test_unsupported_version(self):
        assert is_supported_schema(99) is False

    def test_invalid_type(self):
        assert is_supported_schema("abc") is False
        assert is_supported_schema(None) is False
