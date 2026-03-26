#!/usr/bin/env python3
"""
Auto-memory hook: extracts key decisions, discoveries, and pitfalls from
session transcripts using Gemini AI summarization, saves to .flow/memory/.

Runs on Stop event. Requires `gemini` CLI (falls back to pattern matching).

Only active when:
  - .flow/ exists (project uses flow-code)
  - .flow/config.json has memory.enabled=true OR memory.auto=true
"""

import json
import os
import re
import subprocess
import sys
from pathlib import Path


def get_flow_dir() -> Path:
    """Find .flow/ directory from CWD upward."""
    cwd = Path.cwd()
    for p in [cwd, *cwd.parents]:
        if (p / ".flow").is_dir():
            return p / ".flow"
    return cwd / ".flow"


def is_auto_memory_disabled(flow_dir: Path) -> bool:
    """Check if auto-memory is explicitly disabled in config.
    Default is ON — only returns True if memory.auto is explicitly false."""
    config_path = flow_dir / "config.json"
    if not config_path.exists():
        return False  # No config = default on
    try:
        config = json.loads(config_path.read_text())
        mem = config.get("memory", {})
        if isinstance(mem, dict):
            return mem.get("auto") is False  # Only disabled if explicitly set to false
    except Exception:
        pass
    return False


def read_transcript(hook_input: dict) -> str:
    """Read assistant text from transcript JSONL file."""
    transcript_path = hook_input.get("transcript_path", "")
    if not transcript_path or not Path(transcript_path).exists():
        return ""

    texts = []
    try:
        with open(transcript_path, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    ev = json.loads(line)
                    if ev.get("role") != "assistant":
                        continue
                    msg = ev.get("message", {})
                    for blk in msg.get("content", []):
                        if blk.get("type") == "text":
                            texts.append(blk.get("text", ""))
                except (json.JSONDecodeError, KeyError):
                    pass
    except Exception:
        pass

    return "\n".join(texts)


# ─────────────────────────────────────────────────────────────────────────────
# AI summarization (default: gemini -p)
# ─────────────────────────────────────────────────────────────────────────────

SUMMARIZE_PROMPT = """Analyze this AI coding session transcript and extract the most important learnings.

Output ONLY a JSON array of objects, each with "type" and "content" fields. No markdown, no explanation.

Types:
- "pitfall": things that went wrong, bugs found, things to avoid
- "convention": project patterns discovered, coding conventions learned
- "decision": architectural or design decisions made and why

Rules:
- Max 5 entries (only the most important)
- Each "content" should be one concise sentence (under 150 chars)
- Skip trivial things (file reads, git commands, routine operations)
- Focus on what would help a FUTURE session avoid mistakes or follow decisions
- If nothing important happened, return empty array: []

Example output:
[{"type":"pitfall","content":"Django select_related needed on UserProfile queries to avoid N+1"},{"type":"decision","content":"Chose per-epic review mode over per-task for faster Ralph runs"}]

Transcript:
"""


def summarize_with_gemini(text: str) -> list[dict]:
    """Use gemini -p to extract memories from transcript."""
    # Truncate to ~50k chars to fit context
    if len(text) > 50000:
        text = text[:25000] + "\n...[truncated]...\n" + text[-25000:]

    prompt = SUMMARIZE_PROMPT + text

    try:
        result = subprocess.run(
            ["gemini", "-p", prompt],
            capture_output=True, text=True, timeout=30
        )
        if result.returncode != 0:
            return []

        output = result.stdout.strip()
        # Extract JSON array from output (may have surrounding text)
        match = re.search(r'\[.*\]', output, re.S)
        if not match:
            return []

        memories = json.loads(match.group(0))
        # Validate structure
        valid = []
        for m in memories:
            if isinstance(m, dict) and "type" in m and "content" in m:
                if m["type"] in ("pitfall", "convention", "decision"):
                    valid.append({"type": m["type"], "content": str(m["content"])[:200]})
        return valid[:5]

    except (subprocess.TimeoutExpired, FileNotFoundError, json.JSONDecodeError):
        return []


# ─────────────────────────────────────────────────────────────────────────────
# Pattern matching fallback (when gemini not available)
# ─────────────────────────────────────────────────────────────────────────────

def extract_by_pattern(text: str) -> list[dict]:
    """Fallback: extract memories via regex pattern matching."""
    if not text or len(text) < 100:
        return []

    memories = []
    patterns = [
        (r"(?:decided|chose|chose to|went with|using .+ instead of|switched to)\s+(.{20,150})", "decision"),
        (r"(?:found that|discovered|turns out|learned that|realized)\s+(.{20,150})", "convention"),
        (r"(?:don'?t|avoid|careful with|gotcha|warning|bug:|issue:|never)\s+(.{20,150})", "pitfall"),
        (r"(?:fixed by|solved by|the (?:issue|problem|bug) was|root cause)\s+(.{20,150})", "pitfall"),
    ]

    seen = set()
    for line in text.split("\n"):
        line_lower = line.lower().strip()
        if len(line_lower) < 20:
            continue
        for pattern, mem_type in patterns:
            match = re.search(pattern, line_lower)
            if match:
                content = re.sub(r"\s+", " ", match.group(0).strip())[:200]
                key = content[:50]
                if key not in seen:
                    seen.add(key)
                    memories.append({"type": mem_type, "content": line.strip()[:200]})
        if len(memories) >= 5:
            break

    return memories


# ─────────────────────────────────────────────────────────────────────────────
# Save memories
# ─────────────────────────────────────────────────────────────────────────────

def save_memories(memories: list[dict], flow_dir: Path) -> int:
    """Save memories via flowctl or direct file write."""
    if not memories:
        return 0

    # Find flowctl
    flowctl = None
    for candidate in [
        flow_dir / "bin" / "flowctl",
        Path.cwd() / "scripts" / "ralph" / "flowctl",
        Path.cwd() / "scripts" / "auto-improve" / "flowctl",
    ]:
        if candidate.exists():
            flowctl = str(candidate)
            break

    if not flowctl:
        return save_memories_direct(memories, flow_dir)

    saved = 0
    for mem in memories:
        try:
            result = subprocess.run(
                [flowctl, "memory", "add", "--type", mem["type"], mem["content"]],
                capture_output=True, text=True, timeout=5
            )
            if result.returncode == 0:
                saved += 1
        except Exception:
            pass

    return saved


def save_memories_direct(memories: list[dict], flow_dir: Path) -> int:
    """Write directly to .flow/memory/ files as fallback."""
    memory_dir = flow_dir / "memory"
    if not memory_dir.exists():
        return 0

    type_to_file = {
        "pitfall": "pitfalls.md",
        "convention": "conventions.md",
        "decision": "decisions.md",
    }

    saved = 0
    for mem in memories:
        filename = type_to_file.get(mem["type"], "conventions.md")
        filepath = memory_dir / filename
        if filepath.exists():
            content = filepath.read_text(encoding="utf-8")
            entry = f"- {mem['content']}\n"
            if entry not in content:
                with open(filepath, "a", encoding="utf-8") as f:
                    f.write(entry)
                saved += 1

    return saved


# ─────────────────────────────────────────────────────────────────────────────
# Main
# ─────────────────────────────────────────────────────────────────────────────

def main():
    try:
        hook_input = json.loads(sys.stdin.read())
    except Exception:
        hook_input = {}

    flow_dir = get_flow_dir()

    if not flow_dir.exists():
        sys.exit(0)
    if is_auto_memory_disabled(flow_dir):
        sys.exit(0)

    # Auto-init memory dir if missing
    memory_dir = flow_dir / "memory"
    if not memory_dir.exists():
        memory_dir.mkdir(parents=True, exist_ok=True)
        for fname, header in [
            ("pitfalls.md", "# Pitfalls\n\n<!-- Auto-captured by auto-memory hook -->\n"),
            ("conventions.md", "# Conventions\n\n<!-- Auto-captured by auto-memory hook -->\n"),
            ("decisions.md", "# Decisions\n\n<!-- Auto-captured by auto-memory hook -->\n"),
        ]:
            (memory_dir / fname).write_text(header, encoding="utf-8")

    text = read_transcript(hook_input)
    if not text or len(text) < 200:
        sys.exit(0)

    # Default: AI summarization via gemini
    # Fallback: pattern matching if gemini not available
    memories = summarize_with_gemini(text)
    method = "gemini"

    if not memories:
        memories = extract_by_pattern(text)
        method = "pattern"

    saved = save_memories(memories, flow_dir)

    if saved > 0:
        print(f"auto-memory: captured {saved} entries via {method}", file=sys.stderr)

    sys.exit(0)


if __name__ == "__main__":
    main()
