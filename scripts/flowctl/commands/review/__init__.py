"""Review commands package — split for maintainability.

Submodules:
  codex_utils  — shared Codex CLI helpers (require, run, parse, sandbox)
  prompts      — review prompt builders (impl, plan, completion, standalone)
  commands     — review command handlers (check, impl-review, plan-review, completion-review)
  checkpoint   — checkpoint save/restore/delete
  adversarial  — adversarial review (prompt loading, JSON parsing, command)
"""

from flowctl.commands.review.codex_utils import (  # noqa: F401
    CODEX_EFFORT_LEVELS,
    CODEX_SANDBOX_MODES,
    delete_stale_receipt,
    get_codex_version,
    is_sandbox_failure,
    load_receipt,
    parse_codex_thread_id,
    parse_codex_verdict,
    require_codex,
    resolve_codex_sandbox,
    run_codex_exec,
    save_receipt,
)
from flowctl.commands.review.prompts import (  # noqa: F401
    build_completion_review_prompt,
    build_rereview_preamble,
    build_review_prompt,
    build_standalone_review_prompt,
)
from flowctl.commands.review.commands import (  # noqa: F401
    cmd_codex_check,
    cmd_codex_completion_review,
    cmd_codex_impl_review,
    cmd_codex_plan_review,
)
from flowctl.commands.review.checkpoint import (  # noqa: F401
    cmd_checkpoint_delete,
    cmd_checkpoint_restore,
    cmd_checkpoint_save,
)
from flowctl.commands.review.adversarial import (  # noqa: F401
    cmd_codex_adversarial,
    parse_adversarial_output,
)
