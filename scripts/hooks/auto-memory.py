#!/usr/bin/env python3
"""
Auto-memory hook: extracts key decisions, discoveries, and pitfalls from
session transcripts and saves them to .flow/memory/ via flowctl.

Runs on Stop event. Zero external dependencies.

Only active when:
  - .flow/ exists (project uses flow-code)
  - .flow/config.json has memory.enabled=true OR memory.auto=true
"""

import json
import os
import re
import sys
from pathlib import Path


def get_flow_dir() -> Path:
    """Find .flow/ directory from CWD upward."""
    cwd = Path.cwd()
    for p in [cwd, *cwd.parents]:
        if (p / ".flow").is_dir():
            return p / ".flow"
    return cwd / ".flow"


def is_auto_memory_enabled(flow_dir: Path) -> bool:
    """Check if auto-memory is enabled in config."""
    config_path = flow_dir / "config.json"
    if not config_path.exists():
        return False
    try:
        config = json.loads(config_path.read_text())
        # Enabled if memory.auto is true, or memory.enabled is true
        mem = config.get("memory", {})
        if isinstance(mem, dict):
            return mem.get("auto", False) or mem.get("enabled", False)
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


def extract_memories(text: str) -> list[dict]:
    """Extract key decisions, discoveries, and pitfalls from session text.

    Uses pattern matching — no LLM call needed.
    Looks for:
    - Explicit decisions ("decided to", "chose", "went with", "using X instead of Y")
    - Discoveries ("found that", "discovered", "turns out", "learned that")
    - Pitfalls ("don't", "avoid", "careful with", "gotcha", "bug:", "warning:")
    - Key fixes ("fixed by", "solved by", "the issue was", "root cause")
    """
    if not text or len(text) < 100:
        return []

    memories = []
    lines = text.split("\n")

    # Patterns → memory type
    patterns = [
        # Decisions
        (r"(?:decided|chose|chose to|went with|using .+ instead of|switched to|picked)\s+(.{20,150})",
         "decision"),
        # Discoveries
        (r"(?:found that|discovered|turns out|learned that|realized|it appears)\s+(.{20,150})",
         "convention"),
        # Pitfalls
        (r"(?:don'?t|avoid|careful with|gotcha|warning|bug:|issue:|never)\s+(.{20,150})",
         "pitfall"),
        # Fixes
        (r"(?:fixed by|solved by|the (?:issue|problem|bug) was|root cause)\s+(.{20,150})",
         "pitfall"),
    ]

    seen = set()
    for line in lines:
        line_lower = line.lower().strip()
        if len(line_lower) < 20:
            continue

        for pattern, mem_type in patterns:
            match = re.search(pattern, line_lower)
            if match:
                content = match.group(0).strip()
                # Clean up and deduplicate
                content = re.sub(r"\s+", " ", content)[:200]
                key = content[:50]
                if key not in seen:
                    seen.add(key)
                    # Use the original case line for the memory content
                    orig_content = line.strip()[:200]
                    memories.append({
                        "type": mem_type,
                        "content": orig_content,
                    })

        # Cap at 5 memories per session to avoid noise
        if len(memories) >= 5:
            break

    return memories


def save_memories(memories: list[dict], flow_dir: Path) -> int:
    """Save memories using flowctl memory add."""
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
        # Fallback: write directly to memory files
        return save_memories_direct(memories, flow_dir)

    saved = 0
    for mem in memories:
        try:
            import subprocess
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


def main():
    # Read hook input from stdin
    try:
        hook_input = json.loads(sys.stdin.read())
    except Exception:
        hook_input = {}

    flow_dir = get_flow_dir()

    # Guard: only run if .flow/ exists and auto-memory enabled
    if not flow_dir.exists():
        sys.exit(0)
    if not is_auto_memory_enabled(flow_dir):
        sys.exit(0)

    # Read transcript
    text = read_transcript(hook_input)
    if not text:
        sys.exit(0)

    # Extract and save
    memories = extract_memories(text)
    saved = save_memories(memories, flow_dir)

    if saved > 0:
        # Non-blocking output (exit 0)
        print(f"auto-memory: captured {saved} memory entries", file=sys.stderr)

    sys.exit(0)


if __name__ == "__main__":
    main()
