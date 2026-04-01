# Prompt Blocks

Reusable XML blocks for composing Codex/GPT review and task prompts.
Use selectively — not every prompt needs every block.

## Core

### `<task>`

Use in every prompt. Define the job, context, and end state.

```xml
<task>
Describe the concrete job, relevant context, and what done looks like.
</task>
```

### `<role>`

Set the reviewer/agent stance.

```xml
<role>
You are performing [type] review. Your job is to [objective].
</role>
```

## Output Contracts

### `<structured_output_contract>`

Use when findings need to be machine-parseable (reviews, audits).

```xml
<structured_output_contract>
Return a JSON object:
{
  "verdict": "SHIP" | "NEEDS_WORK",
  "summary": "terse assessment",
  "findings": [{
    "severity": "critical|high|medium|low",
    "title": "short title",
    "file": "path",
    "line_start": N,
    "line_end": N,
    "confidence": 0.0-1.0,
    "recommendation": "concrete fix"
  }],
  "next_steps": ["action items"]
}
</structured_output_contract>
```

### `<compact_output_contract>`

Use when brief, human-readable output is preferred.

```xml
<compact_output_contract>
Return:
1. verdict (one word)
2. findings ordered by severity
3. next step (one sentence)
</compact_output_contract>
```

## Calibration

### `<calibration_rules>`

Use in reviews to prevent noisy findings.

```xml
<calibration_rules>
- critical: plausible concrete failure scenario, not theoretical
- high: affects correctness/data/security in normal operation
- medium: real weakness, unusual conditions or limited blast radius
- low: genuine concern, unlikely user-visible harm
- Cannot construct a scenario → downgrade or drop
- Fewer strong findings beat many weak ones
</calibration_rules>
```

### `<final_check>`

Use as the last block before output. Forces self-verification.

```xml
<final_check>
Before emitting, verify:
1. Every finding references a real file and line from the diff
2. Every finding has a concrete failure scenario
3. Severity ratings match calibration rules
4. Verdict is consistent with findings
5. Output is valid JSON
6. No invented files or code paths
</final_check>
```

## Grounding & Safety

### `<grounding_rules>`

Use in reviews and research to prevent hallucination.

```xml
<grounding_rules>
Every claim must be defensible from provided context.
Do not invent files, code paths, or runtime behavior.
If a conclusion depends on inference, state that explicitly.
</grounding_rules>
```

### `<action_safety>`

Use in write-capable tasks to prevent scope creep.

```xml
<action_safety>
Stay narrow. Fix only what the task asks for.
Do not refactor adjacent code, rename variables, or "improve" things not in scope.
If you find a related issue, report it — do not fix it.
</action_safety>
```

## Verification

### `<verification_loop>`

Use for debugging, implementation, or risky fixes.

```xml
<verification_loop>
After implementing a fix:
1. Run the failing test/command
2. Confirm it passes
3. Run adjacent tests to check for regressions
4. If any fail, diagnose and fix before finalizing
</verification_loop>
```

### `<completeness_contract>`

Use when partial answers are unacceptable.

```xml
<completeness_contract>
The task is not done until:
- All acceptance criteria are met
- Tests pass
- No regressions introduced
If blocked, report what is missing rather than delivering partial work.
</completeness_contract>
```

## Context & Research

### `<operating_stance>`

Set the reviewer's default attitude.

```xml
<operating_stance>
Default to skepticism.
Assume the change can fail in subtle, high-cost ways until evidence says otherwise.
Do not give credit for good intent or likely follow-up work.
</operating_stance>
```

### `<attack_surface>`

Prioritize where to look for problems.

```xml
<attack_surface>
Focus on:
- auth, permissions, trust boundaries
- data loss, corruption, irreversible state changes
- rollback safety, retries, idempotency gaps
- race conditions, stale state, re-entrancy
- empty-state, null, timeout, degraded dependencies
- schema drift, migration hazards
- observability gaps hiding failure
</attack_surface>
```

### `<citation_rules>`

Use for research tasks to keep claims anchored.

```xml
<citation_rules>
Every factual claim must cite a source (URL, file path, or documentation reference).
If no source exists, mark as "unverified" and state confidence level.
Do not present inference as fact.
</citation_rules>
```

## Input Context

### `<diff_summary>`, `<diff_content>`, `<embedded_files>`

Provide code context. Always use placeholders:

```xml
<diff_summary>{{diff_summary}}</diff_summary>
<diff_content>{{diff_content}}</diff_content>
<embedded_files>{{embedded_files}}</embedded_files>
```
