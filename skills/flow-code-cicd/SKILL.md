---
name: flow-code-cicd
description: "Use when setting up, modifying, or troubleshooting CI/CD pipelines, quality gates, or deployment automation"
---

# CI/CD Patterns

## Overview

Automate quality gates so no change reaches production without passing verification. CI/CD is the enforcement mechanism for every other skill -- it catches what humans and agents miss, consistently on every change. Gates ordered from fastest to slowest; shift left to catch problems at the cheapest stage.

## When to Use

- Setting up a new project's CI pipeline
- Adding or modifying quality gates
- Configuring deployment pipelines or staged rollouts
- Integrating feature flags for deploy/release separation
- Debugging CI failures or slow pipelines
- **Especially when:** no CI exists yet, pipeline exceeds 10 minutes, deploys are manual

**When NOT to use:**
- One-off manual deployments that won't recur
- Local-only development with no shared branches
- For test design details, see `references/testing-patterns.md`

## Core Process

### Phase 1: Design Quality Gate Pipeline

Order gates from fastest to slowest. Every gate that fails stops the pipeline -- no skipping.

```
Change Pushed
    |
    v
+-------------------+
|  1. LINT           |  Seconds. Catches style, unused vars, formatting.
|  2. TYPE CHECK     |  Seconds. Catches type errors statically.
|  3. UNIT TESTS     |  Seconds-minutes. Catches logic errors.
|  4. BUILD          |  Minutes. Catches compilation/bundling errors.
|  5. INTEGRATION    |  Minutes. Catches cross-component failures.
|  6. E2E (optional) |  Minutes. Catches user-facing regressions.
|  7. SECURITY AUDIT |  Minutes. Catches vulnerable deps, secrets.
+-------------------+
    |
    v
  Ready for review
```

**No gate can be skipped.** If lint fails, fix lint -- don't disable the rule. If a test fails, fix the code -- don't skip the test.

### Phase 2: Apply Shift-Left Principle

Catch problems as early as possible. A bug caught in linting costs minutes; the same bug caught in production costs hours.

**Local pre-commit gate** -- run before pushing:

```bash
$FLOWCTL guard
```

This runs all configured guards (lint, type-check, tests) locally before code leaves the developer's machine. CI then re-runs the same checks as a safety net, not the first line of defense.

**Pipeline optimization rules:**
1. Cheapest checks first (lint before build, unit before integration)
2. Parallelize independent gates (lint + type-check can run simultaneously)
3. Cache dependencies aggressively
4. Use path filters to skip irrelevant jobs (docs-only PRs skip e2e)

### Phase 3: Configure Pipeline

Platform-agnostic principles with a GitHub Actions example:

**Generic pipeline pattern** (adapt to any CI system):

```
trigger: pull_request, push to main
stages:
  - stage: fast-checks (parallel)
    jobs: [lint, type-check]
  - stage: test
    jobs: [unit-test, build]
    depends_on: fast-checks
  - stage: verify
    jobs: [integration, security-audit]
    depends_on: test
  - stage: deploy-staging (auto on main)
    depends_on: verify
  - stage: deploy-production (manual gate or auto after staging)
    depends_on: deploy-staging
```

**GitHub Actions example:**

```yaml
# .github/workflows/ci.yml
name: CI
on:
  pull_request:
    branches: [main]
  push:
    branches: [main]

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Lint
        run: |
          # Language-specific: cargo clippy, npm run lint, ruff check, etc.
          echo "Run your linter here"

  typecheck:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Type check
        run: |
          # cargo check, npx tsc --noEmit, mypy, etc.
          echo "Run your type checker here"

  test:
    needs: [lint, typecheck]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Unit tests
        run: |
          # cargo test, npm test, pytest, go test, etc.
          echo "Run your tests here"
      - name: Build
        run: |
          # cargo build --release, npm run build, go build, etc.
          echo "Run your build here"

  security:
    needs: [test]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Security audit
        run: |
          # cargo audit, npm audit, pip-audit, govulncheck
          echo "Run your security audit here"
```

See `references/security-checklist.md` for the full security verification checklist including dependency auditing, secret scanning, and OWASP Top 10.

### Phase 4: Configure Feature Flags

Feature flags decouple deployment from release. Deploy code without enabling it.

```
Feature flag lifecycle:
  Create flag → Enable for testing → Canary (1%) → Staged rollout → Full rollout → Remove flag

Flag rules:
  - Every flag gets a cleanup date at creation
  - Flags older than 30 days without rollout trigger review
  - Dead flags (fully rolled out) are tech debt -- remove them
```

**Why flags matter:**
- Ship code to main without enabling it (reduces branch lifetime)
- Roll back without redeploying (disable the flag)
- Canary new features safely (1% of users first)
- A/B test behavior differences

### Phase 5: Set Up Staged Rollouts

Never deploy to 100% of users at once.

```
Staged rollout:
  1%  → Monitor 15 min → errors? → Rollback
  10% → Monitor 30 min → errors? → Rollback
  50% → Monitor 1 hour → errors? → Rollback
  100% → Continue monitoring
```

**Rollback requirements:**
- Every deployment must be reversible within 5 minutes
- Rollback procedure documented and tested (not just "we'll figure it out")
- Rollback does not require a new build (revert to previous artifact)

### Phase 6: Add Monitoring and Alerting

Deployment without monitoring is flying blind.

```
Post-deploy checklist:
  [ ] Error rate monitoring active
  [ ] Latency monitoring active
  [ ] Key business metrics dashboarded
  [ ] Alerting configured for anomalies
  [ ] Runbook exists for common failure modes
```

**CI pipeline health:**
- Track pipeline duration over time -- if it grows past 10 minutes, optimize
- Monitor flaky test rate -- flaky tests erode trust in CI
- Alert on pipeline failures in main branch (broken main = emergency)

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "CI is too slow" | Slow CI means wrong gate order. Fastest checks first. Parallelize. Cache. A 5-minute pipeline prevents hours of debugging. |
| "We'll add tests to CI later" | CI without tests is just a build server. Tests are the point of CI. |
| "Feature flags add complexity" | Deploying untested code to everyone adds more complexity. Flags give you a kill switch. |
| "We don't need staging" | Staging catches issues that local environments can't reproduce. Network, data volume, concurrency -- staging surfaces them. |
| "Manual deploy is fine for now" | Manual processes are forgotten, skipped, and inconsistent. Automate on day one. |
| "This change is trivial, skip CI" | Trivial changes break builds. CI is fast for trivial changes anyway. |
| "The test is flaky, just re-run" | Flaky tests mask real bugs and waste everyone's time. Fix the flakiness. See `references/testing-patterns.md` for test isolation patterns. |
| "We'll figure out rollback if we need it" | You need it. Plan rollback before the first deploy, not during an outage. |

## Red Flags

- No CI pipeline in the project -- every shared project needs automated verification
- CI failures ignored or silenced ("it's probably flaky")
- Tests disabled in CI to make the pipeline pass
- Production deploys without staging verification
- No rollback mechanism -- deploying is a one-way door
- Secrets hardcoded in CI config files instead of a secrets manager
- Pipeline duration growing unchecked past 10 minutes
- Feature flags with no cleanup dates accumulating as dead code
- Main branch broken for hours with no one investigating

## Verification

After setting up or modifying a CI/CD pipeline, confirm:

- [ ] All quality gates present and ordered fastest-to-slowest (lint, types, tests, build, integration, security)
- [ ] `$FLOWCTL guard` configured as local pre-commit gate
- [ ] Pipeline runs on every PR and push to main
- [ ] Failures block merge (branch protection or equivalent configured)
- [ ] Secrets stored in secrets manager, not in code or CI config (see `references/security-checklist.md`)
- [ ] Deployment has a tested rollback mechanism
- [ ] Pipeline completes in under 10 minutes for the standard test suite
- [ ] Feature flags have documented cleanup dates
- [ ] Post-deploy monitoring and alerting are active

For performance gates in CI, see the `flow-code-performance` skill.
For test design and patterns, see `references/testing-patterns.md`.
