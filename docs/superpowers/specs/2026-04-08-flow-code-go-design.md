# Design: `/flow-code:go` — Unified Entry Point with Brainstorm Phase

## Problem

Flow-Code's pipeline starts at Plan, but raw ideas need refinement before planning. `/flow-code:go` was documented as the full-autopilot entry point (brainstorm → plan → work → review → close) but never implemented. Meanwhile `/flow-code:run` serves as the primary user-facing command — this creates confusion.

## Solution

1. Add `Brainstorm` as the first phase in flowctl's pipeline state machine
2. Make `/flow-code:go` the sole user-facing entry point
3. Demote `/flow-code:run` to an internal skill

## Architecture

### State Machine (flowctl-core)

```
Brainstorm → Plan → PlanReview → Work → ImplReview → Close
```

All epics initialize at `Brainstorm`. The phase is either executed (new idea) or skipped (existing flow ID / spec file).

### Phase Behavior

| Input Type | Brainstorm Phase |
|------------|-----------------|
| Natural language idea | Execute: auto brainstorm (AI self-interview → requirements doc) |
| Flow ID (`fn-N`) | Skip: `flowctl phase done --phase brainstorm` |
| Spec file path | Skip: `flowctl phase done --phase brainstorm` |
| `--plan-only` | Skip brainstorm, stop after Plan |

### Command & Skill Mapping

| Layer | File | Change |
|-------|------|--------|
| Command (user-facing) | `commands/flow-code/go.md` | New — routes to `flow-code-run` skill |
| Command (removed) | `commands/flow-code/run.md` | Delete or redirect to `go` |
| Skill | `skills/flow-code-run/SKILL.md` | Add brainstorm phase handling, set `user-invocable: false` |
| Skill trigger | skill frontmatter | Update trigger list: `/flow-code:go` replaces `/flow-code:run` |

## Rust Changes

### `flowctl-core/src/pipeline.rs`

```rust
pub enum PipelinePhase {
    Brainstorm,  // NEW
    Plan,
    PlanReview,
    Work,
    ImplReview,
    Close,
}
```

- `ALL_PHASES`: 6 elements, `Brainstorm` first
- `Brainstorm.next()` → `Some(Plan)`
- `Brainstorm.as_str()` → `"brainstorm"`
- `Brainstorm.prompt_template()` → `"Explore and pressure-test the idea before planning"`
- `Brainstorm.is_terminal()` → `false`
- `PipelinePhase::parse("brainstorm")` → `Some(Brainstorm)`

### `flowctl-cli/src/commands/workflow/pipeline_phase.rs`

- `get_or_init_phase`: default init changes from `Plan` to `Brainstorm`

### Tests

- `test_phase_sequence`: starts from `Brainstorm`, expected array gains `Plan` at front
- `test_all_phases`: `len()` assertion 5 → 6
- `test_serde_roundtrip`: auto-covered
- `test_display`: add `Brainstorm` case
- `test_parse_roundtrip`: auto-covered
- `test_invalid_transition_rejection`: update examples if needed

## Skill Changes

### `skills/flow-code-run/SKILL.md`

Phase loop gains brainstorm handling:

```
when phase == "brainstorm":
  if input is natural language (not flow ID, not spec file):
    1. Gather codebase context (grep, git log, existing specs)
    2. Classify complexity (trivial/medium/large)
    3. Self-interview: 6-10 Q&A pairs grounded in code evidence
    4. Generate 2-3 approaches, auto-select best
    5. Write requirements doc to .flow/specs/<slug>-requirements.md
    6. flowctl phase done --phase brainstorm
  else:
    flowctl phase done --phase brainstorm (skip)
```

The brainstorm logic is inlined from the existing `flow-code-brainstorm` SKILL.md auto mode — not delegated via skill invocation.

### `commands/flow-code/go.md`

```yaml
---
name: go
description: "Full autopilot: brainstorm → plan → work → review → close"
---
# IMPORTANT: This command MUST invoke the skill flow-code-run
Pass $ARGUMENTS to the skill with GO_MODE=true.
```

### Flags (inherited from current `/flow-code:run`)

| Flag | Effect |
|------|--------|
| `--plan-only` | Skip brainstorm, stop after Plan |
| `--no-pr` | Skip PR creation at close |
| `--tdd` | Force test-first in worker |
| `--review=rp\|codex\|none` | Override review backend |

## Documentation Updates

| File | Change |
|------|--------|
| `CLAUDE.md` | Pipeline sequence: add Brainstorm. Entry points: `/flow-code:go` replaces `/flow-code:run` |
| `docs/skills.md` | Core Skills: replace `flow-code-run` with `flow-code-go`. Mark `flow-code-run` internal |
| `docs/flowctl.md` | Phase command docs: add `brainstorm` phase |
| `README.md` | Update pipeline diagram and usage examples |
| `README_CN.md` | Same as README.md |

## Non-Goals

- Interactive brainstorm mode (always auto, zero human input)
- Separate `flow-code-go` skill directory (reuses `flow-code-run`)
- Backward compatibility with old `.flow/` state files
