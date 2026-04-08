# Step 3: Interview (Interactive or Self-Interview)

## Interactive Mode (AUTO_MODE=false)

Original behavior — ask user questions via `AskUserQuestion`.

### Phase 1: Pressure Test

Ask exactly 3 questions, **one at a time**, using `AskUserQuestion` for each.

**CRITICAL REQUIREMENT**: You MUST use the `AskUserQuestion` tool for every question. Do NOT output questions as plain text — they will be ignored.

Wait for each answer before asking the next question.

#### Question 1: Who and why?
> Who uses this? What's the specific pain point or motivation?

#### Question 2: Cost of inaction?
> What happens if we do nothing? What's the actual cost or risk?

#### Question 3: Simpler framing?
> Is there a simpler version that delivers 80% of the value? What's the minimum viable version?

After all 3 answers, summarize the key insights in 2-3 bullets before proceeding.

---

## Auto Mode (AUTO_MODE=true)

AI self-interview — no `AskUserQuestion` calls. All answers derived from codebase analysis, best practices, and reasoning.

**Output contract (auto mode):**
1. Print Q&A pairs to **stdout** so the user sees the reasoning in conversation
2. Embed Q&A pairs in the requirements doc under a `## Self-Interview Trace` section
3. Requirements doc written to `.flow/specs/${SLUG}-requirements.md` (same as interactive)

### Phase A1: Deep Code Analysis

Before self-interview, gather evidence:

1. **Affected surface**: Grep/Glob for all files related to the request. List them.
2. **Current patterns**: How does the codebase currently handle similar functionality? Read 3-5 key files.
3. **Dependencies**: What modules/packages/APIs are involved? Check imports, configs.
4. **Test coverage**: Do tests exist for the affected area? What kind?
5. **Recent history**: `git log --oneline -20` on affected files — who changed what, why?
6. **Existing specs**: Check `.flow/specs/` and `.flow/epics/` for related prior work.

### Phase A2: Self-Interview

Ask and answer questions in structured Q&A format. Output each as a visible block:

```
### Q: <question>
**A:** <answer with code evidence>
```

**Core questions (always ask all):**

#### 1. Problem & Users
> Q: Who uses this and what specific pain point does it solve?
> A: Derive from codebase context — who calls the affected code, what user-facing behavior it impacts.

#### 2. Cost of Inaction
> Q: What happens if we do nothing? What breaks or degrades?
> A: Check for open issues, error patterns, performance trends, tech debt signals in the code.

#### 3. Simpler Framing
> Q: Is there a simpler version that delivers 80% of the value?
> A: Analyze the request — what's the minimum change that solves the core problem? What can be deferred?

#### 4. Existing Patterns
> Q: How does the codebase currently handle similar problems?
> A: Cite specific files, functions, patterns found in Phase A1. Quote code if relevant.

#### 5. Integration Points
> Q: What other systems/modules will this touch? What contracts must be preserved?
> A: List APIs, shared types, database schemas, config files that are affected.

#### 6. Edge Cases & Failure Modes
> Q: What can go wrong? What are the boundary conditions?
> A: Analyze error handling in current code, identify missing cases, concurrency risks.

**Extended questions (Large tier only):**

#### 7. Performance Impact
> Q: Will this change affect latency, memory, or throughput?
> A: Analyze hot paths, data volume, caching layers in affected code.

#### 8. Security Surface
> Q: Does this introduce or modify authentication, authorization, or data handling?
> A: Check for auth middleware, input validation, sensitive data flows.

#### 9. Migration & Compatibility
> Q: Are there breaking changes? Do we need data migration or feature flags?
> A: Check API contracts, database schemas, config formats for backwards compatibility.

#### 10. Testing Strategy
> Q: What test types are needed and what's the current coverage gap?
> A: Analyze existing test files for the affected area, identify missing test categories.

**Adaptive follow-ups**: If any answer reveals unexpected complexity (e.g., a shared module with 10+ consumers, no test coverage, concurrency issues), add 1-2 follow-up Q&A pairs to drill into that specific area. Cap at 15 total Q&A pairs.

## Next Step

Read `steps/step-04-approaches.md` and execute.
