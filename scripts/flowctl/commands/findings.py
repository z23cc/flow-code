"""parse-findings command: extract structured <findings> JSON from review output."""

import argparse
import json
import re
import sys
from typing import List, Tuple

from flowctl.core.io import error_exit, json_output, read_file_or_stdin
from flowctl.core.ids import is_epic_id
from flowctl.core.paths import ensure_flow_exists

# Severity → gap priority mapping
SEVERITY_TO_PRIORITY = {
    "critical": "required",
    "major": "important",
    "minor": "nice-to-have",
    "nitpick": "nice-to-have",
}

REQUIRED_KEYS = ("title", "severity", "location", "recommendation")
MAX_FINDINGS = 50


def _repair_json(text: str) -> str:
    """Stdlib-only JSON repair: strip fences, trailing commas, single quotes."""
    # Strip markdown code fences
    text = re.sub(r"^```(?:json)?\s*\n?", "", text.strip())
    text = re.sub(r"\n?```\s*$", "", text.strip())

    # Remove trailing commas before ] or }
    text = re.sub(r",\s*([}\]])", r"\1", text)

    # Replace single quotes with double quotes (simple heuristic —
    # only when they look like JSON string delimiters)
    # This handles {'key': 'value'} but not contractions inside values.
    # We only apply this if the text doesn't parse as-is.
    try:
        json.loads(text)
        return text
    except (json.JSONDecodeError, ValueError):
        pass

    # Try replacing single-quote delimiters
    repaired = re.sub(r"(?<=[\[{,:\s])'|'(?=[\]},:.\s])", '"', text)
    return repaired


def parse_findings(text: str) -> Tuple[List[dict], List[str]]:
    """Extract structured findings from review output text.

    Tiered extraction:
    1. <findings>...</findings> tag
    2. Bare JSON array [{...}]
    3. Markdown code block ```json...```
    4. Graceful empty

    Returns (findings_list, warnings).
    """
    warnings: List[str] = []
    raw_json = None

    # Tier 1: <findings> tag
    match = re.search(r"<findings>\s*(.*?)\s*</findings>", text, re.DOTALL)
    if match:
        raw_json = match.group(1).strip()
    else:
        # Tier 2: bare JSON array
        match = re.search(r"(\[\s*\{.*?\}\s*\])", text, re.DOTALL)
        if match:
            raw_json = match.group(1).strip()
            warnings.append("No <findings> tag found; extracted bare JSON array")
        else:
            # Tier 3: markdown code block
            match = re.search(r"```(?:json)?\s*\n(\[.*?\])\s*\n?```", text, re.DOTALL)
            if match:
                raw_json = match.group(1).strip()
                warnings.append("No <findings> tag found; extracted from code block")
            else:
                # Tier 4: graceful empty
                warnings.append("No findings found in review output")
                return [], warnings

    # Repair and parse JSON
    repaired = _repair_json(raw_json)
    try:
        findings = json.loads(repaired)
    except (json.JSONDecodeError, ValueError) as e:
        warnings.append(f"Failed to parse findings JSON: {e}")
        return [], warnings

    if not isinstance(findings, list):
        warnings.append("Findings JSON is not a list")
        return [], warnings

    # Validate each finding
    valid_findings: List[dict] = []
    for i, finding in enumerate(findings):
        if not isinstance(finding, dict):
            warnings.append(f"Finding {i} is not an object, skipping")
            continue
        missing = [k for k in REQUIRED_KEYS if k not in finding]
        if missing:
            warnings.append(f"Finding {i} missing keys: {', '.join(missing)}, skipping")
            continue
        # Normalize severity to lowercase
        finding["severity"] = finding["severity"].strip().lower()
        valid_findings.append(finding)

    # Cap at MAX_FINDINGS
    if len(valid_findings) > MAX_FINDINGS:
        warnings.append(
            f"Found {len(valid_findings)} findings, capping at {MAX_FINDINGS}"
        )
        valid_findings = valid_findings[:MAX_FINDINGS]

    return valid_findings, warnings


def cmd_parse_findings(args: argparse.Namespace) -> None:
    """Parse structured findings from review output text."""
    text = read_file_or_stdin(args.file, "review output", use_json=args.json)
    findings, warnings = parse_findings(text)

    registered = 0
    if args.register:
        if not args.epic:
            error_exit(
                "--epic is required when --register is used", use_json=args.json
            )
        if not ensure_flow_exists():
            error_exit(
                ".flow/ does not exist. Run 'flowctl init' first.",
                use_json=args.json,
            )
        if not is_epic_id(args.epic):
            error_exit(f"Invalid epic ID: {args.epic}", use_json=args.json)

        # Import gap internals (avoid circular at module level)
        from flowctl.commands.gap import cmd_gap_add

        for finding in findings:
            severity = finding["severity"]
            priority = SEVERITY_TO_PRIORITY.get(severity)
            if priority is None:
                warnings.append(
                    f"Unknown severity '{severity}' for '{finding['title']}', "
                    f"defaulting to 'important'"
                )
                priority = "important"

            # Only register critical/major (required/important priorities)
            if priority not in ("required", "important"):
                continue

            # Build a mock args namespace to reuse cmd_gap_add
            gap_args = argparse.Namespace(
                epic=args.epic,
                capability=finding["title"],
                priority=priority,
                source=args.source,
                task=None,
                json=True,  # always JSON to capture output
            )

            # Capture stdout to avoid polluting our output
            old_stdout = sys.stdout
            sys.stdout = _CaptureStdout()
            try:
                cmd_gap_add(gap_args)
                registered += 1
            except SystemExit:
                # cmd_gap_add may exit on duplicate (which is fine — idempotent)
                # Check if it was a success (gap already exists = still counts)
                registered += 1
            finally:
                sys.stdout = old_stdout

    # Handle unknown severity warnings for non-register mode
    if not args.register:
        for finding in findings:
            severity = finding["severity"]
            if severity not in SEVERITY_TO_PRIORITY:
                warnings.append(
                    f"Unknown severity '{severity}' for '{finding['title']}', "
                    f"would default to 'important'"
                )

    result = {
        "findings": findings,
        "count": len(findings),
        "registered": registered,
        "warnings": warnings,
    }

    if args.json:
        json_output(result)
    else:
        print(f"Found {len(findings)} finding(s)")
        if registered:
            print(f"Registered {registered} gap(s)")
        for w in warnings:
            print(f"  Warning: {w}", file=sys.stderr)
        for f in findings:
            sev = f["severity"]
            print(f"  [{sev}] {f['title']} — {f['location']}")


class _CaptureStdout:
    """Minimal stdout capture to suppress gap add output."""

    def __init__(self):
        self.data = []

    def write(self, s):
        self.data.append(s)

    def flush(self):
        pass
