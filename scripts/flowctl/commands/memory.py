"""Memory commands: init, add, read, list, search, inject, verify, gc."""

import argparse
import hashlib
import json
import re
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

from flowctl.core.config import get_config
from flowctl.core.constants import MEMORY_DIR
from flowctl.core.io import atomic_write, atomic_write_json, error_exit, json_output
from flowctl.core.paths import ensure_flow_exists, get_flow_dir

# ─────────────────────────────────────────────────────────────────────────────
# Storage layout:
#   .flow/memory/
#   ├── index.jsonl       <- compact index (~50 tokens/entry)
#   ├── stats.json        <- reference counts + last referenced time
#   └── entries/
#       ├── 001-pitfall.md
#       ├── 002-convention.md
#       └── ...
#
# Legacy files (pitfalls.md, conventions.md, decisions.md) are auto-migrated
# on first use if they contain entries.
# ─────────────────────────────────────────────────────────────────────────────

MEMORY_VALID_TYPES = {"pitfall", "convention", "decision"}


def _memory_dir() -> Path:
    return get_flow_dir() / MEMORY_DIR


def _memory_entries_dir() -> Path:
    d = _memory_dir() / "entries"
    d.mkdir(parents=True, exist_ok=True)
    return d


def _memory_index_path() -> Path:
    return _memory_dir() / "index.jsonl"


def _memory_stats_path() -> Path:
    return _memory_dir() / "stats.json"


def _normalize_memory_type(raw: str) -> str:
    """Normalize type input: 'pitfalls' -> 'pitfall', etc."""
    t = raw.lower().rstrip("s")
    if t not in MEMORY_VALID_TYPES:
        return ""
    return t


def _content_hash(content: str) -> str:
    """SHA256 prefix for deduplication."""
    return hashlib.sha256(content.strip().encode("utf-8")).hexdigest()[:12]


def _next_entry_id(entries_dir: Path) -> int:
    """Scan existing entries to find next numeric ID."""
    max_id = 0
    for f in entries_dir.glob("*.md"):
        m = re.match(r"^(\d+)-", f.name)
        if m:
            max_id = max(max_id, int(m.group(1)))
    return max_id + 1


def _load_index(index_path: Path) -> list[dict]:
    """Load index.jsonl entries."""
    entries = []
    if not index_path.exists():
        return entries
    for line in index_path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line:
            try:
                entries.append(json.loads(line))
            except json.JSONDecodeError:
                continue
    return entries


def _save_index(index_path: Path, entries: list[dict]) -> None:
    """Write index.jsonl atomically."""
    lines = [json.dumps(e, separators=(",", ":")) for e in entries]
    atomic_write(index_path, "\n".join(lines) + "\n" if lines else "")


def _load_stats(stats_path: Path) -> dict:
    """Load stats.json."""
    if not stats_path.exists():
        return {}
    try:
        return json.loads(stats_path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return {}


def _save_stats(stats_path: Path, stats: dict) -> None:
    """Write stats.json atomically."""
    atomic_write_json(stats_path, stats)


def _bump_refs(stats_path: Path, entry_ids: list[str]) -> None:
    """Increment reference counts for injected entries."""
    if not entry_ids:
        return
    stats = _load_stats(stats_path)
    now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    for eid in entry_ids:
        eid_str = str(eid)
        if eid_str not in stats:
            stats[eid_str] = {"refs": 0, "last_ref": ""}
        stats[eid_str]["refs"] = stats[eid_str].get("refs", 0) + 1
        stats[eid_str]["last_ref"] = now
    _save_stats(stats_path, stats)


def _migrate_legacy_memory(memory_dir: Path) -> int:
    """Migrate legacy markdown files to atomic entries. Returns count migrated."""
    legacy_map = {
        "pitfalls.md": "pitfall",
        "conventions.md": "convention",
        "decisions.md": "decision",
    }
    entries_dir = memory_dir / "entries"
    entries_dir.mkdir(parents=True, exist_ok=True)
    index_path = memory_dir / "index.jsonl"
    existing_index = _load_index(index_path)
    existing_hashes = {e.get("hash", "") for e in existing_index}

    migrated = 0

    for filename, entry_type in legacy_map.items():
        filepath = memory_dir / filename
        if not filepath.exists():
            continue
        text = filepath.read_text(encoding="utf-8")
        # Split into entries by ## date headers
        raw_entries = re.split(r"(?=^## \d{4}-\d{2}-\d{2})", text, flags=re.MULTILINE)
        for raw in raw_entries:
            raw = raw.strip()
            if not raw or not re.match(r"^## \d{4}-\d{2}-\d{2}", raw):
                continue
            # Extract date and content
            lines = raw.splitlines()
            header = lines[0]  # ## 2025-03-27 manual [pitfall]
            content = "\n".join(lines[1:]).strip()
            if not content:
                continue

            # Check dedup
            chash = _content_hash(content)
            if chash in existing_hashes:
                continue

            # Extract date from header
            date_match = re.search(r"(\d{4}-\d{2}-\d{2})", header)
            created = date_match.group(1) if date_match else datetime.now(timezone.utc).strftime("%Y-%m-%d")

            # Extract tags from content (simple keyword extraction)
            tags = _extract_tags(content)

            # Write entry file
            entry_id = _next_entry_id(entries_dir)
            entry_filename = f"{entry_id:03d}-{entry_type}.md"
            atomic_write(entries_dir / entry_filename, content)

            # Build summary (first line, truncated)
            summary = content.splitlines()[0][:120]

            # Append to index
            idx_entry = {
                "id": entry_id,
                "type": entry_type,
                "summary": summary,
                "tags": tags,
                "hash": chash,
                "created": created,
                "file": entry_filename,
            }
            existing_index.append(idx_entry)
            existing_hashes.add(chash)
            migrated += 1

    if migrated > 0:
        _save_index(index_path, existing_index)
        # Rename legacy files to .bak
        for filename in legacy_map:
            filepath = memory_dir / filename
            if filepath.exists():
                bak = filepath.with_suffix(".md.bak")
                if not bak.exists():
                    filepath.rename(bak)

    return migrated


def _extract_tags(content: str) -> list[str]:
    """Extract simple keyword tags from content."""
    # Common technical terms as tags
    tag_patterns = [
        r"\b(typescript|javascript|python|rust|go|java|ruby|swift)\b",
        r"\b(react|vue|angular|svelte|nextjs|django|flask|fastapi|express)\b",
        r"\b(postgres|mysql|sqlite|redis|mongodb|supabase)\b",
        r"\b(docker|kubernetes|ci|cd|github|gitlab)\b",
        r"\b(api|auth|oauth|jwt|cors|csrf|xss|sql)\b",
        r"\b(test|lint|build|deploy|migration|schema)\b",
    ]
    tags = set()
    lower = content.lower()
    for pattern in tag_patterns:
        for m in re.finditer(pattern, lower):
            tags.add(m.group(1))
    return sorted(tags)[:8]  # Cap at 8 tags


def require_memory_enabled(args) -> Path:
    """Check memory is enabled, auto-init and auto-migrate. Returns memory dir."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    if not get_config("memory.enabled", False):
        if args.json:
            json_output(
                {
                    "error": "Memory not enabled. Run: flowctl config set memory.enabled true"
                },
                success=False,
            )
        else:
            print("Error: Memory not enabled.")
            print("Enable with: flowctl config set memory.enabled true")
        sys.exit(1)

    memory_dir = _memory_dir()
    memory_dir.mkdir(parents=True, exist_ok=True)
    entries_dir = memory_dir / "entries"
    entries_dir.mkdir(parents=True, exist_ok=True)

    # Auto-migrate legacy files if present and index doesn't exist yet
    index_path = _memory_index_path()
    legacy_exists = any(
        (memory_dir / f).exists()
        for f in ["pitfalls.md", "conventions.md", "decisions.md"]
    )
    if legacy_exists and not index_path.exists():
        migrated = _migrate_legacy_memory(memory_dir)
        if migrated > 0 and not getattr(args, "json", False):
            print(f"Migrated {migrated} legacy memory entries to v2 format")

    return memory_dir


# ─────────────────────────────────────────────────────────────────────────────
# Commands
# ─────────────────────────────────────────────────────────────────────────────


def cmd_memory_init(args: argparse.Namespace) -> None:
    """Initialize memory directory (v2: atomic entries)."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    if not get_config("memory.enabled", False):
        if args.json:
            json_output(
                {
                    "error": "Memory not enabled. Run: flowctl config set memory.enabled true"
                },
                success=False,
            )
        else:
            print("Error: Memory not enabled.")
            print("Enable with: flowctl config set memory.enabled true")
        sys.exit(1)

    memory_dir = _memory_dir()
    memory_dir.mkdir(parents=True, exist_ok=True)
    entries_dir = memory_dir / "entries"
    entries_dir.mkdir(parents=True, exist_ok=True)

    created = []
    index_path = _memory_index_path()
    if not index_path.exists():
        atomic_write(index_path, "")
        created.append("index.jsonl")

    stats_path = _memory_stats_path()
    if not stats_path.exists():
        _save_stats(stats_path, {})
        created.append("stats.json")

    # Auto-migrate legacy if present
    legacy_exists = any(
        (memory_dir / f).exists()
        for f in ["pitfalls.md", "conventions.md", "decisions.md"]
    )
    migrated = 0
    if legacy_exists:
        migrated = _migrate_legacy_memory(memory_dir)

    if args.json:
        json_output(
            {
                "path": str(memory_dir),
                "created": created,
                "migrated": migrated,
                "message": "Memory v2 initialized",
            }
        )
    else:
        print(f"Memory v2 initialized at {memory_dir}")
        if created:
            for f in created:
                print(f"  Created: {f}")
        if migrated:
            print(f"  Migrated {migrated} legacy entries")


def cmd_memory_add(args: argparse.Namespace) -> None:
    """Add an atomic memory entry with dedup."""
    memory_dir = require_memory_enabled(args)

    type_name = _normalize_memory_type(args.type)
    if not type_name:
        error_exit(
            f"Invalid type '{args.type}'. Use: pitfall, convention, or decision",
            use_json=args.json,
        )

    content = args.content.strip()
    if not content:
        error_exit("Content cannot be empty", use_json=args.json)

    # Dedup check
    chash = _content_hash(content)
    index_path = _memory_index_path()
    existing = _load_index(index_path)
    for e in existing:
        if e.get("hash") == chash:
            if args.json:
                json_output(
                    {"id": e["id"], "duplicate": True, "message": "Duplicate entry, skipped"}
                )
            else:
                print(f"Duplicate of entry #{e['id']}, skipped")
            return

    # Write atomic entry
    entries_dir = _memory_entries_dir()
    entry_id = _next_entry_id(entries_dir)
    entry_filename = f"{entry_id:03d}-{type_name}.md"
    atomic_write(entries_dir / entry_filename, content)

    # Extract tags and summary
    tags = _extract_tags(content)
    summary = content.splitlines()[0][:120]
    created = datetime.now(timezone.utc).strftime("%Y-%m-%d")

    # Append to index
    idx_entry = {
        "id": entry_id,
        "type": type_name,
        "summary": summary,
        "tags": tags,
        "hash": chash,
        "created": created,
        "last_verified": created,
        "file": entry_filename,
    }
    existing.append(idx_entry)
    _save_index(index_path, existing)

    if args.json:
        json_output(
            {"id": entry_id, "type": type_name, "file": entry_filename, "tags": tags}
        )
    else:
        print(f"Added {type_name} #{entry_id}: {summary}")
        if tags:
            print(f"  Tags: {', '.join(tags)}")


def cmd_memory_read(args: argparse.Namespace) -> None:
    """Read memory entries (L3: full content)."""
    memory_dir = require_memory_enabled(args)

    index = _load_index(_memory_index_path())

    # Filter by type if specified
    type_filter = None
    if args.type:
        type_filter = _normalize_memory_type(args.type)
        if not type_filter:
            error_exit(
                f"Invalid type '{args.type}'. Use: pitfall, convention, or decision",
                use_json=args.json,
            )

    entries_dir = _memory_entries_dir()
    results = []
    for idx in index:
        if type_filter and idx.get("type") != type_filter:
            continue
        entry_path = entries_dir / idx["file"]
        content = ""
        if entry_path.exists():
            content = entry_path.read_text(encoding="utf-8")
        results.append({
            "id": idx["id"],
            "type": idx["type"],
            "summary": idx["summary"],
            "tags": idx.get("tags", []),
            "created": idx.get("created", ""),
            "content": content,
        })

    if args.json:
        json_output({"entries": results, "count": len(results)})
    else:
        if results:
            for r in results:
                print(f"--- #{r['id']} [{r['type']}] {r['created']} ---")
                print(r["content"])
                if r["tags"]:
                    print(f"  Tags: {', '.join(r['tags'])}")
                print()
            print(f"Total: {len(results)} entries")
        else:
            print("No memory entries" + (f" of type '{type_filter}'" if type_filter else ""))


def cmd_memory_list(args: argparse.Namespace) -> None:
    """List memory entries with stats."""
    memory_dir = require_memory_enabled(args)

    index = _load_index(_memory_index_path())
    stats = _load_stats(_memory_stats_path())

    counts: dict[str, int] = {}
    for idx in index:
        t = idx.get("type", "unknown")
        counts[t] = counts.get(t, 0) + 1

    total = len(index)
    total_refs = sum(s.get("refs", 0) for s in stats.values())

    # Compute staleness threshold (90 days)
    stale_cutoff = (datetime.now(timezone.utc) - timedelta(days=90)).strftime("%Y-%m-%d")

    if args.json:
        json_output({
            "counts": counts,
            "total": total,
            "total_refs": total_refs,
            "index": [
                {
                    "id": idx["id"],
                    "type": idx["type"],
                    "summary": idx["summary"],
                    "tags": idx.get("tags", []),
                    "created": idx.get("created", ""),
                    "last_verified": idx.get("last_verified", idx.get("created", "")),
                    "stale": idx.get("last_verified", idx.get("created", "")) < stale_cutoff,
                    "refs": stats.get(str(idx["id"]), {}).get("refs", 0),
                }
                for idx in index
            ],
        })
    else:
        stale_count = 0
        print(f"Memory: {total} entries, {total_refs} total references\n")
        for idx in index:
            eid = str(idx["id"])
            refs = stats.get(eid, {}).get("refs", 0)
            verified = idx.get("last_verified", idx.get("created", ""))
            is_stale = verified < stale_cutoff if verified else True
            stale_tag = " [stale]" if is_stale else ""
            if is_stale:
                stale_count += 1
            print(f"  #{idx['id']:3d} [{idx['type']:10s}] refs={refs:2d}  {idx['summary'][:70]}{stale_tag}")
        print()
        for t, c in sorted(counts.items()):
            print(f"  {t}: {c}")
        print(f"  Total: {total}")
        if stale_count:
            print(f"  Stale: {stale_count} (not verified in 90+ days — run /flow-code:retro to verify)")


def cmd_memory_search(args: argparse.Namespace) -> None:
    """Search memory entries by pattern (regex) or tags."""
    memory_dir = require_memory_enabled(args)

    pattern = args.pattern

    # Validate regex pattern
    try:
        compiled = re.compile(pattern, re.IGNORECASE)
    except re.error as e:
        error_exit(f"Invalid regex pattern: {e}", use_json=args.json)

    index = _load_index(_memory_index_path())
    entries_dir = _memory_entries_dir()
    matches = []

    for idx in index:
        # Search in summary, tags, and full content
        hit = False
        if compiled.search(idx.get("summary", "")):
            hit = True
        elif any(compiled.search(t) for t in idx.get("tags", [])):
            hit = True
        else:
            entry_path = entries_dir / idx["file"]
            if entry_path.exists():
                content = entry_path.read_text(encoding="utf-8")
                if compiled.search(content):
                    hit = True

        if hit:
            content = ""
            entry_path = entries_dir / idx["file"]
            if entry_path.exists():
                content = entry_path.read_text(encoding="utf-8")
            matches.append({
                "id": idx["id"],
                "type": idx["type"],
                "summary": idx["summary"],
                "tags": idx.get("tags", []),
                "content": content,
            })

    if args.json:
        json_output({"pattern": pattern, "matches": matches, "count": len(matches)})
    else:
        if matches:
            for m in matches:
                print(f"--- #{m['id']} [{m['type']}] ---")
                print(m["content"])
                print()
            print(f"Found {len(matches)} matches for '{pattern}'")
        else:
            print(f"No matches for '{pattern}'")


def cmd_memory_inject(args: argparse.Namespace) -> None:
    """Inject relevant memory entries for a task context (progressive disclosure).

    L1 (default): Compact index only (~50 tokens/entry)
    L2 (--type/--tags): Filtered full content
    L3 (--full): All entries full content
    """
    memory_dir = require_memory_enabled(args)

    index = _load_index(_memory_index_path())
    if not index:
        if args.json:
            json_output({"entries": [], "level": "L1", "count": 0})
        else:
            print("No memory entries")
        return

    entries_dir = _memory_entries_dir()

    # Determine filter
    type_filter = _normalize_memory_type(args.type) if args.type else None
    tag_filter = [t.strip().lower() for t in args.tags.split(",")] if args.tags else []

    # Filter entries
    filtered = []
    for idx in index:
        if type_filter and idx.get("type") != type_filter:
            continue
        if tag_filter:
            entry_tags = [t.lower() for t in idx.get("tags", [])]
            if not any(t in entry_tags for t in tag_filter):
                continue
        filtered.append(idx)

    # Determine level
    level = "L1"
    if args.full or type_filter or tag_filter:
        level = "L2" if (type_filter or tag_filter) else "L3"

    # Bump reference counts
    _bump_refs(_memory_stats_path(), [str(e["id"]) for e in filtered])

    if level == "L1":
        # Compact index: one line per entry
        if args.json:
            json_output({
                "entries": [
                    {"id": e["id"], "type": e["type"], "summary": e["summary"], "tags": e.get("tags", [])}
                    for e in filtered
                ],
                "level": "L1",
                "count": len(filtered),
            })
        else:
            print(f"Memory index ({len(filtered)} entries):")
            for e in filtered:
                tags_str = f" [{','.join(e.get('tags', [])[:3])}]" if e.get("tags") else ""
                print(f"  #{e['id']} [{e['type']}]{tags_str} {e['summary'][:100]}")
            print(f"\nUse `memory search <pattern>` for full content of specific entries.")
    else:
        # Full content for filtered entries
        results = []
        for idx in filtered:
            entry_path = entries_dir / idx["file"]
            content = entry_path.read_text(encoding="utf-8") if entry_path.exists() else ""
            results.append({
                "id": idx["id"],
                "type": idx["type"],
                "summary": idx["summary"],
                "tags": idx.get("tags", []),
                "content": content,
            })

        if args.json:
            json_output({"entries": results, "level": level, "count": len(results)})
        else:
            for r in results:
                print(f"--- #{r['id']} [{r['type']}] ---")
                print(r["content"])
                print()


def cmd_memory_verify(args: argparse.Namespace) -> None:
    """Mark a memory entry as verified (still valid)."""
    memory_dir = require_memory_enabled(args)

    entry_id = args.id
    today = datetime.now(timezone.utc).strftime("%Y-%m-%d")

    index_path = _memory_index_path()
    index = _load_index(index_path)

    found = False
    for idx in index:
        if idx["id"] == entry_id:
            idx["last_verified"] = today
            found = True
            break

    if not found:
        error_exit(f"Entry #{entry_id} not found", use_json=args.json)

    _save_index(index_path, index)

    if args.json:
        json_output({"id": entry_id, "last_verified": today, "message": f"Entry #{entry_id} verified"})
    else:
        print(f"Entry #{entry_id} verified as still valid ({today})")


def cmd_memory_gc(args: argparse.Namespace) -> None:
    """Garbage collect stale memory entries (0 refs + older than --days)."""
    memory_dir = require_memory_enabled(args)

    index = _load_index(_memory_index_path())
    stats = _load_stats(_memory_stats_path())
    entries_dir = _memory_entries_dir()

    cutoff_days = args.days
    now = datetime.now(timezone.utc)
    cutoff_date = (now - timedelta(days=cutoff_days)).strftime("%Y-%m-%d")

    stale = []
    keep = []

    for idx in index:
        eid_str = str(idx["id"])
        refs = stats.get(eid_str, {}).get("refs", 0)
        created = idx.get("created", "9999-99-99")

        if refs == 0 and created < cutoff_date:
            stale.append(idx)
        else:
            keep.append(idx)

    if args.dry_run:
        if args.json:
            json_output({
                "dry_run": True,
                "stale": [{"id": s["id"], "type": s["type"], "summary": s["summary"]} for s in stale],
                "count": len(stale),
                "kept": len(keep),
            })
        else:
            print(f"Dry run: {len(stale)} stale entries (0 refs, older than {cutoff_days} days)")
            for s in stale:
                print(f"  #{s['id']} [{s['type']}] {s['summary'][:80]}")
            print(f"Would keep: {len(keep)} entries")
        return

    # Remove stale entries
    removed = 0
    for s in stale:
        entry_path = entries_dir / s["file"]
        if entry_path.exists():
            entry_path.unlink()
        # Remove from stats
        eid_str = str(s["id"])
        stats.pop(eid_str, None)
        removed += 1

    # Rewrite index without stale entries
    _save_index(_memory_index_path(), keep)
    _save_stats(_memory_stats_path(), stats)

    if args.json:
        json_output({"removed": removed, "kept": len(keep)})
    else:
        print(f"Removed {removed} stale entries, kept {len(keep)}")
