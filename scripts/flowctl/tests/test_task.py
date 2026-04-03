"""Tests for flowctl.commands.task — patch_task_section."""

import pytest

from flowctl.commands.task import patch_task_section


SAMPLE_SPEC = """\
# fn-1.1 Some task

## Description
Old description content.

## Acceptance
- [ ] Criterion A

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:"""


class TestPatchTaskSection:
    """Tests for patch_task_section()."""

    def test_replace_description(self):
        """Replacing ## Description content should preserve other sections."""
        result = patch_task_section(SAMPLE_SPEC, "## Description", "New description.")
        assert "New description." in result
        assert "Old description content." not in result
        # Other sections preserved
        assert "## Acceptance" in result
        assert "Criterion A" in result
        assert "## Done summary" in result
        assert "## Evidence" in result

    def test_replace_acceptance(self):
        """Replacing ## Acceptance content."""
        result = patch_task_section(
            SAMPLE_SPEC, "## Acceptance", "- [ ] New criterion"
        )
        assert "- [ ] New criterion" in result
        assert "Criterion A" not in result

    def test_replace_done_summary(self):
        """Replacing ## Done summary content."""
        result = patch_task_section(
            SAMPLE_SPEC, "## Done summary", "Implemented the feature."
        )
        assert "Implemented the feature." in result
        assert "\nTBD\n" not in result

    def test_add_missing_section(self):
        """Patching a section that doesn't exist should auto-append it."""
        content = "# Title\n\n## Description\nSome text.\n"
        result = patch_task_section(content, "## New Section", "New content here.")
        assert "## New Section" in result
        assert "New content here." in result
        # Original content preserved
        assert "## Description" in result
        assert "Some text." in result

    def test_duplicate_heading_raises(self):
        """Patching when target heading appears multiple times should raise ValueError."""
        content = SAMPLE_SPEC + "\n\n## Description\nDuplicate."
        with pytest.raises(ValueError, match="duplicate heading"):
            patch_task_section(content, "## Description", "New text")

    def test_strips_heading_from_new_content(self):
        """If new_content starts with the section heading, it should be stripped."""
        result = patch_task_section(
            SAMPLE_SPEC, "## Description", "## Description\nContent after heading."
        )
        # Should not have double heading
        lines = result.split("\n")
        desc_count = sum(1 for l in lines if l.strip() == "## Description")
        assert desc_count == 1
        assert "Content after heading." in result

    def test_multiline_replacement(self):
        """Multi-line content should be preserved correctly."""
        new_content = "Line 1\nLine 2\nLine 3"
        result = patch_task_section(SAMPLE_SPEC, "## Description", new_content)
        assert "Line 1" in result
        assert "Line 2" in result
        assert "Line 3" in result

    def test_empty_replacement(self):
        """Replacing with empty content should clear the section body."""
        result = patch_task_section(SAMPLE_SPEC, "## Description", "")
        # Section heading still present, but body cleared
        assert "## Description" in result
        # Next section should follow
        assert "## Acceptance" in result

    def test_preserves_title_line(self):
        """The title line (# fn-1.1 ...) should be preserved."""
        result = patch_task_section(SAMPLE_SPEC, "## Description", "New desc.")
        assert result.startswith("# fn-1.1 Some task")

    def test_trailing_whitespace_stripped(self):
        """New content trailing whitespace should be stripped."""
        result = patch_task_section(
            SAMPLE_SPEC, "## Description", "Content with trailing   \n\n\n"
        )
        # Check that trailing newlines from new_content are stripped
        desc_idx = result.index("## Description")
        acc_idx = result.index("## Acceptance")
        between = result[desc_idx:acc_idx]
        # Should not have excessive trailing newlines
        assert not between.endswith("\n\n\n\n")
