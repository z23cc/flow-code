"""Tests for flowctl.commands.admin — validate_task_spec_headings."""

import pytest

from flowctl.commands.admin import validate_task_spec_headings


VALID_SPEC = """\
# fn-1.1 Some task title

## Description
This is the description.

## Acceptance
- [ ] Criterion A
- [ ] Criterion B

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
"""


class TestValidateTaskSpecHeadings:
    """Tests for validate_task_spec_headings()."""

    def test_valid_spec_no_errors(self):
        """A well-formed spec should produce zero errors."""
        errors = validate_task_spec_headings(VALID_SPEC)
        assert errors == []

    def test_missing_description(self):
        """Spec without ## Description should report missing heading."""
        content = VALID_SPEC.replace("## Description\n", "")
        errors = validate_task_spec_headings(content)
        assert any("Missing required heading: ## Description" in e for e in errors)

    def test_missing_acceptance(self):
        """Spec without ## Acceptance should report missing heading."""
        content = VALID_SPEC.replace("## Acceptance\n", "")
        errors = validate_task_spec_headings(content)
        assert any("Missing required heading: ## Acceptance" in e for e in errors)

    def test_missing_done_summary(self):
        """Spec without ## Done summary should report missing heading."""
        content = VALID_SPEC.replace("## Done summary\n", "")
        errors = validate_task_spec_headings(content)
        assert any("Missing required heading: ## Done summary" in e for e in errors)

    def test_missing_evidence(self):
        """Spec without ## Evidence should report missing heading."""
        content = VALID_SPEC.replace("## Evidence\n", "")
        errors = validate_task_spec_headings(content)
        assert any("Missing required heading: ## Evidence" in e for e in errors)

    def test_missing_all_headings(self):
        """Spec with no headings at all should report 4 missing."""
        content = "# Title\n\nJust some text, no sections.\n"
        errors = validate_task_spec_headings(content)
        assert len(errors) == 4
        assert all("Missing required heading" in e for e in errors)

    def test_duplicate_description(self):
        """Spec with duplicate ## Description should report duplicate."""
        content = VALID_SPEC + "\n## Description\nAnother description.\n"
        errors = validate_task_spec_headings(content)
        assert any("Duplicate heading: ## Description" in e for e in errors)

    def test_duplicate_acceptance(self):
        """Spec with duplicate ## Acceptance should report duplicate."""
        content = VALID_SPEC + "\n## Acceptance\n- [ ] Extra\n"
        errors = validate_task_spec_headings(content)
        assert any("Duplicate heading: ## Acceptance" in e for e in errors)

    def test_heading_inside_code_block_ignored(self):
        """Headings inside fenced code blocks should be ignored."""
        content = VALID_SPEC + "\n```\n## Description\n```\n"
        errors = validate_task_spec_headings(content)
        # Fenced code blocks are stripped before checking — no duplicate
        assert errors == []

    def test_heading_inside_tagged_code_block_ignored(self):
        """Headings inside fenced code blocks with language tags should be ignored."""
        content = VALID_SPEC + "\n```markdown\n## Description\n## Acceptance\n```\n"
        errors = validate_task_spec_headings(content)
        assert errors == []

    def test_heading_with_trailing_whitespace(self):
        """Headings with trailing whitespace should still be recognized."""
        content = VALID_SPEC.replace("## Description\n", "## Description   \n")
        errors = validate_task_spec_headings(content)
        assert errors == []

    def test_empty_content(self):
        """Empty content should report all headings as missing."""
        errors = validate_task_spec_headings("")
        assert len(errors) == 4

    def test_similar_heading_not_confused(self):
        """## Descriptions (plural) should NOT satisfy ## Description."""
        content = VALID_SPEC.replace("## Description\n", "## Descriptions\n")
        errors = validate_task_spec_headings(content)
        assert any("Missing required heading: ## Description" in e for e in errors)
