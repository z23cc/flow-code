"""Tests for worker-phase pure functions in flowctl.commands.workflow."""

import pytest

from flowctl.commands.workflow import (
    _build_bootstrap_prompt,
    _build_phase_sequence,
    _estimate_tokens,
    _extract_phase_content,
    _parse_worker_sections,
)
from flowctl.core.constants import (
    PHASE_SEQ_DEFAULT,
    PHASE_SEQ_REVIEW,
    PHASE_SEQ_TDD,
)


# --- _parse_worker_sections ---


class TestParseWorkerSections:
    """Tests for _parse_worker_sections — HTML comment marker parsing."""

    def test_single_section(self):
        content = "<!-- section:core -->\nHello world\n<!-- /section:core -->"
        result = _parse_worker_sections(content)
        assert len(result) == 1
        assert result[0] == {"tag": "core", "content": "Hello world"}

    def test_multiple_sections(self):
        content = (
            "<!-- section:core -->\nCore content\n<!-- /section:core -->\n"
            "<!-- section:team -->\nTeam content\n<!-- /section:team -->\n"
            "<!-- section:tdd -->\nTDD content\n<!-- /section:tdd -->"
        )
        result = _parse_worker_sections(content)
        assert len(result) == 3
        assert [s["tag"] for s in result] == ["core", "team", "tdd"]
        assert result[0]["content"] == "Core content"
        assert result[1]["content"] == "Team content"
        assert result[2]["content"] == "TDD content"

    def test_empty_section_skipped(self):
        content = (
            "<!-- section:core -->\nSome text\n<!-- /section:core -->\n"
            "<!-- section:empty -->\n   \n<!-- /section:empty -->"
        )
        result = _parse_worker_sections(content)
        assert len(result) == 1
        assert result[0]["tag"] == "core"

    def test_frontmatter_discarded(self):
        content = (
            "---\ntitle: worker\n---\n\n"
            "<!-- section:core -->\nBody\n<!-- /section:core -->"
        )
        result = _parse_worker_sections(content)
        assert len(result) == 1
        assert result[0]["content"] == "Body"

    def test_no_sections(self):
        result = _parse_worker_sections("Just plain text with no markers")
        assert result == []


# --- _extract_phase_content ---


class TestExtractPhaseContent:
    """Tests for _extract_phase_content — phase heading extraction with tag filtering."""

    SAMPLE_CONTENT = (
        "<!-- section:core -->\n"
        "## Phase 1: Read\n"
        "Read the spec.\n\n"
        "## Phase 2: Implement\n"
        "Write the code.\n"
        "<!-- /section:core -->\n"
        "<!-- section:tdd -->\n"
        "## Phase 2a: Write Tests First\n"
        "Tests before code.\n"
        "<!-- /section:tdd -->\n"
        "<!-- section:review -->\n"
        "## Phase 4: Review\n"
        "Run review.\n"
        "<!-- /section:review -->"
    )

    def test_core_only(self):
        result = _extract_phase_content(self.SAMPLE_CONTENT, {"core"})
        assert "1" in result
        assert "2" in result
        assert "2a" not in result
        assert "4" not in result

    def test_core_plus_tdd(self):
        result = _extract_phase_content(self.SAMPLE_CONTENT, {"core", "tdd"})
        assert "1" in result
        assert "2" in result
        assert "2a" in result
        assert "4" not in result

    def test_all_tags(self):
        result = _extract_phase_content(self.SAMPLE_CONTENT, {"core", "tdd", "review"})
        assert "1" in result
        assert "2" in result
        assert "2a" in result
        assert "4" in result

    def test_no_matching_tags(self):
        result = _extract_phase_content(self.SAMPLE_CONTENT, {"memory"})
        assert result == {}

    def test_phase_content_preserved(self):
        result = _extract_phase_content(self.SAMPLE_CONTENT, {"core"})
        assert "Read the spec." in result["1"]
        assert "Write the code." in result["2"]


# --- _build_phase_sequence ---


class TestBuildPhaseSequence:
    """Tests for _build_phase_sequence — mode combination logic."""

    def test_default(self):
        seq = _build_phase_sequence()
        assert seq == list(PHASE_SEQ_DEFAULT)
        assert "5b" in seq, "Phase 5b must be in default sequence"

    def test_tdd_only(self):
        seq = _build_phase_sequence(tdd=True)
        assert "2a" in seq, "TDD adds Phase 2a"
        assert seq == [p for p in ["0", "1", "2a", "2", "2.5", "3", "5", "5b", "6"] if p in seq]

    def test_review_only(self):
        seq = _build_phase_sequence(review=True)
        assert "4" in seq, "Review adds Phase 4"
        assert "2a" not in seq, "Review alone does not add 2a"

    def test_tdd_and_review(self):
        seq = _build_phase_sequence(tdd=True, review=True)
        assert "2a" in seq
        assert "4" in seq
        # Canonical ordering preserved
        assert seq.index("2a") < seq.index("2")
        assert seq.index("4") < seq.index("5")

    def test_default_matches_constant(self):
        """Default sequence must match PHASE_SEQ_DEFAULT exactly."""
        assert _build_phase_sequence(tdd=False, review=False) == list(PHASE_SEQ_DEFAULT)

    def test_extra_kwargs_ignored(self):
        """Extra keyword arguments are accepted and ignored."""
        seq = _build_phase_sequence(tdd=False, review=False, ralph=True)
        assert seq == list(PHASE_SEQ_DEFAULT)


# --- _build_bootstrap_prompt ---


class TestBuildBootstrapPrompt:
    """Tests for _build_bootstrap_prompt — minimal worker prompt generation."""

    def test_basic_structure(self):
        prompt = _build_bootstrap_prompt(
            task_id="fn-1.2",
            epic_id="fn-1",
            flowctl_path="/path/to/flowctl.py",
        )
        assert "TASK_ID: fn-1.2" in prompt
        assert "EPIC_ID: fn-1" in prompt
        assert "FLOWCTL: /path/to/flowctl.py" in prompt
        assert "REVIEW_MODE: none" in prompt
        assert "RALPH_MODE: false" in prompt
        assert "TDD_MODE: false" in prompt
        assert "TEAM_MODE" not in prompt, "team=False should omit TEAM_MODE"

    def test_team_mode(self):
        prompt = _build_bootstrap_prompt(
            task_id="fn-1.2",
            epic_id="fn-1",
            flowctl_path="/path/to/flowctl.py",
            team=True,
        )
        assert "TEAM_MODE: true" in prompt

    def test_no_team_flag_in_command(self):
        """Bootstrap prompt must NOT emit --team flag (bug #1 from audit)."""
        prompt = _build_bootstrap_prompt(
            task_id="fn-1.2",
            epic_id="fn-1",
            flowctl_path="/path/to/flowctl.py",
            team=True,
        )
        assert "--team" not in prompt

    def test_tdd_mode_flags(self):
        prompt = _build_bootstrap_prompt(
            task_id="fn-1.2",
            epic_id="fn-1",
            flowctl_path="/path/to/flowctl.py",
            tdd=True,
        )
        assert "--tdd" in prompt
        assert "TDD_MODE: true" in prompt

    def test_review_mode_flags(self):
        prompt = _build_bootstrap_prompt(
            task_id="fn-1.2",
            epic_id="fn-1",
            flowctl_path="/path/to/flowctl.py",
            review="rp",
        )
        assert "--review rp" in prompt
        assert "REVIEW_MODE: rp" in prompt

    def test_ralph_mode(self):
        prompt = _build_bootstrap_prompt(
            task_id="fn-1.2",
            epic_id="fn-1",
            flowctl_path="/path/to/flowctl.py",
            ralph=True,
        )
        assert "RALPH_MODE: true" in prompt


# --- _estimate_tokens ---


class TestEstimateTokens:
    """Tests for _estimate_tokens — rough word-based token estimation."""

    def test_empty_string(self):
        assert _estimate_tokens("") == 1, "Minimum is 1 token"

    def test_single_word(self):
        result = _estimate_tokens("hello")
        assert result == 1  # 1 * 1.3 = 1.3 -> int(1.3) = 1

    def test_ten_words(self):
        text = " ".join(["word"] * 10)
        result = _estimate_tokens(text)
        assert result == 13  # 10 * 1.3 = 13

    def test_proportional(self):
        short = _estimate_tokens("one two three")
        long = _estimate_tokens("one two three four five six seven eight nine ten")
        assert long > short
