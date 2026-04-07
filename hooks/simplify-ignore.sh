#!/bin/bash
# simplify-ignore.sh — Opt-in hook for annotation-based code protection
#
# PreToolUse Read   → backs up file, replaces annotated blocks with BLOCK_<hash> placeholders
# PostToolUse Edit  → expands placeholders back to real code, re-filters
# PostToolUse Write → expands placeholders back to real code, re-filters
# Stop              → restores all files from backup (crash recovery)
#
# Dependencies: jq, shasum or sha1sum, Bash 3.2+

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  printf '%s\n' "error: simplify-ignore requires jq" >&2; exit 1
fi

CACHE="${CLAUDE_PROJECT_DIR:-.}/.claude/.simplify-ignore-cache"
if [ -t 0 ]; then INPUT="{}"; else INPUT=$(cat); fi

# Parse hook input
TOOL_NAME=$(printf '%s' "$INPUT" | jq -r '.tool_name // empty' 2>/dev/null) || TOOL_NAME=""
FILE_PATH=$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // empty' 2>/dev/null) || FILE_PATH=""

# ── Hash helpers ─────────────────────────────────────────────────────────────
hash_cmd() {
  if command -v shasum >/dev/null 2>&1; then shasum
  elif command -v sha1sum >/dev/null 2>&1; then sha1sum
  else printf '%s\n' "error: missing shasum or sha1sum" >&2; exit 1; fi
}
file_id()    { printf '%s' "$1" | hash_cmd | cut -c1-16; }
block_hash() { printf '%s' "$1" | hash_cmd | cut -c1-8; }

# Escape glob metacharacters for Bash 3.2 parameter expansion
escape_glob() {
  local s="$1"
  s="${s//\\/\\\\}"
  s="${s//\*/\\*}"
  s="${s//\?/\\?}"
  s="${s//\[/\\[}"
  printf '%s' "$s"
}

# ── filter_file: replace simplify-ignore blocks with BLOCK_<hash> placeholders
# Reads $1 (source), writes filtered version to $2, saves blocks to cache.
# Returns 0 if blocks found, 1 if none.
filter_file() {
  local src="$1" dest="$2" fid="$3"
  : > "$dest"
  rm -f "$CACHE/${fid}".block.* "$CACHE/${fid}".reason.* "$CACHE/${fid}".prefix.* "$CACHE/${fid}".suffix.*

  local count=0 in_block=0 buf="" reason="" prefix="" suffix=""

  while IFS= read -r line || [ -n "$line" ]; do
    # Detect start marker (must be preceded by a comment prefix: //, /*, #, or <!--)
    if [ $in_block -eq 0 ]; then
      case "$line" in *//*simplify-ignore-start*|*'/*'*simplify-ignore-start*|*'#'*simplify-ignore-start*|*'<!--'*simplify-ignore-start*)
        in_block=1
        buf="$line"
        prefix="${line%%simplify-ignore-start*}"
        suffix=""
        case "$line" in *'*/'*) suffix=" */" ;; *'-->'*) suffix=" -->" ;; esac
        reason=$(printf '%s' "$line" | sed -n 's/.*simplify-ignore-start:[[:space:]]*//p' \
          | sed 's/[[:space:]]*\*\/.*$//' | sed 's/[[:space:]]*-->.*$//' | sed 's/[[:space:]]*$//')
        # Handle single-line block (start + end on same line)
        case "$line" in *simplify-ignore-end*)
          in_block=0
          local h; h=$(block_hash "$buf")
          count=$((count + 1))
          printf '%s' "$buf" > "$CACHE/${fid}.block.${h}"
          [ -n "$reason" ] && printf '%s' "$reason" > "$CACHE/${fid}.reason.${h}"
          printf '%s' "$prefix" > "$CACHE/${fid}.prefix.${h}"
          printf '%s' "$suffix" > "$CACHE/${fid}.suffix.${h}"
          if [ -n "$reason" ]; then
            printf '%s\n' "${prefix}BLOCK_${h}: ${reason}${suffix}" >> "$dest"
          else
            printf '%s\n' "${prefix}BLOCK_${h}${suffix}" >> "$dest"
          fi
          buf=""; reason=""; prefix=""; suffix=""
          continue
          ;; *)
          continue
          ;;
        esac
      ;; esac
    fi
    # Accumulate block content
    if [ $in_block -eq 1 ]; then
      buf="${buf}
${line}"
    fi
    # Detect end marker
    case "$line" in *simplify-ignore-end*)
      if [ $in_block -eq 1 ]; then
        local h; h=$(block_hash "$buf")
        count=$((count + 1))
        printf '%s' "$buf" > "$CACHE/${fid}.block.${h}"
        [ -n "$reason" ] && printf '%s' "$reason" > "$CACHE/${fid}.reason.${h}"
        printf '%s' "$prefix" > "$CACHE/${fid}.prefix.${h}"
        printf '%s' "$suffix" > "$CACHE/${fid}.suffix.${h}"
        if [ -n "$reason" ]; then
          printf '%s\n' "${prefix}BLOCK_${h}: ${reason}${suffix}" >> "$dest"
        else
          printf '%s\n' "${prefix}BLOCK_${h}${suffix}" >> "$dest"
        fi
        in_block=0; buf=""; reason=""; prefix=""; suffix=""
        continue
      fi
      ;;
    esac
    [ $in_block -eq 0 ] && printf '%s\n' "$line" >> "$dest"
  done < "$src"

  # Unclosed block: flush as-is with warning
  if [ $in_block -eq 1 ] && [ -n "$buf" ]; then
    printf 'Warning: unclosed simplify-ignore-start in %s\n' "$src" >&2
    printf '%s\n' "$buf" >> "$dest"
  fi

  [ $count -gt 0 ] && return 0 || return 1
}

# ── Stop: restore all files from backup ──────────────────────────────────────
if [ -z "$TOOL_NAME" ]; then
  [ -d "$CACHE" ] || exit 0
  for bak in "$CACHE"/*.bak; do
    [ -f "$bak" ] || continue
    fid="${bak##*/}"; fid="${fid%.bak}"
    pathfile="$CACHE/${fid}.path"
    [ -f "$pathfile" ] || { rm -f "$bak"; continue; }
    orig=$(cat "$pathfile")
    if [ -f "$orig" ]; then
      cat "$bak" > "$orig"
    else
      mkdir -p "$(dirname "${orig}.recovered")"
      mv "$bak" "${orig}.recovered"
      printf 'Warning: %s moved/deleted. Recovered to %s.recovered\n' "$orig" "$orig" >&2
    fi
    rm -f "$bak" "$pathfile" "$CACHE/${fid}".block.* "$CACHE/${fid}".reason.* \
      "$CACHE/${fid}".prefix.* "$CACHE/${fid}".suffix.*
  done
  exit 0
fi

[ -z "$FILE_PATH" ] && exit 0

# ── PreToolUse Read: filter in-place ─────────────────────────────────────────
if [ "$TOOL_NAME" = "Read" ]; then
  [ -f "$FILE_PATH" ] || exit 0
  # Don't filter our own files
  case "$(basename "$FILE_PATH")" in simplify-ignore*|SIMPLIFY-IGNORE*) exit 0 ;; esac

  mkdir -p "$CACHE"
  ID=$(file_id "$FILE_PATH")

  # Already filtered
  [ -f "$CACHE/${ID}.bak" ] && exit 0

  # No annotations in this file
  grep -q 'simplify-ignore-start' -- "$FILE_PATH" || exit 0

  # Back up original
  cp -p "$FILE_PATH" "$CACHE/${ID}.bak" 2>/dev/null || cp "$FILE_PATH" "$CACHE/${ID}.bak"
  printf '%s' "$FILE_PATH" > "$CACHE/${ID}.path"

  # Filter in-place
  FILTERED="$CACHE/${ID}.$$.tmp"
  rm -f "$FILTERED"
  if filter_file "$FILE_PATH" "$FILTERED" "$ID"; then
    cat "$FILTERED" > "$FILE_PATH"
    rm -f "$FILTERED"
  else
    # No blocks found (race condition) - clean up
    rm -f "$FILTERED" "$CACHE/${ID}.bak" "$CACHE/${ID}.path"
  fi
  exit 0
fi

# ── PostToolUse Edit|Write: expand placeholders, then re-filter ──────────────
if [ "$TOOL_NAME" = "Edit" ] || [ "$TOOL_NAME" = "Write" ]; then
  ID=$(file_id "$FILE_PATH")
  [ -f "$CACHE/${ID}.bak" ] || exit 0
  ls "$CACHE/${ID}".block.* >/dev/null 2>&1 || exit 0

  # Check for missing placeholders BEFORE expanding — if a placeholder was dropped
  # by a whole-file Write, restore from the original backup to prevent data loss.
  MISSING_BLOCKS=0
  for bf in "$CACHE/${ID}".block.*; do
    [ -f "$bf" ] || continue
    h="${bf##*.}"
    if ! grep -q "BLOCK_${h}" "$FILE_PATH" 2>/dev/null; then
      MISSING_BLOCKS=$((MISSING_BLOCKS + 1))
      printf 'Warning: BLOCK_%s placeholder missing — restoring from backup\n' "$h" >&2
    fi
  done
  if [ "$MISSING_BLOCKS" -gt 0 ]; then
    # Re-insert missing blocks into the edited file (not a full backup restore,
    # which would discard legitimate edits outside protected regions).
    # Strategy: for each missing block, append it at the end with a warning comment.
    for bf in "$CACHE/${ID}".block.*; do
      [ -f "$bf" ] || continue
      h="${bf##*.}"
      if ! grep -q "BLOCK_${h}" "$FILE_PATH" 2>/dev/null; then
        block_content=$(cat "$bf")
        printf '\n%s\n' "$block_content" >> "$FILE_PATH"
        printf 'Warning: BLOCK_%s was dropped — re-appended protected block at end of file\n' "$h" >&2
      fi
    done
    # Clear cache so next Read re-establishes placeholders cleanly
    rm -f "$CACHE/${ID}.bak" "$CACHE/${ID}.path" "$CACHE/${ID}".block.* \
      "$CACHE/${ID}".reason.* "$CACHE/${ID}".prefix.* "$CACHE/${ID}".suffix.*
    exit 0
  fi

  # Expand placeholders back to original blocks
  EXPANDED="$CACHE/${ID}.$$.expanded"
  rm -f "$EXPANDED"
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in *BLOCK_*)
      for bf in "$CACHE/${ID}".block.*; do
        [ -f "$bf" ] || continue
        h="${bf##*.}"
        case "$line" in *"BLOCK_${h}"*)
          bp=""; bs=""; br=""
          [ -f "$CACHE/${ID}.prefix.${h}" ] && bp=$(cat "$CACHE/${ID}.prefix.${h}")
          [ -f "$CACHE/${ID}.suffix.${h}" ] && bs=$(cat "$CACHE/${ID}.suffix.${h}")
          [ -f "$CACHE/${ID}.reason.${h}" ] && br=$(cat "$CACHE/${ID}.reason.${h}")
          if [ -n "$br" ]; then
            placeholder="${bp}BLOCK_${h}: ${br}${bs}"
          else
            placeholder="${bp}BLOCK_${h}${bs}"
          fi
          block_content=$(cat "$bf"; printf x); block_content="${block_content%x}"
          esc_placeholder=$(escape_glob "$placeholder")
          line="${line//$esc_placeholder/$block_content}"
          # Fuzzy fallback if model altered reason text
          case "$block_content" in *"BLOCK_${h}"*) ;; *)
            case "$line" in *"BLOCK_${h}"*)
              printf 'Warning: BLOCK_%s modified by model, using fuzzy match\n' "$h" >&2
              line="${line//BLOCK_${h}/$block_content}"
            ;; esac
          ;; esac
        ;; esac
      done
    ;; esac
    printf '%s\n' "$line" >> "$EXPANDED"
  done < "$FILE_PATH"

  # Preserve inode and permissions
  cat "$EXPANDED" > "$FILE_PATH"
  rm -f "$EXPANDED"

  # DO NOT overwrite the original backup — keep it pristine for crash recovery.
  # The backup always contains the pre-filter original, never the expanded version.

  # Re-filter in-place so next Read sees placeholders
  FILTERED="$CACHE/${ID}.$$.tmp"
  rm -f "$FILTERED"
  if filter_file "$FILE_PATH" "$FILTERED" "$ID"; then
    cat "$FILTERED" > "$FILE_PATH"
    rm -f "$FILTERED"
  fi

  exit 0
fi
