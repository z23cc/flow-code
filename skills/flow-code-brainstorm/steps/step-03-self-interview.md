# Step 3: Deep Exploration (Interview or Self-Interview)

## Interactive Mode (AUTO_MODE=false)

Ask questions **one at a time** via `AskUserQuestion`. Wait for each answer. Apply pushback rules below.

## Auto Mode (AUTO_MODE=true)

AI self-interview — no `AskUserQuestion`. All answers from codebase analysis + reasoning.

**Output contract (auto mode):**
1. Print each Q&A pair to stdout (user sees the reasoning)
2. Embed full trace in requirements doc under `## Self-Interview Trace`

---

## Phase A1: Evidence Gathering

Before questioning, gather hard evidence:

1. **Affected surface**: `flowctl find "<key terms>" --json` → list all related files
2. **Current patterns**: Read 3-5 key files. How does the codebase handle similar things?
3. **Dependencies**: Check imports, configs, shared types
4. **Test coverage**: Do tests exist? What kind? What's missing?
5. **Recent history**: `git log --oneline -20` on affected files
6. **Existing specs**: Check `.flow/specs/` and `.flow/epics/` for related work
7. **Impact graph**: `flowctl graph impact <main-file> --json` → what would break?

---

## Phase A2: Forcing Questions (Sequential, with Pushback)

Each question has **rejection criteria** — answers that are too vague MUST be re-examined. In auto mode, if the first answer falls into a rejection category, the AI must challenge itself and provide a more specific answer.

Format each as:

```
### Q1: [Question]
**First answer:** [initial response]
**Pushback:** [challenge the answer using rejection criteria]
**Refined answer:** [specific, evidence-grounded response]
```

### Q1: Demand Reality
> What's the strongest evidence this change is actually needed?

- **Reject**: "It would be nice" / "best practice says" / "users might want" / "it's cleaner"
- **Accept**: Specific failure observed, measured time waste, blocked workflow, real user complaint, production incident
- **Pushback test**: Does the answer cite a SPECIFIC event/metric, or is it hypothetical?

### Q2: Status Quo
> How is this being handled RIGHT NOW without this change?

- **Reject**: "Nothing handles this" (if nothing does, pain isn't real enough)
- **Accept**: Specific workaround described, manual steps counted, duct-tape solution identified
- **Pushback test**: If no workaround exists, WHO is suffering and HOW? If nobody, why build it?

### Q3: Narrowest Wedge
> What's the smallest version that delivers 80% of the value?

- **Reject**: "We need the full implementation" / "It won't work if incomplete"
- **Accept**: One function, one file, one config change that unblocks the core use case
- **Pushback test**: Can you ship this in < 1 day? If not, scope is too big.

### Q4: Existing Code Audit
> What already exists in the codebase that solves part of this?

- **Reject**: "Nothing relevant" (search harder — `flowctl find`, `graph refs`)
- **Accept**: Specific functions, patterns, utilities that can be reused or extended
- **Pushback test**: If >50% already exists, is this a refactor/extension rather than a new feature?

### Q5: Integration & Contracts
> What systems/modules will this touch? What contracts must be preserved?

- **Reject**: "It's self-contained" (almost nothing is — check imports, callers, config)
- **Accept**: Listed APIs, shared types, database schemas, config files affected
- **Pushback test**: Run `flowctl graph impact <file>` — is the impact bigger than expected?

### Q6: Failure Pre-mortem
> Assume this shipped and FAILED in production. Top 3 most likely causes?

- **Reject**: Vague categories ("security issues", "performance problems")
- **Accept**: Specific scenarios ("auth token not refreshed after 1hr", "N+1 query on user list page")
- **Pushback test**: For each cause, is prevention cost < fix-later cost? If not, accept the risk explicitly.

### Q7: Temporal Walk-Through
> Walk through implementation step by step:

```
Hour 1 (foundations):  What does the implementer need to know FIRST?
Hour 2-3 (core):      What ambiguities will they HIT?
Hour 4-5 (integration): What will SURPRISE them?
Hour 6+ (polish):     What will they WISH they'd planned for?
```

Surface decisions that should be resolved NOW, not during implementation.

---

## Extended Questions (Large tier only — skip for Trivial/Medium)

### Q8: Performance Impact
> Will this change affect latency, memory, or throughput?
- Check hot paths, data volume, caching layers in affected code.

### Q9: Security Surface
> Does this introduce or modify auth, data handling, or external access?
- Check auth middleware, input validation, sensitive data flows.

### Q10: Migration & Compatibility
> Are there breaking changes? Data migration needed? Feature flags?
- Check API contracts, database schemas, config formats.

---

## Phase A3: Structured Deepening

After all questions, apply ONE named reasoning method (auto-selected based on task type):

| Task type | Method | Prompt |
|-----------|--------|--------|
| New feature / spec | **Pre-mortem** | "Assume this failed 6 months later. What are the 3 most likely causes?" |
| Architecture / refactor | **First Principles** | "Strip all assumptions. What's the simplest solution from ground truth?" |
| Bug fix / reliability | **Inversion** | "How would you guarantee this fails? Now avoid those things." |
| Security / API | **Red Team** | "You're an attacker. How do you break this?" |
| Scope decision | **Constraint Removal** | "Remove all constraints (time, tech, team). What changes? What stays?" |

Append deepening insights to the self-interview trace.

---

## Pushback Scoring

After all Q&A, rate the exploration quality:

| Dimension | Score (1-5) | Criteria |
|-----------|-------------|----------|
| **Specificity** | ? | Are answers citing specific files, lines, metrics? |
| **Evidence** | ? | Are claims backed by code/git/data, not assumption? |
| **Challenge depth** | ? | Did pushback reveal anything the first answer missed? |
| **Completeness** | ? | Are all major risk areas covered? |
| **Actionability** | ? | Can a developer start implementing from these answers? |

**Total: ?/25**

- **20-25**: Excellent — proceed to approaches
- **15-19**: Good — proceed but flag weak areas for plan phase
- **<15**: Insufficient — add 2-3 follow-up Q&A pairs on the weakest dimensions

## Next Step

Read `steps/step-04-approaches.md` and execute.
