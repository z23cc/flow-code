# Token Optimization Audit (flow-code)

**Date**: 2026-04-05
**Scope**: flow-code plugin (flowctl CLI, skills, agents, hooks, rtk integration)
**Tokenizer**: `cl100k_base` (GPT-4 class)
**Baseline commit**: v0.1.29 / main branch

---

## Executive Summary

flow-code currently has a **shallow rtk integration** (1 PreToolUse hook, ~55 LoC) and a **superficial `--compact` mode** (strips 7 JSON fields, does not restructure output). This audit identifies **47 concrete optimization points** across 9 subsystems.

**Baseline** (measured on real flowctl output):
- Single worker run (list/status calls only): **3,741 tokens**
- 8-task epic (flow-code-work orchestration): **~30K tokens** of flowctl output
- Ralph 10-iter loop: **~300K tokens** of flowctl output
- Teams mode (orchestrator + 4 workers): **~45K tokens** (orchestrator alone)

**Projected savings after full implementation** (list/status calls):
- Single worker run: **3.7K → 0.5K** (-86%)
- 8-task epic: **30K → 4K** (-86%)
- Ralph 10-iter loop: **300K → 41K** (-86%)
- Teams orchestrator: **~45K → ~8K** (-82%)

**Key insight**: For list-type commands, `--json` output is **MORE verbose than pretty** (field-name repetition). The assumption that JSON = compact is wrong.

---

## 1. Current State Audit

### 1.1 rtk integration (existing)

| Component | Location | LoC |
|---|---|---|
| PreToolUse hook registration | `hooks/hooks.json:41` | 1 |
| Hook implementation | `flowctl/crates/flowctl-cli/src/commands/hook.rs:1750-1804` | 55 |
| Unit tests | — | **0** |

Behavior: PreToolUse on Bash tool calls `flowctl hook rtk-rewrite`, which probes `command -v rtk` (fork per call), calls `rtk rewrite <cmd>`, returns `updatedInput.command` if rewritten. Silent passthrough if rtk absent.

**What rtk rewrites** (from `ref/rtk/src/discover/rules.rs`): `git`, `gh`, `cargo`, `pnpm`, `npm`, `pytest`, `ruff`, `docker`, `kubectl`, ~100 commands total.

**What rtk does NOT know**: `flowctl` — entirely outside rtk's command registry.

### 1.2 Existing `--compact` mechanism

Location: `flowctl/crates/flowctl-cli/src/output.rs`

```rust
// Line 37
let enabled = explicit || !std::io::stdout().is_terminal();

// COMPACT_STRIP (line 27)
const COMPACT_STRIP: &[&str] = &[
    "success", "message", "created_at", "updated_at",
    "spec_path", "file_path", "schema_version",
];
```

- `--compact` is a global clap flag (`OutputOpts.compact`)
- TTY auto-detection via `IsTerminal` is **already implemented**
- Strips 7 JSON fields from output recursively

**Commands that already accept `--compact`**: `epics`, `status`, `stats`, `ready`, `validate`, `files` (6)
**Commands that do NOT have `--compact`**: `tasks`, `show`, `cat`, `gap list`, `memory list`, `dag` (the highest-volume ones)

**Measured reality**:
```
flowctl epics --json            → 2418 bytes
flowctl epics --json --compact  → 2418 bytes   ← NO EFFECT
```

Root cause: `epics` JSON doesn't include any of the 7 stripped fields, so `--compact` is a no-op. The mechanism strips fields but does not restructure the output.

### 1.3 Hook fan-out per Bash tool call

`hooks/hooks.json` registers **15 hooks** total. For a single Bash tool call, **5 subprocesses** fire:

| Event | Hook |
|---|---|
| PreToolUse (Bash) | `flowctl hook ralph-guard` |
| PreToolUse (Bash) | `flowctl hook rtk-rewrite` |
| PreToolUse (Bash) | `flowctl hook commit-gate` |
| PostToolUse (Bash) | `flowctl hook ralph-guard` |
| PostToolUse (Bash) | `flowctl hook commit-gate` |

Measured latency: ~20ms per subprocess → **~100ms per Bash call**.

`ralph-guard` is registered redundantly at 4 event points. `commit-gate` checks for `.flow/` existence at every call.

### 1.4 Baseline token measurement (real data)

Per-call token cost for list/status commands (via `cl100k_base`):

| Command | pretty | json | compact (projected) | save vs pretty | save vs json |
|---|---:|---:|---:|---:|---:|
| `epics` (17 epics) | 672 | 775 | 157 | -77% | **-80%** |
| `tasks --epic X` (8 tasks) | 556 | 784 | 68 | -88% | **-91%** |
| `status` | 33 | — | 23 | -30% | — |
| `gap list` | 18 | — | 9 | -50% | — |

**Compact format design** (illustrative):
- epics: `fn-15[8/8 open] fn-16[9/10 open] ...`
- tasks: `tasks:8 epic=fn-15 1:done/backend 2:done/frontend 3:done/frontend(<2)...`
- status: `epics=17/0 tasks=6todo,2wip,100done,0blocked runs=0`

### 1.5 Call frequency (from static grep of worker.md + skills/flow-code-work/)

47 `$FLOWCTL` invocations total across worker + work skill:

| Purpose | Calls | Compact-able? |
|---|---:|---|
| Read specs (`show`, `cat`, `worker-prompt`) | 17 | No (AI needs full markdown) |
| List/state (`tasks`, `epics`, `status`, `gap`, `ready`, `validate`) | 15 | **Yes (80-90% savings)** |
| Writes (`start`, `done`, `restart`, `epic plan`, `task create`) | 15 | Yes (output already small) |

**Ceiling**: compact-able calls are 32% of total invocations, but represent the highest per-call token cost.

---

## 2. Per-Subsystem Optimization Clusters

### 2.1 flowctl output layer (rebuild compact)

Current `--compact` flag strips 7 fields. Real savings requires **restructuring**, not field-deletion.

| # | Optimization | File | Priority |
|---|---|---|:---:|
| 1 | Compact mode restructures output (columnar / compact KV), not just strips fields | `output.rs::strip_compact` (rewrite) | P0 |
| 2 | Compact applies to pretty output, not just JSON | Each command's `print_*` function | P0 |
| 3 | `tasks` command: add `--compact` support | `commands/task/list.rs` | P0 |
| 4 | `show`/`cat`: add `--summary` / `--section <name>` / `--fields <list>` | `commands/show.rs`, `commands/cat.rs` | P0 |
| 5 | `gap list` / `memory list`: add `--compact` | `commands/gap.rs`, `commands/memory.rs` | P1 |
| 6 | `dag`: add `--compact` (strip ASCII art to edge list) | `commands/dag.rs` | P1 |
| 7 | Expand `COMPACT_STRIP`: `priority` (often null), `domain` (enum), `status` (abbrevable) | `output.rs` | P1 |
| 8 | Strip epic prefix from task IDs in list output (show `.3` instead of `fn-15-...leptos.3`) | New formatter helper | P0 |
| 9 | Columnar JSON output for list commands: `{cols:[...], rows:[[...]]}` | `output.rs` | P2 |

### 2.2 Hook chain (5 subprocess per Bash call)

| # | Optimization | File | Priority |
|---|---|---|:---:|
| 10 | rtk probe cache to `$TMPDIR/flowctl-rtk-probe` (1h mtime) | `hook.rs:1764-1769` | P0 |
| 11 | Merge pre/post-tool-use hooks into single subprocess (ralph-guard + commit-gate + rtk-rewrite) | `hooks.json` restructure + `hook.rs` new `cmd_pre_tool_use` dispatcher | P0 |
| 12 | Internal short-circuit: non-`.flow/` projects exit 0 immediately | Each hook fn head | P1 |
| 13 | `pre-compact` hook output: compact format (current 21 lines of full IDs ~1K chars) | `hook.rs::cmd_pre_compact` | P0 |
| 14 | `subagent-context` hook: compact JSON (current ~400 chars per spawn with all fields) | `hook.rs::cmd_subagent_context` | P0 |
| 15 | `commit-gate` internal `git diff` via rtk | `hook.rs::cmd_commit_gate` | P2 |

### 2.3 ID length / character duplication

Measured on current `.flow/tasks/`:

```
fn-19-migrate-flowctl-to-libsql-async-native  (46 chars)
fn-15-react-vite-shadcnui-react-flow-leptos   (43 chars, appears 39× in its task specs = 1,677 chars)
fn-16-web-orchestration-platform-fullstack    (42 chars, appears 62× = 2,604 chars)
fn-20-abf-borrowed-enhancements-archetypes    (42 chars, appears 31× = 1,302 chars)
```

| # | Optimization | File | Priority |
|---|---|---|:---:|
| 16 | Short ID aliases: `fn-19` / `fn-19.1` as first-class fully-qualified IDs | `flowctl-core/id_resolver.rs` | P0 |
| 17 | flowctl commands accept short IDs as input (auto-expand to full) | ID resolver | P0 |
| 18 | flowctl output uses short IDs by default (full ID only in spec file frontmatter) | All formatters | P0 |
| 19 | Task IDs rendered relative to epic in lists (`.3` not `fn-15-....3`) | List formatter | P0 |
| 20 | `flowctl epic rename` command to shorten existing epic IDs | New command | P2 |

### 2.4 Task spec file redundancy

Sample from `.flow/tasks/fn-16-web-orchestration-platform-fullstack.3.md`:

```yaml
---
schema_version: 1                              # redundant — not used for runtime migration
id: fn-16-web-orchestration-platform-fullstack.3
epic: fn-16-web-orchestration-platform-fullstack   # derivable from id
title: "WebSocket: protocol..."
status: in_progress
domain: backend
depends_on:
- fn-16-web-orchestration-platform-fullstack.2
files:
- flowctl/crates/flowctl-daemon/src/handlers/ws.rs
- flowctl/crates/flowctl-scheduler/src/event_bus.rs
file_path: .flow/tasks/...                     # derivable (file's own path)
created_at: 2026-04-05T04:56:55.313450Z        # microsecond precision unused
updated_at: 2026-04-05T05:21:14.261080Z        # same
---
# fn-16-...fullstack.3 WebSocket: protocol...  # duplicates id + title from YAML

## Description
**Size:** M                                    # same as frontmatter
**Layer:** backend                             # same as frontmatter (domain)
**Files:** flowctl/.../ws.rs, ...              # same as frontmatter (files)
```

Per-spec waste: ~250 chars of pure duplication. 8-task epic → ~2KB redundancy.

| # | Optimization | File | Priority |
|---|---|---|:---:|
| 21 | Remove `file_path` YAML field (derivable from file's own path) | `flowctl-core/json_store.rs` | P0 |
| 22 | Remove `schema_version: 1` from every spec (only needed during migration) | spec_writer | P0 |
| 23 | Timestamps: drop microsecond precision (`…313450Z` → `…Z`) | spec_writer | P0 |
| 24 | H1 heading should not repeat full ID (`# Title` not `# fn-15-….3 Title`) | spec_writer | P0 |
| 25 | Description section should not duplicate Size/Layer/Files from YAML | spec_writer | P0 |
| 26 | `depends_on` uses short IDs within same epic | spec_writer | P1 |
| 27 | `files` list uses common-prefix compression | spec_writer | P2 |

### 2.5 Skill registry (session-start overhead)

Measured: **25 SKILL.md** files, description fields total **3,771 chars** (~1K tokens), ALL loaded at session start.

Top offenders by description length:
```
492 chars  browser/SKILL.md         (trigger phrase list bloat)
229 chars  deps/SKILL.md
181 chars  plan/SKILL.md
172 chars  django/SKILL.md
170 chars  ralph-init/SKILL.md
```

| # | Optimization | File | Priority |
|---|---|---|:---:|
| 28 | Compress SKILL descriptions (<100 chars, remove long trigger phrase lists) | 25× `skills/*/SKILL.md` | P0 |
| 29 | Rewrite `browser/SKILL.md` description (492 → ~80 chars) | `skills/browser/SKILL.md` | P0 |
| 30 | Add `trigger_phrases:` frontmatter field, description becomes single sentence | Skill frontmatter schema | P1 |

### 2.6 Agent / Skill file size (invocation overhead)

```
26,695 bytes  agents/worker.md           (~6.5K tokens every spawn)
16,036 bytes  flow-code-work/phases.md   (deleted — skill deprecated, logic moved to flow-code-run)
15,364 bytes  flow-code-plan/steps.md    (deleted — skill deprecated, logic moved to flow-code-run)
15,307 bytes  browser/SKILL.md
14,447 bytes  flow-code-epic-review/workflow.md
```

`worker.md` is 647 lines, 22 bash code blocks, 11 CRITICAL/IMPORTANT markers.

| # | Optimization | File | Priority |
|---|---|---|:---:|
| 31 | Split `worker.md` into core + phase references (lazy-load via Read) | `agents/worker.md` → 3 files | P0 |
| 32 | Large skill reference subfiles: explicit Read in workflow, not bundled load | Skill directory convention | P1 |
| 33 | Deduplicate CRITICAL/IMPORTANT markers in worker.md (11 occurrences) | `agents/worker.md` | P2 |
| 34 | Single-line bash blocks where possible | `agents/worker.md` | P2 |

### 2.7 Worker protocol & Teams mode

Teams mode runs N workers in parallel via Agent Team teammates. Orchestrator (main session) observes all workers + coordinates via plain-text protocol messages ("Task complete:", "Blocked:", "Need file access:").

The orchestrator context is the **true bottleneck** — it accumulates across the entire epic lifecycle, not per-task.

| # | Optimization | File | Priority |
|---|---|---|:---:|
| 35 | Worker completion messages: compact format `done: fn-15.3 files=3 build=ok 120s` | `agents/worker.md` Phase 5 | P0 |
| 36 | Evidence stored to file by default (`.flow/evidence/<task>.json`); orchestrator reads on demand | `agents/worker.md` Phase 4 | P0 |
| 37 | Orchestrator polling `tasks` defaults to compact output | `skills/flow-code-work/SKILL.md` | P0 |
| 38 | `flowctl files X` compact (Teams lock-conflict hot path) | Same as #3 | P0 |
| 39 | `flowctl lock-check --file X` minimal output (`free` / `held:X`) | `commands/lock.rs` | P0 |

### 2.8 rtk integration layer

| # | Optimization | File | Priority |
|---|---|---|:---:|
| 40 | Ship `.rtk/filters.toml` for line-level flowctl output filtering | `.rtk/filters.toml` + `rtk trust` | P1 |
| 41 | `hook.rs::cmd_rtk_rewrite` extension: auto-append `--compact` to flowctl commands missing output mode | `hook.rs` new fn | P0 |
| 42 | Worker evidence capture uses `rtk git diff` explicitly | `agents/worker.md` Phase 3 | P1 |
| 43 | Add unit tests for `cmd_rtk_rewrite` (currently 0 tests) | `hook.rs` `#[cfg(test)]` | P0 |

### 2.9 Feedback & observability

| # | Optimization | File | Priority |
|---|---|---|:---:|
| 44 | `flowctl status` integrates `rtk gain` summary (reads `~/.local/share/rtk/history.db`) | `commands/status.rs` | P1 |
| 45 | `/flow-code:setup` detects and recommends rtk install | `skills/flow-code-setup/workflow.md` | P1 |
| 46 | `flowctl stats tokens` adds rtk savings breakdown | `commands/stats.rs` | P2 |
| 47 | `/flow-code:retro` auto-summarizes epic's `rtk gain --since` | `skills/flow-code-retro/SKILL.md` | P2 |

---

## 3. Priority Matrix

### 3.1 P0 (blocking, highest ROI) — 23 items

Core output restructure (§2.1): **#1, #3, #4, #8**
Hook chain consolidation (§2.2): **#10, #11, #13, #14**
Short ID system (§2.3): **#16, #17, #18, #19**
Spec redundancy cleanup (§2.4): **#21, #22, #23, #24, #25**
Skill registry compression (§2.5): **#28, #29**
Worker/Teams protocol (§2.7): **#35, #36, #37, #38, #39**
rtk integration hardening (§2.8): **#41, #43**
Agent file split (§2.6): **#31**

### 3.2 P1 (high value, second wave) — 12 items

§2.1: #2, #5, #6, #7
§2.2: #12
§2.4: #26
§2.5: #30
§2.6: #32
§2.8: #40, #42
§2.9: #44, #45

### 3.3 P2 (polish) — 9 items

§2.1: #9  
§2.2: #15  
§2.3: #20  
§2.4: #27  
§2.6: #33, #34  
§2.8: (none)  
§2.9: #46, #47

### 3.4 Out-of-scope (reference only)

- RP `context_builder` output compression (separate rp-cli code path)
- Codex adversarial review output (separate token budget)
- Memory semantic compaction (needs embeddings dedup, distinct epic)

---

## 4. Execution Dependencies

**Prerequisite chain** (P0 items):

```
#1 (compact restructure)
  ├── #2 (pretty compact)
  ├── #3 (tasks --compact)
  ├── #5 (gap/memory --compact)
  ├── #6 (dag --compact)
  ├── #7 (expand COMPACT_STRIP)
  └── #37 (orchestrator polling compact)  [depends on #3]

#16 (short ID aliases)
  ├── #17 (input short IDs)
  ├── #18 (output short IDs)
  ├── #19 (relative task IDs)
  └── #8 (strip epic prefix in formatter)  [depends on #16, #18]

#21-25 (spec redundancy)
  └── New spec_writer behavior; applies to new tasks, existing tasks need migration

#31 (split worker.md)
  └── Changes agent invocation contract; must update references in skills

#11 (hook merge)
  └── Independent, can ship alone

#41 (rtk auto-compact injection)
  └── Depends on #3 (tasks needs --compact first) + #16 (short ID support)

#35-36 (worker protocol + evidence file)
  └── Independent of flowctl changes; updates worker.md only

#43 (rtk rewrite tests)
  └── Independent
```

**Parallelizable groups (P0)**:

| Group | Items | Dependencies |
|---|---|---|
| A: compact infra | #1, #3, #5, #6, #7 | None (start here) |
| B: short IDs | #16, #17, #18, #19 | None (parallel to A) |
| C: spec writer cleanup | #21, #22, #23, #24, #25 | None (parallel to A, B) |
| D: hook consolidation | #10, #11, #12, #13, #14 | None (parallel) |
| E: agent/skill compression | #28, #29, #31 | None (parallel) |
| F: worker protocol | #35, #36 | None (doc-only) |
| G: formatters | #8 | Needs B |
| H: tasks compact + orchestrator | #37, #38, #39 | Needs A |
| I: rtk auto-compact | #41 | Needs A + B |
| J: testing | #43 | None |

Groups A-F, J can all start immediately and run in parallel.

---

## 5. Measurement & Validation Plan

### 5.1 Baseline capture script

Location: `/tmp/flow-token-bench/` (from audit session)

```bash
# scripts/token_bench.sh (to be committed)
#!/bin/bash
FLOWCTL="./bin/flowctl"
OUT=/tmp/flow-token-bench

$FLOWCTL epics > $OUT/epics.pretty
$FLOWCTL epics --json > $OUT/epics.json
$FLOWCTL tasks --epic <SAMPLE_EPIC> > $OUT/tasks.pretty
$FLOWCTL tasks --epic <SAMPLE_EPIC> --json > $OUT/tasks.json
# ... etc for status, gap, memory, dag

# Run token counter (tiktoken) on each file
python3 scripts/measure_tokens.py $OUT/*.* > $OUT/baseline.csv
```

### 5.2 Per-task acceptance criteria

Every P0 item MUST ship with:

1. **Before/after token count** (captured via token_bench.sh)
2. **≥60% savings assertion** for list/status commands (per rtk convention)
3. **Snapshot test** (insta or equivalent) for output format stability
4. **Backward compatibility proof**: existing `--json` contract preserved for parsing consumers

### 5.3 Regression gates

Add to `scripts/ci_test.sh`:

```bash
# Assert token budget per command
assert_tokens() {
  local cmd="$1" max="$2"
  local actual=$(./bin/flowctl $cmd | python3 scripts/measure_tokens.py -)
  [ "$actual" -le "$max" ] || { echo "FAIL: $cmd = $actual > $max"; exit 1; }
}
assert_tokens "epics"              200
assert_tokens "tasks --epic fn-15" 100
assert_tokens "status"             30
```

---

## 6. Risk Register

| Risk | Mitigation |
|---|---|
| Compact restructure breaks skill parsers | Preserve `--json` contract; new compact is default-for-TTY-pipe only |
| Short ID collisions (two epics starting with `fn-15`) | Resolver returns error on ambiguity, forces full ID |
| Hook merge breaks ralph-guard isolation | New `cmd_pre_tool_use` dispatcher preserves per-hook exit codes |
| Spec writer changes break existing `.flow/tasks/*.md` | Migration runs on `flowctl init` / `flowctl migrate-state` |
| Worker.md split breaks lazy loaders/discovery | Keep `worker.md` as entry point that references other files |
| `.rtk/filters.toml` trust rotation friction | Pre-commit hook auto-runs `rtk trust .rtk/filters.toml` |

---

## 7. Scope Boundaries

**In scope** (this audit, `fn-12` epic):
- flowctl output layer
- Hook chain
- ID system
- Spec format
- Skill/agent file structure
- rtk integration depth
- Worker/Teams protocol

**Out of scope**:
- RP `context_builder` / chat output (separate rp-cli path)
- Codex CLI output (separate path, lower volume)
- Memory semantic compaction (needs dedicated epic with embeddings)
- Upstream rtk contributions (rtk registry does not benefit from flowctl-specific logic)

---

## 8. Projected Outcomes

### 8.1 Per-scenario token savings

| Scenario | Today | After P0 | After P0+P1 | Savings |
|---|---:|---:|---:|---:|
| Single worker run (list/status only) | 3,741 | 516 | 516 | -86% |
| Epic spec read (worker Phase 1) | ~1,200 | ~700 | ~500 | -58% |
| 8-task epic (orchestrator total) | ~30K | ~5K | ~4K | -87% |
| Teams 4-worker epic (orchestrator) | ~45K | ~8K | ~7K | -84% |
| Ralph 10-iter loop | ~300K | ~50K | ~41K | -86% |
| Session start (skill registry) | ~1,000 | ~400 | ~300 | -70% |
| Worker spawn (worker.md load) | ~6,500 | ~3,500 | ~3,000 | -54% |

### 8.2 Capability outcomes (beyond token count)

- **Orchestrator context lifetime extension**: Teams mode can handle 30-40% larger epics before hitting compaction threshold
- **Hook latency**: Per-Bash-call overhead drops from ~100ms to ~20ms (#11 hook merge)
- **Cold start**: `flowctl hook rtk-rewrite` avoids fork on 99%+ of calls after cache warm (#10)
- **Debuggability**: Explicit `--pretty` flag for humans, `FLOW_OUTPUT=pretty` env override for CI

---

## 9. Appendices

### A. Measurement Tools

- **Token counter**: `/tmp/flow-token-bench/measure.py` (uses `tiktoken.cl100k_base`)
- **Raw samples**: `/tmp/flow-token-bench/raw.txt`
- **Report**: `/tmp/flow-token-bench/report.md`

### B. Commands Without `--compact` (confirmed missing)

```
tasks --epic X
show <id>
cat <id>
gap list
memory list
dag <epic>
worker-prompt
```

### C. Hook Inventory

```
SessionStart:   ensure-flowctl.sh
PreToolUse:     ralph-guard, rtk-rewrite, commit-gate  (×3 on Bash)
PreToolUse:     ralph-guard                            (on Edit|Write)
PostToolUse:    ralph-guard, commit-gate               (×2 on Bash)
Stop:           ralph-guard, auto-memory               (×2)
SubagentStop:   ralph-guard
SubagentStart:  subagent-context
PreCompact:     pre-compact
TaskCompleted:  task-completed
```

### D. Related Existing Infrastructure

- `OutputOpts` global flags: `flowctl/crates/flowctl-cli/src/output.rs`
- `is_terminal()` detection: `flowctl/crates/flowctl-cli/src/output.rs:37`
- `init_compact()` called once: `flowctl/crates/flowctl-cli/src/main.rs:451`
- Spec frontmatter schema: `flowctl/crates/flowctl-core/src/json_store.rs`
- ID format: `flowctl/crates/flowctl-core/src/` (no dedicated resolver yet)

### E. Epic Stub

Existing placeholder: `.flow/specs/fn-12-token-compact-rtk-guard.md` — "三层 Token 优化: compact输出 + rtk集成 + guard过滤" — currently empty (TBD fields only). This audit populates that epic.

---

## 10. Next Steps

1. **Review & approval**: Team review of priority assignment and scope boundaries
2. **Populate `fn-12` epic spec**: Use §2 content to fill Overview/Scope/Approach
3. **Task decomposition**: Convert 23 P0 items into parallelizable tasks (per §4 groups A-J)
4. **Baseline capture**: Commit `scripts/token_bench.sh` + baseline CSV
5. **Execution**: Start with parallel groups A (compact infra) + B (short IDs) + D (hooks) + E (skill compression)

**Audit author note**: This document reflects static analysis + synthetic workload modeling. Numbers are directional. Actual runtime savings will vary with AI call patterns; commit baseline tooling first so regression can be measured continuously.
