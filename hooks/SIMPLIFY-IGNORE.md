# simplify-ignore hook

Annotation-based code protection for Claude Code. Mark code blocks with `simplify-ignore-start` / `simplify-ignore-end` annotations and the hook replaces them with `BLOCK_<hash>` placeholders before the model sees them. On edit or write, placeholders are expanded back to the original code. On session stop, all files are fully restored.

## Setup

Add to your project's `.claude/settings.json` (NOT `hooks/hooks.json` -- this is opt-in per project):

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Read",
        "hooks": [{ "type": "command", "command": "bash ${CLAUDE_PROJECT_DIR}/hooks/simplify-ignore.sh" }]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [{ "type": "command", "command": "bash ${CLAUDE_PROJECT_DIR}/hooks/simplify-ignore.sh" }]
      }
    ],
    "Stop": [
      {
        "hooks": [{ "type": "command", "command": "bash ${CLAUDE_PROJECT_DIR}/hooks/simplify-ignore.sh" }]
      }
    ]
  }
}
```

Add `.claude/.simplify-ignore-cache/` to your `.gitignore`.

## Annotation syntax

```js
/* simplify-ignore-start: perf-critical */
// manually unrolled XOR -- 3x faster than a loop
result[0] = buf[0] ^ key[0];
result[1] = buf[1] ^ key[1];
/* simplify-ignore-end */
```

The reason after the colon is optional. It appears in the placeholder (`BLOCK_a1b2c3d4: perf-critical`) so the model knows why the block is hidden.

## Supported comment styles

| Style | Example |
|-------|---------|
| `/* */` | `/* simplify-ignore-start: reason */` |
| `//` | `// simplify-ignore-start: reason` |
| `#` | `# simplify-ignore-start: reason` |
| `<!-- -->` | `<!-- simplify-ignore-start: reason -->` |

Multiple blocks per file and single-line blocks (start + end on same line) are supported. Placeholders preserve the original comment prefix/suffix.

## How it works

| Hook event | Action |
|------------|--------|
| PreToolUse Read | Backs up file, replaces annotated blocks with `BLOCK_<hash>` placeholders |
| PostToolUse Edit/Write | Expands placeholders back, saves model changes, re-filters |
| Stop | Restores all files from backup |

Block hashes are 8 hex chars from `shasum`/`sha1sum` of the block content, making round-trips unambiguous.

## Crash recovery

If Claude Code exits without triggering the Stop hook, files may still have `BLOCK_<hash>` placeholders. Restore manually:

```bash
echo '{}' | bash hooks/simplify-ignore.sh
```

Backups are in `.claude/.simplify-ignore-cache/` within your project directory.

## Known limitations

- **Single-line blocks hide the entire line.** Use dedicated lines for annotations.
- **Suffix detection covers `*/` and `-->` only.** Non-standard template comment closers (ERB, Blade) may not work; use `#` or `//` instead.
- **Fuzzy fallback on altered placeholders.** If the model changes a placeholder's reason text, the hook tries a hash-only match which may leave cosmetic debris.
- **File renaming leaves placeholders.** Renamed files retain placeholders; originals are saved as `.recovered` on stop.
- **Annotation must be in a comment.** Only lines with `//`, `/*`, `#`, or `<!--` before `simplify-ignore-start` are recognized. String literals or docs won't accidentally trigger.
- **Disk state during session.** While active, protected files on disk contain placeholders between Read and Edit/Write events. External tools may see broken source during this window.
- **Missing placeholder safety.** If a whole-file Write drops a `BLOCK_<hash>` placeholder, the hook restores from backup to prevent data loss.

## Requirements

- `jq`
- `shasum` or `sha1sum` (auto-detected)
- Bash 3.2+
