# Step 3: Deep Exploration (Interview or Self-Interview)

## Interactive Mode (AUTO_MODE=false)

Ask questions **one at a time** via `AskUserQuestion`. Wait for each answer. Apply pushback rules.

## Auto Mode (AUTO_MODE=true)

AI self-interview — no `AskUserQuestion`. All answers from codebase analysis + reasoning.

**Output contract (auto mode):**
1. Print each Q&A to stdout (user sees reasoning)
2. Embed full trace in requirements doc under `## Self-Interview Trace`

---

## Phase A1: Evidence Gathering

Before questioning, gather hard evidence:

1. `flowctl find "<key terms>" --json` → list all related files
2. Read 3-5 key files — how does the codebase handle similar things?
3. Check imports, configs, shared types for dependencies
4. `git log --oneline -20` on affected files
5. Check `.flow/specs/` and `.flow/epics/` for related prior work
6. `flowctl graph impact <main-file> --json` → what would break?
7. `flowctl graph map --json` → project overview

---

## Phase A2: Forcing Questions (5 Dimensions × 4-5 Questions Each)

Each question has **rejection criteria**. In auto mode, if the first answer is vague, challenge and refine.

Format:
```
### Q[N]: [Question]
**Answer:** [response with evidence]
**Pushback:** [challenge if answer hits rejection criteria]
**Refined:** [specific, grounded answer]
```

---

### Dimension 1: Problem Reality (4 questions)

**Q1: Demand Evidence**
> What's the strongest evidence this change is actually needed?
- Reject: "It would be nice" / "best practice" / "users might want" / "cleaner code"
- Accept: Specific failure, measured waste, blocked workflow, production incident
- Push: Does the answer cite a SPECIFIC event/metric, or is it hypothetical?

**Q2: Status Quo**
> How is this being handled RIGHT NOW without this change?
- Reject: "Nothing handles this" (no workaround = no urgency)
- Accept: Specific workaround, manual steps, duct-tape solution
- Push: Who is suffering and how often? If nobody, why build it?

**Q3: Desperate Specificity**
> Name the specific user/workflow/scenario that needs this MOST.
- Reject: Categories ("developers", "users", "the team")
- Accept: A specific role, a specific task, what fails for THEM
- Push: Can you describe their current workflow step by step?

**Q4: Cost of Inaction**
> What happens in 3 months if we do NOTHING?
- Reject: "Technical debt grows" (unmeasurable)
- Accept: "Feature X becomes impossible" / "Latency exceeds SLA" / "Team spends N hours/week on workaround"

---

### Dimension 2: Solution Space (5 questions)

**Q5: Narrowest Wedge**
> What's the smallest version that delivers 80% of the value?
- Reject: "We need the full implementation"
- Accept: One function/file/config that unblocks the core use case
- Push: Can this ship in <1 day? If not, cut more.

**Q6: Existing Code Audit**
> What already exists in the codebase that solves part of this?
- Reject: "Nothing relevant" (search harder — `flowctl find`, `graph refs`)
- Accept: Specific functions, patterns, utilities to reuse or extend
- Push: If >50% exists, is this an extension/refactor rather than new build?

**Q7: Alternative Approaches**
> What are 2 completely different ways to solve this?
- Reject: Variations of the same approach ("do it in module A vs module B")
- Accept: Genuinely different architectures (e.g., polling vs webhooks, batch vs streaming)
- Push: What would a developer who disagrees with your approach suggest?

**Q8: Non-Goals**
> What should this explicitly NOT do?
- Reject: "It should do everything the user needs" (scope creep)
- Accept: Explicit boundaries ("no UI", "no migration", "read-only first")
- Push: Check `.flow/project-context.md` Non-Goals — does this align?

**Q9: Precedent**
> Has this been attempted before in this codebase? What happened?
- Check: git log for reverted commits, closed PRs, abandoned specs
- Accept: "Attempted in fn-X but abandoned because Y" → learn from it

---

### Dimension 3: Risk & Failure (4 questions)

**Q10: Pre-mortem**
> Assume this shipped and FAILED. Top 3 most likely causes?
- Reject: Vague categories ("security issues", "performance problems")
- Accept: "Auth token not refreshed after 1hr", "N+1 query on list page"
- Push: For each cause — is prevention cheaper than fix-later?

**Q11: Blast Radius**
> If this goes wrong, what's the worst that happens?
- Reject: "It breaks" (too vague)
- Accept: "Data loss in X table" / "All API calls return 500" / "Users locked out"
- Push: Is there a rollback plan? Feature flag? Database backup?

**Q12: Edge Cases**
> What are the 3 trickiest edge cases?
- Must be specific: empty inputs, concurrent access, large payloads, timezone issues, Unicode, null values
- Push: Which edge case is the one that teams ALWAYS forget?

**Q13: Security & Privacy**
> Does this touch auth, user data, external APIs, or file system?
- If yes: What's the attack surface? Input validation? Secrets management?
- If no: Confirm by checking imports — really no auth/data handling?

---

### Dimension 4: Implementation (4 questions)

**Q14: Integration Points**
> What systems/modules will this touch? What contracts must be preserved?
- Reject: "It's self-contained" (check imports, callers, config)
- Accept: Listed APIs, shared types, database schemas, config files
- Push: `flowctl graph impact <file>` — is impact bigger than expected?

**Q15: Temporal Walk-Through**
> Walk through implementation hour by hour:
- Hour 1 (setup): What does the implementer need to know FIRST?
- Hour 2-3 (core): What ambiguities will they HIT?
- Hour 4-5 (integration): What will SURPRISE them?
- Hour 6+ (polish): What will they WISH they'd planned for?

**Q16: Testing Strategy**
> What test types are needed? What's the current coverage gap?
- Reject: "We'll add tests" (what KIND?)
- Accept: "Unit tests for parser, integration test for API endpoint, E2E for user flow"
- Push: Is there an existing test pattern to follow? (`flowctl find "test" --json`)

**Q17: Dependencies & Ordering**
> What must happen BEFORE this can start? What's blocked until this is done?
- Check: Other in-progress epics, required migrations, API changes
- Push: Can any dependency be parallelized?

---

### Dimension 5: Long-term (3 questions) — Large tier only

**Q18: Maintainability**
> In 6 months, will a new developer understand this code without the author?
- Push: What documentation/comments are needed? What's the non-obvious part?

**Q19: Scalability**
> What breaks first at 10x load? 100x? 
- Accept: "Database becomes bottleneck at 10K concurrent users"
- Push: What's the cheapest mitigation (caching, pagination, async)?

**Q20: Future-Fit**
> If the project's direction changes in 1 year, does this become MORE or LESS useful?
- Push: Is this a building block or a dead end?

---

## Phase A3: Structured Deepening

After all questions, apply ONE named reasoning method (auto-selected):

| Task type | Method | Prompt |
|-----------|--------|--------|
| New feature / spec | **Pre-mortem** | "Assume this failed 6 months later. 3 most likely causes?" |
| Architecture / refactor | **First Principles** | "Strip all assumptions. Simplest solution from ground truth?" |
| Bug fix / reliability | **Inversion** | "How would you guarantee this fails? Avoid those things." |
| Security / API | **Red Team** | "You're an attacker. How do you break this?" |
| Scope decision | **Constraint Removal** | "Remove all constraints. What changes?" |

Append insights to self-interview trace.

---

## Quality Scoring

Rate the exploration (1-5 per dimension):

| Dimension | Score | Criteria |
|-----------|-------|----------|
| **Problem Reality** (Q1-4) | /5 | Specific evidence, not hypothetical? |
| **Solution Space** (Q5-9) | /5 | Alternatives explored, non-goals defined? |
| **Risk & Failure** (Q10-13) | /5 | Concrete failure modes, not categories? |
| **Implementation** (Q14-17) | /5 | Temporal walk-through done, deps mapped? |
| **Long-term** (Q18-20) | /5 | Maintainability and scalability addressed? |
| **Total** | /25 | |

- **20-25**: Excellent — proceed to approaches
- **15-19**: Good — proceed but flag weak dimensions
- **<15**: Insufficient — add 2-3 follow-up Q&A on weakest dimension, re-score

**Adaptive tier sizing:**
- **Trivial**: Q1-6 only (6 questions, ~3 min)
- **Medium**: Q1-17 (17 questions, ~8 min)
- **Large**: Q1-20 all (20 questions, ~12 min)

## Next Step

Read `steps/step-04-approaches.md` and execute.
