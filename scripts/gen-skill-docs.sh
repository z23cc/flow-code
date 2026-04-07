#!/usr/bin/env bash
# gen-skill-docs.sh — Resolve {{PLACEHOLDERS}} in .tmpl files to generate SKILL.md
# Usage: bash scripts/gen-skill-docs.sh [--dry-run] [--check]
#   --dry-run: show what would change without writing
#   --check:   exit 1 if any generated file is stale (for CI)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SHARED_DIR="$SCRIPT_DIR/skills/_shared"

DRY_RUN=false
CHECK_MODE=false

for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=true ;;
    --check)   CHECK_MODE=true ;;
    -h|--help)
      echo "Usage: bash scripts/gen-skill-docs.sh [--dry-run] [--check]"
      echo "  --dry-run  Show what would change without writing"
      echo "  --check    Exit 1 if any generated file is stale (for CI)"
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      exit 1
      ;;
  esac
done

STALE_FILE="/tmp/gen-skill-docs-stale.$$"
rm -f "$STALE_FILE"

# extract_frontmatter_name <file>
# Reads the YAML frontmatter and returns the name: value
extract_frontmatter_name() {
  local file="$1"
  awk '
    BEGIN { in_fm=0 }
    NR==1 && /^---$/ { in_fm=1; next }
    in_fm && /^---$/ { exit }
    in_fm && /^name:/ { sub(/^name:[[:space:]]*/, ""); print; exit }
  ' "$file"
}

# resolve_placeholders <tmpl_file> <skill_name>
# Reads template, resolves all {{PLACEHOLDER}} markers, outputs to stdout
resolve_placeholders() {
  local tmpl_file="$1"
  local skill_name="$2"

  local generated_notice="<!-- AUTO-GENERATED from SKILL.md.tmpl — DO NOT EDIT DIRECTLY -->"
  local flowctl_path='FLOWCTL="$HOME/.flow/bin/flowctl"'

  # Use python3 for reliable multiline placeholder replacement
  python3 - "$tmpl_file" "$skill_name" "$generated_notice" "$flowctl_path" "$SHARED_DIR" <<'PYEOF'
import sys, os

tmpl_file = sys.argv[1]
skill_name = sys.argv[2]
generated_notice = sys.argv[3]
flowctl_path = sys.argv[4]
shared_dir = sys.argv[5]

with open(tmpl_file, 'r') as f:
    content = f.read()

# Simple string placeholders
content = content.replace('{{GENERATED_NOTICE}}', generated_notice)
content = content.replace('{{FLOWCTL_PATH}}', flowctl_path)
content = content.replace('{{SKILL_NAME}}', skill_name)

# File-content placeholders
file_placeholders = {
    '{{PREAMBLE}}': os.path.join(shared_dir, 'preamble.md'),
    '{{RP_REVIEW_PROTOCOL}}': os.path.join(shared_dir, 'rp-review-protocol.md'),
}

for placeholder, filepath in file_placeholders.items():
    if placeholder in content:
        if os.path.isfile(filepath):
            with open(filepath, 'r') as f:
                replacement = f.read()
            content = content.replace(placeholder, replacement)
        else:
            print(f"Warning: {placeholder} used but {filepath} not found", file=sys.stderr)

# Check for unresolved placeholders
import re
unresolved = set(re.findall(r'\{\{[A-Z_]+\}\}', content))
for p in sorted(unresolved):
    print(f"Warning: Unresolved placeholder {p} in {tmpl_file}", file=sys.stderr)

sys.stdout.write(content)
PYEOF
}

# Find all .tmpl files under skills/
find "$SCRIPT_DIR/skills" -name "*.md.tmpl" -type f | sort | while read -r tmpl_file; do
  # Output path: strip .tmpl suffix
  output_file="${tmpl_file%.tmpl}"
  skill_dir="$(dirname "$tmpl_file")"
  skill_dirname="$(basename "$skill_dir")"

  # Extract SKILL_NAME from frontmatter name: field
  SKILL_NAME=$(extract_frontmatter_name "$tmpl_file")
  # Fallback to directory name if no frontmatter name
  if [ -z "$SKILL_NAME" ]; then
    SKILL_NAME="$skill_dirname"
  fi

  # Resolve placeholders
  resolved=$(resolve_placeholders "$tmpl_file" "$SKILL_NAME")

  # Relative paths for display
  rel_tmpl="${tmpl_file#$SCRIPT_DIR/}"
  rel_out="${output_file#$SCRIPT_DIR/}"

  if $CHECK_MODE; then
    # Compare with existing output
    if [ -f "$output_file" ]; then
      existing=$(cat "$output_file")
      if [ "$existing" != "$resolved" ]; then
        echo "STALE: $rel_out (does not match $rel_tmpl)"
        echo "stale" >> "$STALE_FILE"
      else
        echo "OK: $rel_out"
      fi
    else
      echo "MISSING: $rel_out (not yet generated from $rel_tmpl)"
      echo "stale" >> "$STALE_FILE"
    fi
  elif $DRY_RUN; then
    echo "Would generate: $rel_out (from $rel_tmpl)"
    if [ -f "$output_file" ]; then
      existing=$(cat "$output_file")
      if [ "$existing" != "$resolved" ]; then
        echo "  -> Content would change"
      else
        echo "  -> No changes"
      fi
    else
      echo "  -> New file"
    fi
  else
    # Write the resolved content
    printf '%s' "$resolved" > "$output_file"
    echo "Generated: $rel_out"
  fi
done

# Check for staleness in check mode
if $CHECK_MODE; then
  if [ -f "$STALE_FILE" ]; then
    count=$(wc -l < "$STALE_FILE" | tr -d ' ')
    rm -f "$STALE_FILE"
    echo ""
    echo "ERROR: $count generated file(s) are stale. Run: bash scripts/gen-skill-docs.sh"
    exit 1
  else
    echo ""
    echo "All generated files are up to date."
  fi
fi
