"""Tests for flowctl.core.ids — ID parsing, generation, and validation."""

import pytest

from flowctl.core.ids import (
    epic_id_from_task,
    generate_epic_suffix,
    is_epic_id,
    is_task_id,
    normalize_epic,
    normalize_task,
    parse_id,
    slugify,
    task_priority,
)


# --- parse_id ---


class TestParseId:
    """Tests for parse_id() — the highest-risk function, used everywhere."""

    def test_legacy_epic(self):
        """fn-1 -> (1, None)"""
        assert parse_id("fn-1") == (1, None)

    def test_legacy_task(self):
        """fn-1.2 -> (1, 2)"""
        assert parse_id("fn-1.2") == (1, 2)

    def test_legacy_large_numbers(self):
        """fn-99.100 -> (99, 100)"""
        assert parse_id("fn-99.100") == (99, 100)

    def test_short_suffix_epic(self):
        """fn-5-x7k -> (5, None)"""
        assert parse_id("fn-5-x7k") == (5, None)

    def test_short_suffix_task(self):
        """fn-5-x7k.3 -> (5, 3)"""
        assert parse_id("fn-5-x7k.3") == (5, 3)

    def test_slug_suffix_epic(self):
        """fn-4-flowctl-comprehensive-optimization-and -> (4, None)"""
        assert parse_id("fn-4-flowctl-comprehensive-optimization-and") == (4, None)

    def test_slug_suffix_task(self):
        """fn-4-flowctl-comprehensive-optimization-and.1 -> (4, 1)"""
        assert parse_id("fn-4-flowctl-comprehensive-optimization-and.1") == (4, 1)

    def test_two_char_suffix(self):
        """fn-3-ab -> (3, None)"""
        assert parse_id("fn-3-ab") == (3, None)

    def test_single_char_suffix(self):
        """fn-3-a -> (3, None)"""
        assert parse_id("fn-3-a") == (3, None)

    def test_invalid_empty(self):
        assert parse_id("") == (None, None)

    def test_invalid_no_prefix(self):
        assert parse_id("task-1") == (None, None)

    def test_invalid_no_number(self):
        assert parse_id("fn-") == (None, None)

    def test_invalid_no_fn_prefix(self):
        assert parse_id("1.2") == (None, None)

    def test_invalid_uppercase(self):
        """Suffixes must be lowercase."""
        assert parse_id("fn-1-ABC") == (None, None)

    def test_invalid_special_chars(self):
        assert parse_id("fn-1-a_b") == (None, None)

    def test_invalid_double_dot(self):
        assert parse_id("fn-1.2.3") == (None, None)

    def test_zero_epic(self):
        """fn-0 should be valid (0 is a valid integer)."""
        assert parse_id("fn-0") == (0, None)

    def test_zero_task(self):
        """fn-0.0 should be valid."""
        assert parse_id("fn-0.0") == (0, 0)

    def test_suffix_with_digits(self):
        """fn-2-a3b -> (2, None)"""
        assert parse_id("fn-2-a3b") == (2, None)

    def test_long_slug_with_many_segments(self):
        """Multi-segment slug suffix with task number."""
        assert parse_id("fn-10-my-long-slug-name.5") == (10, 5)


# --- is_epic_id / is_task_id ---


class TestIsEpicId:
    def test_epic_id(self):
        assert is_epic_id("fn-1") is True

    def test_task_id_not_epic(self):
        assert is_epic_id("fn-1.2") is False

    def test_invalid_not_epic(self):
        assert is_epic_id("garbage") is False

    def test_slug_epic(self):
        assert is_epic_id("fn-4-flowctl-opt") is True


class TestIsTaskId:
    def test_task_id(self):
        assert is_task_id("fn-1.2") is True

    def test_epic_id_not_task(self):
        assert is_task_id("fn-1") is False

    def test_invalid_not_task(self):
        assert is_task_id("garbage") is False

    def test_slug_task(self):
        assert is_task_id("fn-4-opt.1") is True


# --- epic_id_from_task ---


class TestEpicIdFromTask:
    def test_legacy(self):
        assert epic_id_from_task("fn-1.2") == "fn-1"

    def test_with_suffix(self):
        assert epic_id_from_task("fn-5-x7k.3") == "fn-5-x7k"

    def test_with_slug(self):
        assert epic_id_from_task("fn-4-flowctl-opt.1") == "fn-4-flowctl-opt"

    def test_invalid_raises(self):
        with pytest.raises(ValueError):
            epic_id_from_task("fn-1")  # epic ID, not task

    def test_garbage_raises(self):
        with pytest.raises(ValueError):
            epic_id_from_task("garbage")


# --- slugify ---


class TestSlugify:
    def test_basic(self):
        assert slugify("Hello World") == "hello-world"

    def test_special_chars(self):
        assert slugify("Hello! @World#") == "hello-world"

    def test_underscores(self):
        assert slugify("my_cool_thing") == "my-cool-thing"

    def test_unicode(self):
        result = slugify("café résumé")
        assert result == "cafe-resume"

    def test_max_length(self):
        result = slugify("a" * 100, max_length=10)
        assert len(result) <= 10

    def test_max_length_word_boundary(self):
        result = slugify("hello-world-foo-bar", max_length=12)
        # Should truncate at word boundary
        assert len(result) <= 12
        assert not result.endswith("-")

    def test_empty_returns_none(self):
        assert slugify("!!!") is None

    def test_all_whitespace_returns_none(self):
        # After processing, should produce empty string
        assert slugify("   ") is None


# --- generate_epic_suffix ---


class TestGenerateEpicSuffix:
    def test_default_length(self):
        suffix = generate_epic_suffix()
        assert len(suffix) == 3

    def test_custom_length(self):
        suffix = generate_epic_suffix(length=5)
        assert len(suffix) == 5

    def test_valid_chars(self):
        """Suffix should only contain lowercase letters and digits."""
        import string

        valid = set(string.ascii_lowercase + string.digits)
        for _ in range(20):  # run multiple times for randomness
            suffix = generate_epic_suffix()
            assert all(c in valid for c in suffix)


# --- normalize_epic / normalize_task ---


class TestNormalizeEpic:
    def test_adds_missing_fields(self):
        data = {}
        result = normalize_epic(data)
        assert result["plan_review_status"] == "unknown"
        assert result["plan_reviewed_at"] is None
        assert result["completion_review_status"] == "unknown"
        assert result["completion_reviewed_at"] is None
        assert result["branch_name"] is None
        assert result["depends_on_epics"] == []
        assert result["default_impl"] is None
        assert result["default_review"] is None
        assert result["default_sync"] is None
        assert result["gaps"] == []

    def test_preserves_existing(self):
        data = {"plan_review_status": "ship", "branch_name": "my-branch"}
        result = normalize_epic(data)
        assert result["plan_review_status"] == "ship"
        assert result["branch_name"] == "my-branch"


class TestNormalizeTask:
    def test_adds_missing_fields(self):
        data = {}
        result = normalize_task(data)
        assert result["priority"] is None
        assert result["depends_on"] == []
        assert result["impl"] is None
        assert result["review"] is None
        assert result["sync"] is None

    def test_migrates_legacy_deps(self):
        data = {"deps": ["fn-1.1", "fn-1.2"]}
        result = normalize_task(data)
        assert result["depends_on"] == ["fn-1.1", "fn-1.2"]

    def test_depends_on_preferred_over_deps(self):
        data = {"depends_on": ["fn-1.3"], "deps": ["fn-1.1"]}
        result = normalize_task(data)
        assert result["depends_on"] == ["fn-1.3"]


# --- task_priority ---


class TestTaskPriority:
    def test_none_priority(self):
        assert task_priority({"priority": None}) == 999

    def test_missing_priority(self):
        assert task_priority({}) == 999

    def test_numeric_priority(self):
        assert task_priority({"priority": 1}) == 1

    def test_string_priority(self):
        assert task_priority({"priority": "5"}) == 5

    def test_invalid_priority(self):
        assert task_priority({"priority": "not-a-number"}) == 999
