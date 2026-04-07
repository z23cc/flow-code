#!/usr/bin/env bash
# validate-skills.sh — validate SKILL.md files against skill-anatomy.md schema
# Exit 0 = all pass, 1 = failures found
set -euo pipefail

# Resolve plugin root (same pattern as other scripts)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLUGIN_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

SKILLS_DIR="$PLUGIN_ROOT/skills"

pass_count=0
fail_count=0
warn_count=0
total=0

# Required frontmatter fields
REQUIRED_FM_FIELDS="name description"

# Required ## sections per docs/skill-anatomy.md
REQUIRED_SECTIONS="Overview|When to Use|Core Process|Common Rationalizations|Red Flags|Verification"

MAX_LINES=500

validate_skill() {
  local skill_file="$1"
  local skill_dir
  skill_dir="$(basename "$(dirname "$skill_file")")"
  local has_error=0
  local error_msgs=""
  local warn_msgs=""

  # --- Check YAML frontmatter exists ---
  local first_line
  first_line="$(head -1 "$skill_file")"
  if [[ "$first_line" != "---" ]]; then
    has_error=1
    error_msgs="      ✗ missing YAML frontmatter"
    total=$((total + 1))
    fail_count=$((fail_count + 1))
    echo "FAIL  ${skill_dir}"
    echo "$error_msgs"
    return
  fi

  # Extract frontmatter (between first and second ---)
  local fm
  fm="$(awk 'BEGIN{n=0} /^---$/{n++; if(n==2) exit; next} n==1{print}' "$skill_file")"

  if [[ -z "$fm" ]]; then
    has_error=1
    total=$((total + 1))
    fail_count=$((fail_count + 1))
    echo "FAIL  ${skill_dir}"
    echo "      ✗ empty or malformed YAML frontmatter"
    return
  fi

  # --- Check required frontmatter fields ---
  for field in $REQUIRED_FM_FIELDS; do
    if ! echo "$fm" | grep -qE "^${field}:"; then
      has_error=1
      error_msgs="${error_msgs}      ✗ missing frontmatter field: ${field}
"
    fi
  done

  # --- Check name field value ---
  local name_val
  name_val="$(echo "$fm" | grep -E '^name:' | head -1 | sed 's/^name:[[:space:]]*//' | tr -d '"' | tr -d "'")"
  if [[ -n "$name_val" && "$name_val" != "$skill_dir" ]]; then
    warn_msgs="${warn_msgs}      ⚠ name '${name_val}' does not match directory '${skill_dir}'
"
  fi

  # --- Check description starts with "Use when" ---
  local desc_val
  desc_val="$(echo "$fm" | grep -E '^description:' | head -1 | sed 's/^description:[[:space:]]*//' | tr -d '"' | tr -d "'")"
  if [[ -n "$desc_val" ]]; then
    if ! echo "$desc_val" | grep -q '^Use when'; then
      warn_msgs="${warn_msgs}      ⚠ description should start with 'Use when...'
"
    fi
  fi

  # --- Check required ## sections (warn, not fail) ---
  local body
  body="$(awk 'BEGIN{n=0} /^---$/{n++; next} n>=2{print}' "$skill_file")"

  local IFS='|'
  for section in $REQUIRED_SECTIONS; do
    if ! echo "$body" | grep -qiE "^## .*${section}"; then
      warn_msgs="${warn_msgs}      ⚠ missing recommended section: ## ${section}
"
    fi
  done
  unset IFS

  # --- File size warning ---
  local line_count
  line_count="$(wc -l < "$skill_file" | tr -d ' ')"
  if [[ "$line_count" -gt "$MAX_LINES" ]]; then
    warn_msgs="${warn_msgs}      ⚠ ${line_count} lines (>${MAX_LINES} recommended max)
"
  fi

  # --- Print result ---
  total=$((total + 1))

  if [[ "$has_error" -eq 1 ]]; then
    fail_count=$((fail_count + 1))
    echo "FAIL  ${skill_dir}"
    [[ -n "$error_msgs" ]] && printf '%s' "$error_msgs"
  else
    pass_count=$((pass_count + 1))
    echo "PASS  ${skill_dir}"
  fi

  if [[ -n "$warn_msgs" ]]; then
    local wc_lines
    wc_lines="$(echo "$warn_msgs" | grep -c '⚠' || true)"
    warn_count=$((warn_count + wc_lines))
    printf '%s' "$warn_msgs"
  fi
}

# --- Main ---
echo "Validating skills in ${SKILLS_DIR}/"
echo "---"

for skill_file in "$SKILLS_DIR"/*/SKILL.md; do
  [[ -f "$skill_file" ]] || continue
  validate_skill "$skill_file"
done

echo "---"
echo "Total: ${total}  Pass: ${pass_count}  Fail: ${fail_count}  Warnings: ${warn_count}"

if [[ "$fail_count" -gt 0 ]]; then
  exit 1
fi
exit 0
