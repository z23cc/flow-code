"""Review commands package — split from monolithic review.py for maintainability.

All public symbols are re-exported here for backward compatibility.
Import from submodules for new code:
  from flowctl.commands.review.codex_utils import run_codex_exec
  from flowctl.commands.review.adversarial import cmd_codex_adversarial
"""

# Re-export everything from legacy module for backward compat
from flowctl.commands.review._legacy import (  # noqa: F401
    # Codex utilities
    CODEX_EFFORT_LEVELS,
    CODEX_SANDBOX_MODES,
    require_codex,
    get_codex_version,
    resolve_codex_sandbox,
    run_codex_exec,
    parse_codex_thread_id,
    parse_codex_verdict,
    is_sandbox_failure,
    # Prompt builders
    build_review_prompt,
    build_standalone_review_prompt,
    build_rereview_preamble,
    build_completion_review_prompt,
    # Commands
    cmd_codex_check,
    cmd_codex_impl_review,
    cmd_codex_plan_review,
    cmd_codex_completion_review,
    cmd_codex_adversarial,
    cmd_checkpoint_save,
    cmd_checkpoint_restore,
    cmd_checkpoint_delete,
    # Adversarial helpers
    parse_adversarial_output,
)
