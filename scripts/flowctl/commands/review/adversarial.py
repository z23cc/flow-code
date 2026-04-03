"""Adversarial review: Codex tries to break the code."""

import argparse
import json
import re
from pathlib import Path
from typing import Optional

from flowctl.core.git import get_changed_files, get_diff_context, get_embedded_file_contents
from flowctl.core.io import error_exit, json_output

from flowctl.commands.review.codex_utils import (
    run_codex_exec,
    parse_codex_verdict,
    resolve_codex_sandbox,
    CODEX_EFFORT_LEVELS,
)

def _get_plugin_root() -> Path:
    """Get the flow-code plugin root directory."""
    return Path(__file__).resolve().parent.parent.parent.parent


def _load_adversarial_prompt(focus_block: str, diff_summary: str,
                             diff_content: str, embedded_files: str) -> str:
    """Load adversarial review prompt from prompts/adversarial-review.md."""
    prompt_path = _get_plugin_root() / "prompts" / "adversarial-review.md"
    template = prompt_path.read_text()
    result = template.replace(
        "{{focus_block}}", focus_block,
    ).replace(
        "{{diff_summary}}", diff_summary,
    ).replace(
        "{{diff_content}}", diff_content,
    ).replace(
        "{{embedded_files}}", embedded_files,
    )
    # Warn on unconsumed placeholders
    remaining = re.findall(r"\{\{(\w+)\}\}", result)
    if remaining:
        import sys
        print(f"Warning: unconsumed placeholders in adversarial prompt: {remaining}",
              file=sys.stderr)
    return result


def parse_adversarial_output(output: str) -> Optional[dict]:
    """Parse structured JSON output from adversarial review.

    Handles multiple output formats:
    1. Direct JSON object with verdict
    2. JSONL streaming events (codex exec --json) with verdict nested in agent_message
    3. Markdown-fenced JSON
    4. JSON embedded in free text

    Returns the parsed dict on success, None on failure.
    """
    # Strategy 1: Direct JSON parse (clean output)
    try:
        data = json.loads(output.strip())
        if isinstance(data, dict) and "verdict" in data:
            return data
    except (json.JSONDecodeError, ValueError):
        pass

    # Strategy 2: Extract from JSONL streaming events (codex exec --json output)
    # Look for agent_message items containing the verdict JSON
    for line in output.split("\n"):
        line = line.strip()
        if not line:
            continue
        try:
            event = json.loads(line)
            if event.get("type") == "item.completed":
                item = event.get("item", {})
                if item.get("type") == "agent_message":
                    text = item.get("text", "")
                    try:
                        data = json.loads(text)
                        if isinstance(data, dict) and "verdict" in data:
                            return data
                    except (json.JSONDecodeError, ValueError):
                        pass
        except (json.JSONDecodeError, ValueError):
            continue

    # Strategy 3: Markdown fences
    json_match = re.search(r"```(?:json)?\s*\n?(.*?)\n?```", output, re.DOTALL)
    if json_match:
        try:
            data = json.loads(json_match.group(1).strip())
            if isinstance(data, dict) and "verdict" in data:
                return data
        except (json.JSONDecodeError, ValueError):
            pass

    # Strategy 4: Greedy brace match for embedded JSON
    brace_match = re.search(r"\{[^{}]*\"verdict\"[^{}]*\}", output)
    if brace_match:
        try:
            data = json.loads(brace_match.group(0))
            if isinstance(data, dict) and "verdict" in data:
                return data
        except (json.JSONDecodeError, ValueError):
            pass

    return None


def cmd_codex_adversarial(args: argparse.Namespace) -> None:
    """Run adversarial review via Codex — tries to break the code, not validate it."""
    base_branch = args.base
    focus = getattr(args, "focus", None)

    # Get diff summary + content via shared helper
    diff_summary, diff_content = get_diff_context(base_branch)

    if not diff_summary and not diff_content:
        error_exit(f"No changes found between {base_branch} and HEAD", use_json=args.json)

    # Embed changed files
    changed_files = get_changed_files(base_branch)
    embedded_content, _ = get_embedded_file_contents(changed_files)

    # Build prompt
    focus_block = f"Focus area: {focus}" if focus else ""
    prompt = _load_adversarial_prompt(
        focus_block=focus_block,
        diff_summary=diff_summary,
        diff_content=diff_content,
        embedded_files=embedded_content or "(no files embedded)",
    )

    # Resolve sandbox
    try:
        sandbox = resolve_codex_sandbox(getattr(args, "sandbox", "auto"))
    except ValueError as e:
        error_exit(str(e), use_json=args.json, code=2)

    # Run codex
    effort = getattr(args, "effort", "high")
    output, thread_id, exit_code, stderr = run_codex_exec(
        prompt, sandbox=sandbox, effort=effort
    )

    if exit_code != 0:
        msg = (stderr or output or "codex exec failed").strip()
        error_exit(f"Adversarial review failed: {msg}", use_json=args.json, code=2)

    # Parse structured JSON output first, fall back to regex verdict
    structured = parse_adversarial_output(output)

    if args.json:
        if structured:
            structured["base"] = base_branch
            structured["focus"] = focus
            structured["files_reviewed"] = len(changed_files)
            json_output(structured)
        else:
            verdict = parse_codex_verdict(output)
            json_output({
                "verdict": verdict or "UNKNOWN",
                "output": output,
                "base": base_branch,
                "focus": focus,
                "files_reviewed": len(changed_files),
            })
    else:
        if structured:
            print(json.dumps(structured, indent=2))
            print(f"\nVerdict: {structured.get('verdict', 'UNKNOWN')}")
        else:
            print(output)
            verdict = parse_codex_verdict(output)
            if verdict:
                print(f"\nVerdict: {verdict}")
