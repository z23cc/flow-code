"""Tests for flowctl.commands.workflow — start, done, ready, next, restart, block."""

import argparse
import json
import os

import pytest

from flowctl.commands.workflow import (
    cmd_block,
    cmd_done,
    cmd_next,
    cmd_ready,
    cmd_restart,
    cmd_start,
)
from flowctl.core.state import load_task_with_state, save_task_runtime


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _create_epic(flow_dir, epic_id, *, status="open", title="Test Epic",
                 plan_review_status="unknown", depends_on_epics=None,
                 completion_review_status="unknown"):
    """Create an epic JSON file."""
    epic_data = {
        "id": epic_id,
        "title": title,
        "status": status,
        "plan_review_status": plan_review_status,
        "completion_review_status": completion_review_status,
        "depends_on_epics": depends_on_epics or [],
    }
    epic_path = flow_dir / "epics" / f"{epic_id}.json"
    epic_path.write_text(json.dumps(epic_data, indent=2) + "\n", encoding="utf-8")
    return epic_path


def _create_task(flow_dir, task_id, *, epic, title="Test Task", status="todo",
                 depends_on=None, priority=None, files=None, domain=None):
    """Create a task JSON file and a minimal spec .md file."""
    task_data = {
        "id": task_id,
        "title": title,
        "epic": epic,
        "status": status,
        "depends_on": depends_on or [],
    }
    if priority is not None:
        task_data["priority"] = priority
    if files is not None:
        task_data["files"] = files
    if domain is not None:
        task_data["domain"] = domain

    task_path = flow_dir / "tasks" / f"{task_id}.json"
    task_path.write_text(json.dumps(task_data, indent=2) + "\n", encoding="utf-8")

    # Create minimal spec markdown
    spec_path = flow_dir / "tasks" / f"{task_id}.md"
    spec_path.write_text(
        f"# {task_id} {title}\n\n"
        "## Description\nTask description.\n\n"
        "## Acceptance\n- [ ] Criterion A\n\n"
        "## Done summary\nTBD\n\n"
        "## Evidence\n- Commits:\n- Tests:\n- PRs:\n",
        encoding="utf-8",
    )
    return task_path


def _ns(**kwargs):
    """Build an argparse.Namespace with sensible defaults."""
    defaults = {"json": True, "force": False}
    defaults.update(kwargs)
    return argparse.Namespace(**defaults)


# ---------------------------------------------------------------------------
# Fixture: populated epic with 3 tasks and dependencies
# ---------------------------------------------------------------------------


@pytest.fixture
def populated_epic(git_repo):
    """Create an epic fn-1 with 3 tasks: .1 (no deps), .2 (depends on .1), .3 (depends on .2)."""
    flow_dir = git_repo / ".flow"
    _create_epic(flow_dir, "fn-1")
    _create_task(flow_dir, "fn-1.1", epic="fn-1", title="First task")
    _create_task(flow_dir, "fn-1.2", epic="fn-1", title="Second task", depends_on=["fn-1.1"])
    _create_task(flow_dir, "fn-1.3", epic="fn-1", title="Third task", depends_on=["fn-1.2"])

    # Set up state dir for runtime state
    state_dir = git_repo / ".git" / "flow-state"
    state_dir.mkdir(parents=True, exist_ok=True)
    os.environ["FLOW_STATE_DIR"] = str(state_dir)
    os.environ["FLOW_ACTOR"] = "test@test.com"

    yield flow_dir

    # Cleanup env
    os.environ.pop("FLOW_STATE_DIR", None)
    os.environ.pop("FLOW_ACTOR", None)


# ===========================================================================
# cmd_start tests (>=5 cases)
# ===========================================================================


class TestCmdStart:
    """Tests for cmd_start — claiming and starting tasks."""

    def test_start_todo_task(self, populated_epic, capsys):
        """Starting a todo task should set it to in_progress."""
        cmd_start(_ns(id="fn-1.1", note=None))
        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "in_progress"

        task = load_task_with_state("fn-1.1")
        assert task["status"] == "in_progress"
        assert task["assignee"] == "test@test.com"

    def test_start_sets_claimed_at(self, populated_epic):
        """Starting a task should set claimed_at timestamp."""
        cmd_start(_ns(id="fn-1.1", note=None))
        task = load_task_with_state("fn-1.1")
        assert "claimed_at" in task
        assert task["claimed_at"] is not None

    def test_start_with_note(self, populated_epic, capsys):
        """Starting a task with --note should store the claim note."""
        cmd_start(_ns(id="fn-1.1", note="Taking this one"))
        task = load_task_with_state("fn-1.1")
        assert task.get("claim_note") == "Taking this one"

    def test_start_already_done_fails(self, populated_epic, capsys):
        """Starting a done task should fail."""
        # First, start and complete fn-1.1
        save_task_runtime("fn-1.1", {"status": "done", "assignee": "test@test.com"})

        with pytest.raises(SystemExit):
            cmd_start(_ns(id="fn-1.1", note=None))

    def test_start_blocked_task_fails(self, populated_epic):
        """Starting a blocked task should fail without --force."""
        save_task_runtime("fn-1.1", {"status": "blocked"})

        with pytest.raises(SystemExit):
            cmd_start(_ns(id="fn-1.1", note=None))

    def test_start_blocked_task_force(self, populated_epic, capsys):
        """Starting a blocked task with --force should succeed."""
        save_task_runtime("fn-1.1", {"status": "blocked"})
        cmd_start(_ns(id="fn-1.1", note=None, force=True))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "in_progress"

    def test_start_unmet_dependency_fails(self, populated_epic):
        """Starting a task with unmet dependencies should fail."""
        # fn-1.2 depends on fn-1.1 which is still todo
        with pytest.raises(SystemExit):
            cmd_start(_ns(id="fn-1.2", note=None))

    def test_start_met_dependency_succeeds(self, populated_epic, capsys):
        """Starting a task whose deps are done should succeed."""
        save_task_runtime("fn-1.1", {"status": "done"})
        cmd_start(_ns(id="fn-1.2", note=None))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "in_progress"

    def test_start_claimed_by_other_fails(self, populated_epic):
        """Starting a task claimed by someone else should fail."""
        save_task_runtime("fn-1.1", {"status": "in_progress", "assignee": "other@test.com"})

        with pytest.raises(SystemExit):
            cmd_start(_ns(id="fn-1.1", note=None))

    def test_start_resume_own_in_progress(self, populated_epic, capsys):
        """Re-starting your own in_progress task should succeed (resume)."""
        save_task_runtime("fn-1.1", {"status": "in_progress", "assignee": "test@test.com"})
        cmd_start(_ns(id="fn-1.1", note=None))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "in_progress"

    def test_start_invalid_task_id_fails(self, populated_epic):
        """Starting with an invalid task ID should fail."""
        with pytest.raises(SystemExit):
            cmd_start(_ns(id="not-a-task", note=None))


# ===========================================================================
# cmd_done tests (>=5 cases)
# ===========================================================================


class TestCmdDone:
    """Tests for cmd_done — completing tasks with evidence."""

    def test_done_in_progress_task(self, populated_epic, capsys):
        """Completing an in_progress task should set status to done."""
        save_task_runtime("fn-1.1", {"status": "in_progress", "assignee": "test@test.com"})

        cmd_done(_ns(
            id="fn-1.1",
            summary="Implemented feature",
            summary_file=None,
            evidence=None,
            evidence_json=None,
            force=False,
        ))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "done"
        assert out["id"] == "fn-1.1"

    def test_done_requires_in_progress(self, populated_epic):
        """Completing a todo task should fail without --force."""
        with pytest.raises(SystemExit):
            cmd_done(_ns(
                id="fn-1.1",
                summary="Done",
                summary_file=None,
                evidence=None,
                evidence_json=None,
                force=False,
            ))

    def test_done_already_done_fails(self, populated_epic):
        """Completing a task that is already done should fail."""
        save_task_runtime("fn-1.1", {"status": "done"})

        with pytest.raises(SystemExit):
            cmd_done(_ns(
                id="fn-1.1",
                summary="Done again",
                summary_file=None,
                evidence=None,
                evidence_json=None,
                force=False,
            ))

    def test_done_with_evidence_json_inline(self, populated_epic, capsys):
        """Completing with inline evidence JSON should parse and store it."""
        save_task_runtime("fn-1.1", {"status": "in_progress", "assignee": "test@test.com"})

        evidence = json.dumps({"commits": ["abc123"], "tests": ["pytest"], "prs": []})
        cmd_done(_ns(
            id="fn-1.1",
            summary="Done",
            summary_file=None,
            evidence=None,
            evidence_json=evidence,
            force=False,
        ))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "done"

    def test_done_with_evidence_file(self, populated_epic, capsys, tmp_path):
        """Completing with evidence from a file should work."""
        save_task_runtime("fn-1.1", {"status": "in_progress", "assignee": "test@test.com"})

        evidence_file = tmp_path / "evidence.json"
        evidence_file.write_text(
            json.dumps({"commits": ["def456"], "tests": [], "prs": []}),
            encoding="utf-8",
        )

        cmd_done(_ns(
            id="fn-1.1",
            summary="Done",
            summary_file=None,
            evidence=None,
            evidence_json=str(evidence_file),
            force=False,
        ))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "done"

    def test_done_updates_spec_file(self, populated_epic):
        """Done should update the ## Done summary and ## Evidence sections in the spec."""
        save_task_runtime("fn-1.1", {"status": "in_progress", "assignee": "test@test.com"})

        cmd_done(_ns(
            id="fn-1.1",
            summary="Implemented the feature successfully",
            summary_file=None,
            evidence=None,
            evidence_json=json.dumps({"commits": ["abc"], "tests": ["pytest"], "prs": []}),
            force=False,
        ))

        spec_path = populated_epic / "tasks" / "fn-1.1.md"
        content = spec_path.read_text(encoding="utf-8")
        assert "Implemented the feature successfully" in content
        assert "abc" in content

    def test_done_with_summary_file(self, populated_epic, capsys, tmp_path):
        """Completing with summary from a file should work."""
        save_task_runtime("fn-1.1", {"status": "in_progress", "assignee": "test@test.com"})

        summary_file = tmp_path / "summary.md"
        summary_file.write_text("Summary from file", encoding="utf-8")

        cmd_done(_ns(
            id="fn-1.1",
            summary=None,
            summary_file=str(summary_file),
            evidence=None,
            evidence_json=None,
            force=False,
        ))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "done"

    def test_done_force_skips_status_check(self, populated_epic, capsys):
        """Done with --force should allow completing a todo task."""
        cmd_done(_ns(
            id="fn-1.1",
            summary="Force done",
            summary_file=None,
            evidence=None,
            evidence_json=None,
            force=True,
        ))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "done"

    def test_done_cross_actor_fails(self, populated_epic):
        """Completing a task claimed by someone else should fail."""
        save_task_runtime("fn-1.1", {"status": "in_progress", "assignee": "other@test.com"})

        with pytest.raises(SystemExit):
            cmd_done(_ns(
                id="fn-1.1",
                summary="Done",
                summary_file=None,
                evidence=None,
                evidence_json=None,
                force=False,
            ))

    def test_done_calculates_duration(self, populated_epic, capsys):
        """Done should calculate duration from claimed_at."""
        from flowctl.core.io import now_iso
        save_task_runtime("fn-1.1", {
            "status": "in_progress",
            "assignee": "test@test.com",
            "claimed_at": now_iso(),
        })

        cmd_done(_ns(
            id="fn-1.1",
            summary="Done",
            summary_file=None,
            evidence=None,
            evidence_json=None,
            force=False,
        ))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "done"
        assert "duration_seconds" in out


# ===========================================================================
# cmd_ready tests (>=3 cases)
# ===========================================================================


class TestCmdReady:
    """Tests for cmd_ready — listing ready tasks for an epic."""

    def test_ready_returns_unblocked_tasks(self, populated_epic, capsys):
        """Ready should return tasks with no unmet dependencies."""
        cmd_ready(_ns(epic="fn-1"))

        out = json.loads(capsys.readouterr().out)
        ready_ids = [t["id"] for t in out["ready"]]
        assert "fn-1.1" in ready_ids
        # fn-1.2 depends on fn-1.1 (todo), so not ready
        assert "fn-1.2" not in ready_ids

    def test_ready_after_dep_done(self, populated_epic, capsys):
        """After a dep is done, the dependent task should appear in ready."""
        save_task_runtime("fn-1.1", {"status": "done"})

        cmd_ready(_ns(epic="fn-1"))

        out = json.loads(capsys.readouterr().out)
        ready_ids = [t["id"] for t in out["ready"]]
        assert "fn-1.2" in ready_ids
        # fn-1.1 is done, not in ready
        assert "fn-1.1" not in ready_ids

    def test_ready_skipped_counts_as_done(self, populated_epic, capsys):
        """A skipped dependency should count as satisfied."""
        save_task_runtime("fn-1.1", {"status": "skipped"})

        cmd_ready(_ns(epic="fn-1"))

        out = json.loads(capsys.readouterr().out)
        ready_ids = [t["id"] for t in out["ready"]]
        assert "fn-1.2" in ready_ids

    def test_ready_shows_in_progress_separately(self, populated_epic, capsys):
        """In-progress tasks should appear in the in_progress list, not ready."""
        save_task_runtime("fn-1.1", {"status": "in_progress", "assignee": "test@test.com"})

        cmd_ready(_ns(epic="fn-1"))

        out = json.loads(capsys.readouterr().out)
        ready_ids = [t["id"] for t in out["ready"]]
        in_progress_ids = [t["id"] for t in out["in_progress"]]
        assert "fn-1.1" not in ready_ids
        assert "fn-1.1" in in_progress_ids

    def test_ready_shows_blocked_tasks(self, populated_epic, capsys):
        """Blocked tasks should appear in the blocked list."""
        save_task_runtime("fn-1.1", {"status": "blocked"})

        cmd_ready(_ns(epic="fn-1"))

        out = json.loads(capsys.readouterr().out)
        blocked_ids = [b["id"] for b in out["blocked"]]
        assert "fn-1.1" in blocked_ids

    def test_ready_invalid_epic_fails(self, populated_epic):
        """Ready with an invalid epic ID should fail."""
        with pytest.raises(SystemExit):
            cmd_ready(_ns(epic="not-an-epic"))

    def test_ready_nonexistent_epic_fails(self, populated_epic):
        """Ready with a non-existent epic should fail."""
        with pytest.raises(SystemExit):
            cmd_ready(_ns(epic="fn-999"))


# ===========================================================================
# cmd_next tests (>=3 cases)
# ===========================================================================


class TestCmdNext:
    """Tests for cmd_next — selecting the next task to work on."""

    def test_next_returns_first_ready_task(self, populated_epic, capsys):
        """Next should return the first ready task."""
        cmd_next(_ns(
            epics_file=None,
            require_plan_review=False,
            require_completion_review=False,
        ))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "work"
        assert out["task"] == "fn-1.1"
        assert out["reason"] == "ready_task"

    def test_next_resumes_in_progress(self, populated_epic, capsys):
        """Next should resume an in_progress task owned by current actor."""
        save_task_runtime("fn-1.1", {"status": "in_progress", "assignee": "test@test.com"})

        cmd_next(_ns(
            epics_file=None,
            require_plan_review=False,
            require_completion_review=False,
        ))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "work"
        assert out["task"] == "fn-1.1"
        assert out["reason"] == "resume_in_progress"

    def test_next_none_when_all_done(self, populated_epic, capsys):
        """Next should return 'none' when all tasks are done."""
        for tid in ("fn-1.1", "fn-1.2", "fn-1.3"):
            save_task_runtime(tid, {"status": "done"})

        cmd_next(_ns(
            epics_file=None,
            require_plan_review=False,
            require_completion_review=False,
        ))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "none"

    def test_next_completion_review_gate(self, populated_epic, capsys):
        """When all tasks done and completion review required, should signal completion_review."""
        for tid in ("fn-1.1", "fn-1.2", "fn-1.3"):
            save_task_runtime(tid, {"status": "done"})

        cmd_next(_ns(
            epics_file=None,
            require_plan_review=False,
            require_completion_review=True,
        ))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "completion_review"
        assert out["reason"] == "needs_completion_review"

    def test_next_plan_review_gate(self, populated_epic, capsys):
        """When plan review is required but not done, should signal plan review needed."""
        cmd_next(_ns(
            epics_file=None,
            require_plan_review=True,
            require_completion_review=False,
        ))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "plan"
        assert out["reason"] == "needs_plan_review"

    def test_next_skips_done_epics(self, populated_epic, capsys):
        """Next should skip epics with status=done."""
        # Mark epic as done
        epic_path = populated_epic / "epics" / "fn-1.json"
        epic_data = json.loads(epic_path.read_text(encoding="utf-8"))
        epic_data["status"] = "done"
        epic_path.write_text(json.dumps(epic_data, indent=2) + "\n", encoding="utf-8")

        cmd_next(_ns(
            epics_file=None,
            require_plan_review=False,
            require_completion_review=False,
        ))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "none"


# ===========================================================================
# cmd_restart tests (>=2 cases)
# ===========================================================================


class TestCmdRestart:
    """Tests for cmd_restart — resetting tasks and cascading to dependents."""

    def test_restart_resets_task_and_dependents(self, populated_epic, capsys):
        """Restart should reset the target task and cascade to downstream dependents."""
        # Set up: fn-1.1 done, fn-1.2 done, fn-1.3 in_progress
        save_task_runtime("fn-1.1", {"status": "done"})
        save_task_runtime("fn-1.2", {"status": "done"})
        save_task_runtime("fn-1.3", {"status": "in_progress", "assignee": "test@test.com"})

        cmd_restart(_ns(id="fn-1.1", dry_run=False, force=True))

        out = json.loads(capsys.readouterr().out)
        assert out["success"] is True
        # All three should be reset
        assert "fn-1.1" in out["reset"]

        # Verify runtime states
        t1 = load_task_with_state("fn-1.1")
        assert t1["status"] == "todo"

    def test_restart_dry_run(self, populated_epic, capsys):
        """Dry run should report what would be reset without changing anything."""
        save_task_runtime("fn-1.1", {"status": "done"})
        save_task_runtime("fn-1.2", {"status": "done"})

        cmd_restart(_ns(id="fn-1.1", dry_run=True, force=False))

        out = json.loads(capsys.readouterr().out)
        assert out["dry_run"] is True
        assert "fn-1.1" in out["would_reset"]

        # Verify nothing actually changed
        t1 = load_task_with_state("fn-1.1")
        assert t1["status"] == "done"

    def test_restart_in_progress_requires_force(self, populated_epic):
        """Restarting when target is in_progress should fail without --force."""
        save_task_runtime("fn-1.1", {"status": "in_progress", "assignee": "test@test.com"})

        with pytest.raises(SystemExit):
            cmd_restart(_ns(id="fn-1.1", dry_run=False, force=False))

    def test_restart_skips_todo_tasks(self, populated_epic, capsys):
        """Tasks already in todo should be reported as skipped, not reset."""
        save_task_runtime("fn-1.1", {"status": "done"})
        # fn-1.2 and fn-1.3 are still todo

        cmd_restart(_ns(id="fn-1.1", dry_run=False, force=False))

        out = json.loads(capsys.readouterr().out)
        assert "fn-1.1" in out["reset"]
        # fn-1.2 and fn-1.3 are dependents but already todo
        assert "fn-1.2" in out.get("skipped", [])

    def test_restart_invalid_task_id_fails(self, populated_epic):
        """Restarting with an invalid task ID should fail."""
        with pytest.raises(SystemExit):
            cmd_restart(_ns(id="bad-id", dry_run=False, force=False))

    def test_restart_nonexistent_task_fails(self, populated_epic):
        """Restarting a non-existent task should fail."""
        with pytest.raises(SystemExit):
            cmd_restart(_ns(id="fn-1.99", dry_run=False, force=False))


# ===========================================================================
# cmd_block tests
# ===========================================================================


class TestCmdBlock:
    """Tests for cmd_block — blocking a task with a reason."""

    def test_block_sets_blocked_status(self, populated_epic, capsys, tmp_path):
        """Block should set task status to blocked."""
        reason_file = tmp_path / "reason.md"
        reason_file.write_text("Waiting for external API", encoding="utf-8")

        cmd_block(_ns(id="fn-1.1", reason_file=str(reason_file)))

        out = json.loads(capsys.readouterr().out)
        assert out["status"] == "blocked"

        task = load_task_with_state("fn-1.1")
        assert task["status"] == "blocked"

    def test_block_done_task_fails(self, populated_epic, tmp_path):
        """Blocking a done task should fail."""
        save_task_runtime("fn-1.1", {"status": "done"})

        reason_file = tmp_path / "reason.md"
        reason_file.write_text("Some reason", encoding="utf-8")

        with pytest.raises(SystemExit):
            cmd_block(_ns(id="fn-1.1", reason_file=str(reason_file)))

    def test_block_updates_spec(self, populated_epic, tmp_path):
        """Block should update the ## Done summary section in the spec."""
        reason_file = tmp_path / "reason.md"
        reason_file.write_text("Blocked by external dependency", encoding="utf-8")

        cmd_block(_ns(id="fn-1.1", reason_file=str(reason_file)))

        spec_path = populated_epic / "tasks" / "fn-1.1.md"
        content = spec_path.read_text(encoding="utf-8")
        assert "Blocked by external dependency" in content
