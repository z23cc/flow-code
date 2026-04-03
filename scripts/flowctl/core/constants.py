"""Constants for the flowctl system."""

SCHEMA_VERSION = 2
SUPPORTED_SCHEMA_VERSIONS = [1, 2]
FLOW_DIR = ".flow"
META_FILE = "meta.json"
EPICS_DIR = "epics"
SPECS_DIR = "specs"
TASKS_DIR = "tasks"
MEMORY_DIR = "memory"
REVIEWS_DIR = "reviews"
CONFIG_FILE = "config.json"

EPIC_STATUS = ["open", "done"]
TASK_STATUS = ["todo", "in_progress", "blocked", "done"]

TASK_SPEC_HEADINGS = [
    "## Description",
    "## Acceptance",
    "## Done summary",
    "## Evidence",
]

# Runtime fields stored in state-dir (not tracked in git)
RUNTIME_FIELDS = {
    "status",
    "updated_at",
    "claimed_at",
    "assignee",
    "claim_note",
    "evidence",
    "blocked_reason",
    "phase_progress",
}

# Phase definitions for worker-phase gate execution
# Each phase: (id, title, done_condition)
PHASE_DEFS = {
    "0":   ("0",   "Verify Configuration",  "OWNED_FILES verified and configuration validated"),
    "1":   ("1",   "Re-anchor",             "Run flowctl show <task> and verify spec was read"),
    "2a":  ("2a",  "TDD Red-Green",         "Failing tests written and confirmed to fail"),
    "2":   ("2",   "Implement",             "Feature implemented and code compiles"),
    "2.5": ("2.5", "Verify & Fix",          "flowctl guard passes and diff reviewed"),
    "3":   ("3",   "Commit",                "Changes committed with conventional commit message"),
    "4":   ("4",   "Review",                "SHIP verdict received from reviewer"),
    "5":   ("5",   "Complete",              "flowctl done called and task status is done"),
    "5b":  ("5b",  "Memory Auto-Save",      "Non-obvious lessons saved to memory (if any)"),
    "6":   ("6",   "Return",               "Summary returned to main conversation"),
}

# Phase sequences by mode
# Teams is the default — Phase 0 (Verify Configuration) always included.
PHASE_SEQ_DEFAULT = ["0", "1", "2", "2.5", "3", "5", "6"]
PHASE_SEQ_TDD     = ["1", "2a", "2", "2.5", "3", "5", "6"]
PHASE_SEQ_REVIEW  = ["1", "2", "2.5", "3", "4", "5", "6"]
