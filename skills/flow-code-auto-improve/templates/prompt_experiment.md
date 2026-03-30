You are running one auto-improve experiment (experiment #{{EXPERIMENT_NUMBER}}).

## Setup

1. Read your improvement program:
```bash
cat {{PROGRAM_MD}}
```

2. Read previous experiment results (learn from history):
```bash
cat {{EXPERIMENTS_LOG}} 2>/dev/null || echo "No previous experiments"
```

2b. Analyze experiment history for patterns:
- Count how many were kept vs discarded vs crashed
- What file paths appear most in kept experiments? Do more there.
- What hypothesis types were discarded? Avoid repeating those.
- If 3+ consecutive discards on similar themes, switch to a different improvement area.

3. Read codebase map (if exists — architecture overview, module guide, gotchas):
```bash
cat docs/CODEBASE_MAP.md 2>/dev/null | head -200 || echo "No codebase map (run /flow-code:map to create one)"
```

4. Read git log to see what improvements have been kept:
```bash
git log --oneline -20
```

## Baseline & Progress

**Current project metrics** (compare your changes against these):
{{BASELINE_METRICS}}

**Progress**: {{PROGRESS_PCT}}% through experiment budget (experiment #{{EXPERIMENT_NUMBER}})

**Strategy based on progress:**
- **Early (0-20%)**: Easy wins — dead code, unused imports, obvious lint fixes, missing type hints
- **Middle (20-80%)**: Target goal — focus on {{GOAL}}
- **Late (80-100%)**: Cleanup — simplification, consistency, remove TODOs, tighten types

## Experiment Steps

**Step 1: Discover**
Read code in scope (`{{SCOPE}}`) and find ONE concrete improvement opportunity.
Do NOT repeat hypotheses that were already discarded in experiments.jsonl.

**Step 2: Hypothesize**
State your hypothesis clearly. Output:
```
<hypothesis>Doing X will improve Y because Z</hypothesis>
```

**Step 3: Implement**
- Write a test first if possible (TDD style)
- Make the minimal code change
- You may ONLY modify files in: `{{SCOPE}}`
- You may READ any file for context

**Step 4: Guard**
Run the guard command — it MUST pass:
```bash
{{GUARD_CMD}}
```
If guard fails, try to fix. If still fails after 2 attempts, output `<result>crash</result>`.

**Step 5: Judge**
**Quantitative check** (use if metrics available in baseline):
- Tests count maintained or increased? → keep signal
- Lint errors decreased or maintained? → keep signal
- Type errors increased? → strong discard signal (don't introduce new type errors)

Based on the program.md criteria, decide:
- `<result>keep</result>` — improvement is real, focused, and simple
- `<result>discard</result>` — not worth it (complexity > benefit, marginal, or speculative)
- `<result>crash</result>` — something went wrong that you can't fix

## Rules

- ONE improvement per experiment. Do not batch multiple changes.
- Follow the simplicity criterion in program.md strictly.
- If you run out of ideas in scope, try a different angle (security, tests, performance, readability).
- Do NOT ask the human anything. You are fully autonomous.
- Do NOT output `<result>keep</result>` unless the guard command passes.

## Output

You MUST output both tags before finishing:
1. `<hypothesis>...</hypothesis>` — what you tried
2. `<result>keep|discard|crash</result>` — the outcome
3. `<metrics>{"tests_added": N, "lint_fixed": N, "type_errors_delta": N}</metrics>` — quantitative impact estimate (use 0 if unknown)
