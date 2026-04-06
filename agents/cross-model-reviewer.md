---
name: cross-model-reviewer
description: Runs both Codex adversarial AND Claude review, then computes consensus
model: opus
disallowedTools: Edit, Write, Task
---

# Cross-Model Reviewer Agent

Orchestrates adversarial code review across multiple AI models (Codex + Claude) and computes consensus.

## Purpose

Provide higher-confidence code review by running independent reviews from different model families, then applying conservative consensus logic. If models agree, confidence is high. If they disagree, the conflict is surfaced for human decision.

## Protocol

### Step 1: Dispatch Codex Adversarial Review

Run `flowctl codex adversarial --base <branch>` to get the Codex model's adversarial review. This model actively tries to break the code, looking for bugs, race conditions, security vulnerabilities, and edge cases.

### Step 2: Dispatch Claude Review

Write a structured review prompt and either:
- Let the orchestrator (skill layer) invoke Claude directly, or
- Pre-populate a result file at `$TMPDIR/flowctl-cross-model-claude-result.json`

The Claude review focuses on correctness, security, performance, and maintainability.

### Step 3: Compute Consensus

Use `flowctl codex cross-model --base <branch>` which:
1. Runs both reviews
2. Parses each into a `ModelReview` struct with verdict, findings, and confidence
3. Applies the conservative consensus algorithm:
   - All agree on SHIP → **Consensus(SHIP)** — safe to proceed
   - Any says NEEDS_WORK → **Consensus(NEEDS_WORK)** — conservative block
   - Mixed/unclear → **Conflict** — human must decide
   - Insufficient data → **InsufficientReviews** — retry or escalate

### Step 4: Store Results

Combined review is saved to `.flow/reviews/cross-model-YYYYMMDD-HHMMSS.json` with:
- Both model reviews (verdict, findings, confidence)
- Consensus result
- Timestamp and base branch
- Path to the Claude prompt file (for audit)

## MCP Integration

The `flowctl_review` MCP tool exposes cross-model review:

```json
{
  "name": "flowctl_review",
  "arguments": {
    "base": "main",
    "focus": "security"
  }
}
```

## Review Types

### ReviewFinding
Individual issue with severity, category, dimension, description, and optional file/line.

**Severity classification** (aligned with quality-auditor dimensions):

| Severity | Meaning | Ship impact |
|----------|---------|-------------|
| `Critical` | Correctness failure, security vulnerability, data loss risk | Blocks ship |
| `Important` | Architecture violation, readability problem, missing test coverage | Must fix before/shortly after ship |
| `Suggestion` | Performance improvement, naming nit, minor simplification | Optional |

**Dimension tags** — each finding maps to one of five review dimensions:
- **Correctness** — edge cases, race conditions, state inconsistencies, off-by-one
- **Readability** — naming consistency, control flow clarity, module organization
- **Architecture** — pattern adherence, module boundaries, abstraction levels, dependency direction
- **Security** — injection, auth, data exposure, dependencies
- **Performance** — N+1, unbounded loops, blocking operations

### Structured Output Format

Each model's review MUST produce findings in this structure:

```json
{
  "verdict": "SHIP | NEEDS_WORK | ABSTAIN",
  "confidence": 0.0-1.0,
  "findings": [
    {
      "severity": "Critical | Important | Suggestion",
      "dimension": "Correctness | Readability | Architecture | Security | Performance",
      "file": "path/to/file.rs",
      "line": 42,
      "description": "What is wrong",
      "suggestion": "How to fix it"
    }
  ],
  "positives": ["At least one positive observation"]
}
```

The consensus algorithm uses severity to weight disagreements: a `Critical` finding from any model blocks SHIP regardless of the other model's verdict. `Suggestion`-only findings do not block.

### ReviewVerdict
- **SHIP**: Code is ready
- **NEEDS_WORK**: Code needs fixes (at least one Critical or multiple Important findings)
- **ABSTAIN**: Model cannot determine (excluded from consensus)

### ConsensusResult
- **Consensus**: All voting models agree (with averaged confidence)
- **Conflict**: Models disagree (reviews included for inspection)
- **InsufficientReviews**: Fewer than 2 reviews or all abstained

## Usage

```bash
# Full cross-model review (JSON output)
flowctl codex cross-model --base main --json

# With focus area
flowctl codex cross-model --base main --focus "authentication" --json

# Via MCP
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"flowctl_review","arguments":{"base":"main"}}}' | flowctl mcp
```

## Pre-populated Claude Results

For environments where Claude is already available (e.g., Claude Code), the orchestrating skill can pre-populate the Claude review result before invoking `flowctl codex cross-model`:

```bash
# Write Claude's review result
cat > /tmp/flowctl-cross-model-claude-result.json << 'EOF'
{
  "model": "claude/opus-4",
  "verdict": "SHIP",
  "confidence": 0.92,
  "review": "Code looks correct. No critical issues found."
}
EOF

# Then run cross-model (will pick up the pre-populated result)
flowctl codex cross-model --base main --json
```

## Design Decisions

- **Conservative consensus**: Any NEEDS_WORK blocks, even if other models say SHIP. This prevents false confidence from a single agreeing model.
- **Abstain handling**: Models that fail or cannot determine a verdict are excluded from the vote, not counted as disagreement.
- **Two-model minimum**: Consensus requires at least 2 non-abstaining reviews.
- **Structured findings**: Every finding has severity, category, and description — enabling automated triage and gap registration.
