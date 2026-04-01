# Prompt Recipes

Copy the smallest recipe that fits the task. Trim blocks you don't need.

## Adversarial Review (existing: `adversarial-review.md`)

Blocks: `role` + `task` + `operating_stance` + `attack_surface` + `review_method` + `finding_bar` + `calibration_rules` + `structured_output_contract` + `final_check` + `grounding_rules`

Use when: reviewing code changes with skepticism, trying to find reasons NOT to ship.

## Security Review

```xml
<role>
You are performing a security-focused code review.
Your job is to find vulnerabilities, not validate correctness.
</role>

<task>
Review the code changes for security vulnerabilities.
Focus on: injection, auth bypass, data exposure, SSRF, path traversal.
{{focus_block}}
</task>

<attack_surface>
Focus exclusively on:
- SQL/NoSQL injection, command injection, template injection
- Authentication bypass, authorization escalation, IDOR
- Sensitive data exposure (PII, tokens, credentials in logs/responses)
- SSRF, path traversal, redirect open
- Cryptographic weaknesses (weak hashing, hardcoded secrets)
- Deserialization, prototype pollution, supply chain
</attack_surface>

<calibration_rules>
- critical: exploitable without authentication or with low-privilege user
- high: exploitable but requires specific conditions or elevated access
- medium: potential weakness, requires chained attacks or insider access
- low: defense-in-depth improvement, not directly exploitable
</calibration_rules>

<structured_output_contract>
[Use standard schema from blocks.md]
</structured_output_contract>

<final_check>
[Use standard final_check from blocks.md]
</final_check>

<grounding_rules>
[Use standard grounding_rules from blocks.md]
</grounding_rules>
```

## Performance Review

```xml
<role>
You are performing a performance-focused code review.
Your job is to find scalability issues and inefficiencies.
</role>

<task>
Review the code changes for performance problems.
Focus on: N+1 queries, unbounded loops, missing pagination, memory leaks.
{{focus_block}}
</task>

<attack_surface>
Focus on:
- Database: N+1 queries, missing indexes, full table scans, large JOINs
- Memory: unbounded collections, leaked references, large allocations per request
- I/O: synchronous blocking, missing timeouts, no connection pooling
- Concurrency: lock contention, thread pool exhaustion, connection limits
- Caching: missing cache, cache invalidation bugs, thundering herd
- Pagination: unbounded result sets, offset pagination at scale
</attack_surface>

<calibration_rules>
- critical: O(N²) or worse in hot path, unbounded memory growth
- high: N+1 in common request, missing index on frequent query
- medium: suboptimal but bounded, affects latency not correctness
- low: micro-optimization, measurable only under extreme load
</calibration_rules>

<compact_output_contract>
Return:
1. verdict (SHIP or NEEDS_WORK)
2. findings ordered by impact on p99 latency
3. estimated user-visible effect of each finding
</compact_output_contract>
```

## Diagnosis / Debugging

```xml
<task>
Diagnose why {{description}} is failing.
Use the provided context to identify the root cause.
</task>

<compact_output_contract>
Return:
1. most likely root cause (one sentence)
2. evidence supporting this conclusion
3. smallest safe fix
4. how to verify the fix works
</compact_output_contract>

<verification_loop>
Before finalizing:
1. Confirm the root cause explains ALL observed symptoms
2. Confirm the fix addresses the root cause, not a symptom
3. Check for side effects of the proposed fix
</verification_loop>

<grounding_rules>
[Use standard grounding_rules from blocks.md]
</grounding_rules>
```

## Implementation Task

```xml
<task>
Implement {{description}}.
Follow existing patterns in the codebase.
</task>

<completeness_contract>
Done means:
- All acceptance criteria met
- Tests pass (existing + new)
- No regressions
- Code follows project conventions
If blocked, report the blocker — do not deliver partial work.
</completeness_contract>

<action_safety>
[Use standard action_safety from blocks.md]
</action_safety>

<verification_loop>
[Use standard verification_loop from blocks.md]
</verification_loop>
```
