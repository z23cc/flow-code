#!/usr/bin/env bash
set -euo pipefail

# ─────────────────────────────────────────────────────────────────────────────
# auto-improve.sh — Autonomous code improvement loop
# Inspired by Karpathy's autoresearch: modify → test → keep/discard → repeat
# ─────────────────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
CONFIG="$SCRIPT_DIR/config.env"

fail() { echo "auto-improve: $*" >&2; exit 1; }

# Python detection
pick_python() {
  if command -v python3 >/dev/null 2>&1; then echo "python3"; return; fi
  if command -v python  >/dev/null 2>&1; then echo "python"; return; fi
  echo ""
}
PYTHON_BIN="$(pick_python)"
[[ -n "$PYTHON_BIN" ]] || fail "python not found"

# ─────────────────────────────────────────────────────────────────────────────
# UI helpers
# ─────────────────────────────────────────────────────────────────────────────
START_TIME="$(date +%s)"
elapsed_time() {
  local now elapsed mins secs
  now="$(date +%s)"
  elapsed=$((now - START_TIME))
  mins=$((elapsed / 60))
  secs=$((elapsed % 60))
  printf "%d:%02d" "$mins" "$secs"
}

if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
  C_RESET='\033[0m' C_BOLD='\033[1m' C_DIM='\033[2m'
  C_BLUE='\033[34m' C_GREEN='\033[32m' C_YELLOW='\033[33m' C_RED='\033[31m' C_CYAN='\033[36m'
else
  C_RESET='' C_BOLD='' C_DIM='' C_BLUE='' C_GREEN='' C_YELLOW='' C_RED='' C_CYAN=''
fi

ui() { echo -e "$*"; }

# ─────────────────────────────────────────────────────────────────────────────
# Structured JSON logging
# ─────────────────────────────────────────────────────────────────────────────
STRUCTURED_LOG=""
jlog() {
  [[ -n "$STRUCTURED_LOG" ]] || return 0
  local level="$1" event="$2"
  shift 2
  "$PYTHON_BIN" - "$level" "$event" "$@" <<'PY' >> "$STRUCTURED_LOG"
import json, sys
from datetime import datetime, timezone
level, event = sys.argv[1], sys.argv[2]
extra = {}
for arg in sys.argv[3:]:
    if "=" in arg:
        k, v = arg.split("=", 1)
        if v in ("true", "false"): v = v == "true"
        else:
            try: v = int(v)
            except ValueError:
                try: v = float(v)
                except ValueError: pass
        extra[k] = v
entry = {"ts": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3] + "Z", "level": level, "event": event, **extra}
print(json.dumps(entry, separators=(",", ":")))
PY
}

# ─────────────────────────────────────────────────────────────────────────────
# Config
# ─────────────────────────────────────────────────────────────────────────────
# Pre-scan for --config
for _arg in "$@"; do
  if [[ "${_prev:-}" == "--config" ]]; then CONFIG="$_arg"; break; fi
  _prev="$_arg"
done
unset _prev _arg

[[ -f "$CONFIG" ]] || fail "config not found: $CONFIG"

set -a
# shellcheck disable=SC1090
source "$CONFIG"
set +a

GOAL="${GOAL:-Improve code quality}"
SCOPE="${SCOPE:-.}"
GUARD_CMD="${GUARD_CMD:-echo ok}"
EXPERIMENT_TAG="${EXPERIMENT_TAG:-$(date -u +%Y%m%d)}"
MAX_EXPERIMENTS="${MAX_EXPERIMENTS:-50}"
YOLO="${YOLO:-0}"
WATCH_MODE=""

# Parse CLI args (override config.env values)
while [[ $# -gt 0 ]]; do
  case "$1" in
    --goal) GOAL="$2"; shift 2 ;;
    --scope) SCOPE="$2"; shift 2 ;;
    --max) MAX_EXPERIMENTS="$2"; shift 2 ;;
    --guard) GUARD_CMD="$2"; shift 2 ;;
    --watch)
      if [[ "${2:-}" == "verbose" ]]; then WATCH_MODE="verbose"; shift; else WATCH_MODE="tools"; fi
      shift ;;
    --config) shift 2 ;;  # already consumed in pre-scan
    --help|-h)
      echo "Usage: auto-improve.sh [options]"
      echo ""
      echo "Options:"
      echo "  --goal <text>    What to improve"
      echo "  --scope <dirs>   Directories to modify (space-separated)"
      echo "  --max <n>        Max experiments (default: 50)"
      echo "  --guard <cmd>    Guard command (auto-detected if omitted)"
      echo "  --watch          Show tool calls in real-time"
      echo "  --watch verbose  Show full model responses"
      echo "  --config <path>  Alternate config file"
      echo "  --help           Show this help"
      exit 0 ;;
    *) fail "Unknown option: $1" ;;
  esac
done

CLAUDE_BIN="${CLAUDE_BIN:-claude}"

# Detect CLI type: claude or codex
CLI_TYPE="claude"
case "$(basename "$CLAUDE_BIN")" in
  codex*) CLI_TYPE="codex" ;;
esac

# ─────────────────────────────────────────────────────────────────────────────
# Run directory
# ─────────────────────────────────────────────────────────────────────────────
rand4() { LC_ALL=C tr -dc 'a-z0-9' < /dev/urandom 2>/dev/null | head -c4 || echo "0000"; }

RUN_ID="$(date -u +%Y%m%d-%H%M%S)-$(rand4)"
RUN_DIR="$SCRIPT_DIR/runs/$RUN_ID"
mkdir -p "$RUN_DIR"

STRUCTURED_LOG="$RUN_DIR/events.jsonl"
EXPERIMENTS_LOG="$RUN_DIR/experiments.jsonl"
PROGRESS_FILE="$RUN_DIR/progress.txt"

{
  echo "# Auto-Improve Progress Log"
  echo "Run: $RUN_ID"
  echo "Goal: $GOAL"
  echo "Scope: $SCOPE"
  echo "Started: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "---"
} > "$PROGRESS_FILE"

# Create symlink to latest run
ln -sfn "$RUN_ID" "$SCRIPT_DIR/runs/latest"

# ─────────────────────────────────────────────────────────────────────────────
# Sentinels (PAUSE/STOP)
# ─────────────────────────────────────────────────────────────────────────────
check_sentinels() {
  if [[ -f "$RUN_DIR/STOP" ]]; then
    ui "   ${C_RED}STOP sentinel detected${C_RESET}"
    jlog "info" "run_end" "reason=STOPPED" "experiments=$exp_count" "kept=$kept_count" "elapsed=$(elapsed_time)"
    generate_summary
    exit 0
  fi
  while [[ -f "$RUN_DIR/PAUSE" ]]; do
    ui "   ${C_YELLOW}PAUSED${C_RESET} — remove $RUN_DIR/PAUSE to resume"
    sleep 5
  done
}

# ─────────────────────────────────────────────────────────────────────────────
# Git helpers
# ─────────────────────────────────────────────────────────────────────────────
ensure_branch() {
  local branch="auto-improve/${EXPERIMENT_TAG}"
  local current
  current="$(git -C "$ROOT_DIR" rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")"
  if [[ "$current" != "$branch" ]]; then
    git -C "$ROOT_DIR" checkout -b "$branch" 2>/dev/null || git -C "$ROOT_DIR" checkout "$branch" 2>/dev/null || true
  fi
  BRANCH="$branch"
}

save_checkpoint() {
  CHECKPOINT_COMMIT="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || echo "")"
}

rollback() {
  if [[ -n "${CHECKPOINT_COMMIT:-}" ]]; then
    git -C "$ROOT_DIR" reset --hard "$CHECKPOINT_COMMIT" >/dev/null 2>&1
  fi
}

# ─────────────────────────────────────────────────────────────────────────────
# Extract tags from Claude output
# ─────────────────────────────────────────────────────────────────────────────
extract_tag() {
  local tag="$1" text="$2"
  echo "$text" | "$PYTHON_BIN" -c "
import re, sys
text = sys.stdin.read()
m = re.findall(r'<$tag>(.*?)</$tag>', text, re.S)
print(m[-1].strip() if m else '')
" 2>/dev/null || echo ""
}

# ─────────────────────────────────────────────────────────────────────────────
# Status JSON (for /flow-code:loop-status)
# ─────────────────────────────────────────────────────────────────────────────
write_status_json() {
  local phase="${1:-idle}" current_exp="${2:-0}"
  "$PYTHON_BIN" - "$RUN_ID" "$current_exp" "$MAX_EXPERIMENTS" "$phase" \
    "$kept_count" "$discarded_count" "$crash_count" "$GOAL" "$SCOPE" \
    "$BRANCH" "$RUN_DIR/status.json" <<'PY'
import json, sys
from datetime import datetime, timezone
a = sys.argv[1:]
status = {
    "run_id": a[0],
    "experiment": int(a[1]),
    "max_experiments": int(a[2]),
    "phase": a[3],
    "kept": int(a[4]),
    "discarded": int(a[5]),
    "crashed": int(a[6]),
    "goal": a[7],
    "scope": a[8],
    "git_branch": a[9],
    "updated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "type": "auto-improve",
}
with open(a[10], "w") as f:
    json.dump(status, f, indent=2)
PY
  ln -sfn "$RUN_ID" "$SCRIPT_DIR/runs/latest" 2>/dev/null || true
}

# ─────────────────────────────────────────────────────────────────────────────
# Summary generator (enhanced: readable table + markdown)
# ─────────────────────────────────────────────────────────────────────────────
generate_summary() {
  local summary_file="$RUN_DIR/summary.md"
  "$PYTHON_BIN" - "$RUN_DIR/experiments.jsonl" "$GOAL" "$SCOPE" "$RUN_ID" <<'PY' > "$summary_file"
import json, sys
from pathlib import Path

log_path = Path(sys.argv[1])
goal, scope, run_id = sys.argv[2], sys.argv[3], sys.argv[4]

experiments = []
if log_path.exists():
    for line in log_path.read_text().strip().split("\n"):
        if line.strip():
            try: experiments.append(json.loads(line))
            except: pass

kept = [e for e in experiments if e.get("result") == "keep"]
discarded = [e for e in experiments if e.get("result") == "discard"]
crashed = [e for e in experiments if e.get("result") == "crash"]

print(f"# Auto-Improve Summary")
print(f"")
print(f"**Run:** {run_id}")
print(f"**Goal:** {goal}")
print(f"**Scope:** {scope}")
print(f"**Total experiments:** {len(experiments)}")
print(f"**Kept:** {len(kept)} | **Discarded:** {len(discarded)} | **Crashed:** {len(crashed)}")
if experiments:
    rate = len(kept) / len(experiments) * 100
    print(f"**Success rate:** {rate:.0f}%")
print(f"")

# Results table
if experiments:
    print(f"## Results Table")
    print(f"")
    print(f"| # | Result | Commit | Hypothesis |")
    print(f"|---|--------|--------|------------|")
    for e in experiments:
        num = e.get("num", "?")
        result = e.get("result", "?")
        commit = e.get("commit", "?")[:7]
        hyp = e.get("hypothesis", "no description")[:60]
        icon = {"keep": "KEEP", "discard": "DISC", "crash": "CRASH"}.get(result, result)
        print(f"| {num} | {icon} | {commit} | {hyp} |")
    print(f"")

if kept:
    print(f"## Improvements Kept")
    print(f"")
    for e in kept:
        print(f"- **{e.get('commit','?')[:7]}**: {e.get('hypothesis','no description')}")
    print(f"")

if discarded:
    print(f"## Experiments Discarded")
    print(f"")
    for e in discarded[:10]:
        print(f"- {e.get('hypothesis','no description')}")
    if len(discarded) > 10:
        print(f"- ...and {len(discarded)-10} more")
    print(f"")

if crashed:
    print(f"## Crashes")
    print(f"")
    for e in crashed:
        print(f"- {e.get('hypothesis','no description')}: {e.get('error','unknown')}")
PY

  ui "   ${C_GREEN}Summary:${C_RESET} $summary_file"

  # Print compact table to terminal
  if [[ -f "$EXPERIMENTS_LOG" ]]; then
    ui ""
    "$PYTHON_BIN" - "$EXPERIMENTS_LOG" <<'PY'
import json, sys
from pathlib import Path

log_path = Path(sys.argv[1])
experiments = []
for line in log_path.read_text().strip().split("\n"):
    if line.strip():
        try: experiments.append(json.loads(line))
        except: pass

if not experiments:
    sys.exit(0)

# Print table header
print(f"   {'#':>3}  {'Result':<8} {'Commit':<9} {'Hypothesis'}")
print(f"   {'─'*3}  {'─'*8} {'─'*9} {'─'*40}")
for e in experiments:
    num = str(e.get("num", "?"))
    result = e.get("result", "?").upper()
    commit = e.get("commit", "?")[:7]
    hyp = e.get("hypothesis", "")[:50]
    print(f"   {num:>3}  {result:<8} {commit:<9} {hyp}")

kept = sum(1 for e in experiments if e.get("result") == "keep")
total = len(experiments)
rate = kept / total * 100 if total else 0
print(f"\n   {kept}/{total} kept ({rate:.0f}% success rate)")
PY
    ui ""
  fi
}

# ─────────────────────────────────────────────────────────────────────────────
# Main UI
# ─────────────────────────────────────────────────────────────────────────────
ui ""
ui "${C_BOLD}${C_BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${C_RESET}"
ui "${C_BOLD}${C_BLUE}  Auto-Improve Loop${C_RESET}"
ui "${C_BOLD}${C_BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${C_RESET}"
ui ""
ui "${C_DIM}   Goal:${C_RESET} ${C_BOLD}$GOAL${C_RESET}"
ui "${C_DIM}   Scope:${C_RESET} $SCOPE"
ui "${C_DIM}   Guard:${C_RESET} $GUARD_CMD"
ui "${C_DIM}   Branch:${C_RESET} auto-improve/$EXPERIMENT_TAG"
ui "${C_DIM}   CLI:${C_RESET} $CLAUDE_BIN ($CLI_TYPE)"
ui "${C_DIM}   Max experiments:${C_RESET} $MAX_EXPERIMENTS"
ui "${C_DIM}   Run dir:${C_RESET} $RUN_DIR"
ui ""

# Setup
ensure_branch
save_checkpoint

jlog "info" "run_start" "run_id=$RUN_ID" "goal=$GOAL" "scope=$SCOPE" \
  "guard_cmd=$GUARD_CMD" "max_experiments=$MAX_EXPERIMENTS" "branch=$BRANCH"

# Write initial status
write_status_json "starting" "0"

# ─────────────────────────────────────────────────────────────────────────────
# Experiment loop
# ─────────────────────────────────────────────────────────────────────────────
exp_count=0
kept_count=0
discarded_count=0
crash_count=0

while true; do
  # Check limits
  if [[ "$MAX_EXPERIMENTS" -gt 0 && "$exp_count" -ge "$MAX_EXPERIMENTS" ]]; then
    ui ""
    ui "   ${C_GREEN}Max experiments ($MAX_EXPERIMENTS) reached.${C_RESET}"
    jlog "info" "run_end" "reason=MAX_EXPERIMENTS" "experiments=$exp_count" "kept=$kept_count" "elapsed=$(elapsed_time)"
    generate_summary
    exit 0
  fi

  check_sentinels
  exp_count=$((exp_count + 1))

  ui ""
  ui "   ${C_CYAN}${C_BOLD}Experiment $exp_count${C_RESET} ${C_DIM}($(elapsed_time) elapsed | kept=$kept_count discarded=$discarded_count crashed=$crash_count)${C_RESET}"

  # Update status for /flow-code:loop-status
  write_status_json "running" "$exp_count"

  # Save pre-experiment state
  save_checkpoint
  iter_log="$RUN_DIR/exp-$(printf '%03d' "$exp_count").log"

  # Render prompt
  prompt="$(cat "$SCRIPT_DIR/prompt_experiment.md" \
    | sed "s|{{GOAL}}|$GOAL|g" \
    | sed "s|{{SCOPE}}|$SCOPE|g" \
    | sed "s|{{GUARD_CMD}}|$GUARD_CMD|g" \
    | sed "s|{{EXPERIMENT_NUMBER}}|$exp_count|g" \
    | sed "s|{{EXPERIMENTS_LOG}}|$RUN_DIR/experiments.jsonl|g" \
    | sed "s|{{PROGRAM_MD}}|$SCRIPT_DIR/program.md|g"
  )"

  # Build CLI args (platform-aware)
  local sys_prompt="AUTO-IMPROVE MODE. You are running autonomously. Follow program.md exactly. Output <result>keep|discard|crash</result> and <hypothesis>description</hypothesis> tags."

  claude_args=()
  if [[ "$CLI_TYPE" == "codex" ]]; then
    # Codex CLI flags
    claude_args+=(-q --full-auto)
    [[ -n "${AUTO_IMPROVE_CODEX_MODEL:-}" ]] && claude_args+=(--model "$AUTO_IMPROVE_CODEX_MODEL")
  else
    # Claude Code flags
    claude_args+=(-p --output-format stream-json --verbose)
    claude_args+=(--append-system-prompt "$sys_prompt")
    [[ "$YOLO" == "1" ]] && claude_args+=(--dangerously-skip-permissions)
    [[ -n "${AUTO_IMPROVE_CLAUDE_MODEL:-}" ]] && claude_args+=(--model "$AUTO_IMPROVE_CLAUDE_MODEL")
    [[ -n "${AUTO_IMPROVE_CLAUDE_PERMISSION_MODE:-}" ]] && claude_args+=(--permission-mode "$AUTO_IMPROVE_CLAUDE_PERMISSION_MODE")
    [[ "${AUTO_IMPROVE_CLAUDE_VERBOSE:-}" == "1" ]] && claude_args+=(--verbose)
  fi

  # Run experiment
  jlog "info" "experiment_start" "num=$exp_count" "cli=$CLI_TYPE"
  set +e
  if [[ "$CLI_TYPE" == "codex" ]]; then
    # Codex: prepend system prompt to the user prompt, output is plain text
    "$CLAUDE_BIN" "${claude_args[@]}" "${sys_prompt}

${prompt}" > "$iter_log" 2>&1
  else
    "$CLAUDE_BIN" "${claude_args[@]}" "$prompt" > "$iter_log" 2>&1
  fi
  claude_rc=$?
  set -e

  # Extract text from log (handles both Claude stream-json and Codex plain text)
  claude_text="$("$PYTHON_BIN" - "$iter_log" "$CLI_TYPE" <<'PY'
import json, sys
log_path, cli_type = sys.argv[1], sys.argv[2]
out = []
try:
    with open(log_path) as f:
        if cli_type == "codex":
            # Codex: plain text output
            out.append(f.read())
        else:
            # Claude: stream-json format
            for line in f:
                try:
                    ev = json.loads(line.strip())
                    if ev.get("type") == "assistant":
                        for blk in (ev.get("message",{}).get("content") or []):
                            if blk.get("type") == "text": out.append(blk.get("text",""))
                except: pass
except: pass
print("\n".join(out))
PY
  )"

  result="$(extract_tag "result" "$claude_text")"
  hypothesis="$(extract_tag "hypothesis" "$claude_text")"

  # Default to crash if no result tag
  [[ -z "$result" ]] && result="crash"
  [[ -z "$hypothesis" ]] && hypothesis="experiment $exp_count"

  # Handle result
  case "$result" in
    keep)
      # Verify guard passes
      ui "   ${C_DIM}Running guard: $GUARD_CMD${C_RESET}"
      set +e
      (cd "$ROOT_DIR" && eval "$GUARD_CMD") > "$RUN_DIR/guard-$(printf '%03d' "$exp_count").log" 2>&1
      guard_rc=$?
      set -e

      if [[ "$guard_rc" -ne 0 ]]; then
        ui "   ${C_RED}Guard FAILED${C_RESET} — rolling back"
        rollback
        result="discard"
        discarded_count=$((discarded_count + 1))
      else
        # Commit the improvement
        git -C "$ROOT_DIR" add -A
        git -C "$ROOT_DIR" commit -m "auto-improve: $hypothesis" --no-verify >/dev/null 2>&1 || true
        commit_hash="$(git -C "$ROOT_DIR" rev-parse --short HEAD 2>/dev/null || echo "unknown")"
        kept_count=$((kept_count + 1))
        ui "   ${C_GREEN}KEEP${C_RESET} — $hypothesis (${commit_hash})"
      fi
      ;;
    discard)
      rollback
      discarded_count=$((discarded_count + 1))
      ui "   ${C_YELLOW}DISCARD${C_RESET} — $hypothesis"
      ;;
    crash|*)
      rollback
      crash_count=$((crash_count + 1))
      ui "   ${C_RED}CRASH${C_RESET} — $hypothesis"
      ;;
  esac

  # Log experiment
  commit_hash="$(git -C "$ROOT_DIR" rev-parse --short HEAD 2>/dev/null || echo "unknown")"
  "$PYTHON_BIN" -c "
import json, sys
from datetime import datetime, timezone
entry = {
    'ts': datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%S.%f')[:-3] + 'Z',
    'num': int(sys.argv[1]),
    'commit': sys.argv[2],
    'result': sys.argv[3],
    'hypothesis': sys.argv[4],
}
print(json.dumps(entry, separators=(',', ':')))
" "$exp_count" "$commit_hash" "$result" "$hypothesis" >> "$EXPERIMENTS_LOG"

  jlog "info" "experiment_done" "num=$exp_count" "result=$result" "hypothesis=$hypothesis" "commit=$commit_hash"

  # Append to progress
  {
    echo "## Experiment $exp_count — $result"
    echo "hypothesis: $hypothesis"
    echo "commit: $commit_hash"
    echo "---"
  } >> "$PROGRESS_FILE"
done
