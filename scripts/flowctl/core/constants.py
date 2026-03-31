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
}
