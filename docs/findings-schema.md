# Findings JSON Schema Reference

Canonical schema for structured review findings used by all flow-code reviewer agents and skills. Compatible with the CE (Compound Engineering) findings format.

Source of truth: `flowctl-core/src/review_protocol.rs` (`ReviewFinding` struct).

---

## Finding Object

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `severity` | `string` enum | yes | -- | `P0` \| `P1` \| `P2` \| `P3`. See [Severity Levels](#severity-levels). |
| `category` | `string` | yes | -- | Domain tag: `security`, `performance`, `logic`, `testing`, `architecture`, `maintainability`, `style`. |
| `description` | `string` | yes | -- | Issue title, 100 characters max. |
| `file` | `string` | no | -- | Relative path from repo root. Omitted when finding is project-wide. |
| `line` | `integer` | no | -- | Line number (>= 1). Omitted when not line-specific. |
| `confidence` | `float` | no | `0.8` | Reviewer certainty, 0.0-1.0. See [Confidence Thresholds](#confidence-thresholds). |
| `autofix_class` | `string` enum | no | `manual` | `safe_auto` \| `gated_auto` \| `manual` \| `advisory`. See [Autofix Classes](#autofix-classes). |
| `owner` | `string` enum | no | `review-fixer` | `review-fixer` \| `downstream-resolver` \| `human` \| `release`. See [Owner Routing](#owner-routing). |
| `evidence` | `string[]` | no | `[]` | Code-grounded proof. At least 1 item recommended. |
| `pre_existing` | `boolean` | no | `false` | True when the issue exists in unchanged code, unrelated to the current diff. |
| `requires_verification` | `boolean` | no | `false` | True when any fix must be re-verified with targeted tests or follow-up review. |
| `suggested_fix` | `string` | no | -- | Concrete minimal fix, if obvious. Omitted when not applicable. |
| `why_it_matters` | `string` | no | -- | Impact and failure mode: what breaks, not what is wrong. |
| `reviewer` | `string` | no | -- | Persona name that produced this finding (e.g. `correctness-reviewer`). |

Only three fields are strictly required: `severity`, `category`, and `description`. All others have serde defaults and can be omitted in JSON input.

---

## Severity Levels

| Level | Meaning | Example | Action |
|-------|---------|---------|--------|
| `P0` | Critical breakage, exploitable vulnerability, data loss/corruption | SQL injection, infinite loop on main path, silent data deletion | Must fix before merge |
| `P1` | High-impact defect likely hit in normal usage | Breaking API contract, wrong return type, auth bypass on common flow | Should fix |
| `P2` | Moderate issue with meaningful downside | Edge-case panic, O(n^2) in hot path, missing error propagation | Fix if straightforward |
| `P3` | Low-impact, narrow scope, minor improvement | Unused import, naming inconsistency, docs typo | Author's discretion |

---

## Confidence Thresholds

Findings below the minimum threshold for their severity are suppressed by `filter_by_confidence()`.

| Range | Label | Gate rule |
|-------|-------|-----------|
| < 0.50 | Suppress always | Never reported, any severity |
| 0.50 - 0.59 | P0 exception | Reported only if severity is `P0` (min_confidence = 0.5) |
| 0.60 - 0.69 | Flag | Included for all severities; mark clearly actionable |
| 0.70 - 0.84 | Confident | Real and important; default confidence is 0.8 |
| 0.85 - 1.00 | Certain | Verifiable from code alone |

Implementation: `Severity::min_confidence()` returns `0.5` for P0 and `0.6` for P1-P3.

---

## Autofix Classes

| Class | JSON value | Who fixes | Behavior |
|-------|------------|-----------|----------|
| Safe auto | `safe_auto` | Automated tooling | Local, deterministic fix. Applied without approval. |
| Gated auto | `gated_auto` | Automated + approval | Concrete fix exists but changes behavior or contracts. Needs human sign-off. |
| Manual | `manual` | Developer | Actionable but requires design decisions or cross-cutting changes. |
| Advisory | `advisory` | Nobody (informational) | Surfaced in report. No code change expected. |

Restrictiveness order (lower = more automatable): `safe_auto(0)` < `gated_auto(1)` < `manual(2)` < `advisory(3)`.

---

## Owner Routing

| Owner | JSON value | Next action |
|-------|------------|-------------|
| Review fixer | `review-fixer` | The in-skill fixer applies the fix when autofix policy allows. |
| Downstream resolver | `downstream-resolver` | Converted into residual work (task/issue) for later resolution. |
| Human | `human` | A person must make a judgment call. |
| Release | `release` | Operational or rollout follow-up, not a code fix. |

---

## Merge Pipeline

When multiple reviewers produce findings, the merge pipeline combines them into a single deduplicated, prioritized list. The pipeline has seven steps:

1. **Validate** -- Reject findings missing required fields (`severity`, `category`, `description`).
2. **Confidence gate** -- Apply `filter_by_confidence()`: drop findings below their severity's minimum threshold.
3. **Dedup** -- Fingerprint each finding and collapse duplicates (see [Fingerprinting](#fingerprinting)).
4. **Boost** -- When multiple reviewers flag the same issue, take the highest severity and highest confidence across duplicates.
5. **Route** -- Assign `owner` using autofix class and review-mode policy (e.g. safe_auto findings get `review-fixer`).
6. **Partition** -- Split into actionable findings (`safe_auto`, `gated_auto`, `manual`) vs. informational (`advisory`).
7. **Sort** -- Order by severity (P0 first via `Severity::sort_key()`), then by confidence descending.

---

## Fingerprinting

Dedup uses a three-part fingerprint to identify duplicate findings across reviewers:

```
fingerprint = normalize(file) + line_bucket(line, ±3) + normalize(description)
```

- **normalize(file)**: Lowercase, strip leading `./`.
- **line_bucket(line, tolerance)**: Round line number to nearest bucket of size 3 (e.g. lines 10-12 map to the same bucket). Findings without a line match on file + description only.
- **normalize(description)**: Lowercase, strip trailing punctuation, collapse whitespace.

Two findings with the same fingerprint are considered duplicates. The boost step then merges their metadata.

---

## Example

A complete finding as JSON:

```json
{
  "severity": "P0",
  "category": "security",
  "description": "SQL injection via unsanitized user input",
  "file": "src/db.rs",
  "line": 42,
  "confidence": 0.95,
  "autofix_class": "safe_auto",
  "owner": "review-fixer",
  "evidence": [
    "Line 42: format!(\"SELECT * FROM users WHERE id = {}\", user_input)",
    "user_input flows from request handler without sanitization"
  ],
  "pre_existing": false,
  "requires_verification": true,
  "suggested_fix": "Use parameterized query: sqlx::query(\"SELECT * FROM users WHERE id = $1\").bind(user_input)",
  "why_it_matters": "Allows arbitrary SQL execution against the production database",
  "reviewer": "security-reviewer"
}
```

Minimal valid finding (all defaults applied):

```json
{
  "severity": "P2",
  "category": "performance",
  "description": "Unnecessary allocation in hot loop"
}
```

This deserializes with `confidence: 0.8`, `autofix_class: "manual"`, `owner: "review-fixer"`, `evidence: []`, `pre_existing: false`, `requires_verification: false`.
