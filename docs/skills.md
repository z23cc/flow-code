# Flow-Code Skills Reference

Skills are organized into **core** (essential workflow) and **extensions** (optional capabilities).

## Core Skills (7)

These skills form the primary workflow. They ship with the plugin.

| Skill | Command | Purpose | Location |
|-------|---------|---------|----------|
| `flow-code-go` | `/flow-code:go` | **Full autopilot** — brainstorm → plan → work → review → close → PR (sole user-facing entry point) | `skills/` |
| `flow-code-run` | Internal | Internal pipeline engine, invoked by `/flow-code:go` | `skills/` |
| `flow-code-plan` | `/flow-code:plan` | Research codebase and create epic with tasks | `codex/skills/` |
| `flow-code-work` | `/flow-code:work` | Execute tasks with re-anchoring, reviews, wave checkpoints | `codex/skills/` |
| `flow-code` | `/flow-code` | Task/epic management entry point (list, create, status) | `skills/` |
| `flow-code-setup` | `/flow-code:setup` | Install flowctl CLI and configure project | `skills/` |
| `flow-code-map` | `/flow-code:map` | Generate codebase architecture maps | `skills/` |

## Extension Skills (47)

Optional capabilities that extend the core workflow. Skills live in `skills/` and/or `codex/skills/` (54 unique skills total, 7 core + 47 extension).

### Review & Quality

| Skill | Command | Purpose |
|-------|---------|---------|
| `flow-code-plan-review` | Internal | Cross-model plan validation (RP or Codex) |
| `flow-code-impl-review` | Internal | Per-task implementation review with fix loop |
| `flow-code-epic-review` | Internal | Epic completion adversarial gate |
| `flow-code-code-review` | Internal | Five-axis scoring with severity labels (Worker Phase 6, impl-review, PR review) |

### Development Extensions

| Skill | Command | Purpose |
|-------|---------|---------|
| `flow-code-brainstorm` | `/flow-code:brainstorm` | Explore and pressure-test ideas (interactive or `--auto` self-interview) |
| `flow-code-debug` | `/flow-code:debug` | Systematic debugging with root cause investigation |
| `flow-code-auto-improve` | `/flow-code:auto-improve` | Autonomous code quality improvement loops |
| `flow-code-django` | `/flow-code:django` | Django-specific patterns, security, and testing |
| `flow-code-deps` | `/flow-code:deps` | Dependency graph visualization and execution order |
| `flow-code-api-design` | `/flow-code:api-design` | API design and module boundary review |
| `flow-code-performance` | `/flow-code:performance` | Performance investigation, optimization, and benchmarks |
| `flow-code-tdd` | `/flow-code:tdd` | Test-first methodology, Prove-It Pattern, coverage strategy |
| `flow-code-incremental` | Internal | Vertical slicing, incremental commits, Implement-Test-Verify-Commit cycle |
| `flow-code-simplify` | `/flow-code:simplify` | Reduce code complexity while preserving exact behavior |
| `flow-code-frontend-ui` | `/flow-code:frontend-ui` | Production-quality UI: components, layouts, state, accessibility |

### Architecture & Infrastructure Skills

| Skill | Command | Purpose |
|-------|---------|---------|
| `flow-code-security` | Internal | OWASP Top 10, three-tier security boundaries |
| `flow-code-auth` | Internal | Authentication, authorization, OAuth, JWT, RBAC, session management |
| `flow-code-database` | Internal | Schema design, migrations, query optimization, ORM patterns |
| `flow-code-caching` | Internal | HTTP cache, CDN, Redis, in-memory, cache invalidation strategy |
| `flow-code-containerization` | Internal | Dockerfiles, docker-compose, Kubernetes, image optimization |
| `flow-code-microservices` | Internal | Service boundaries, inter-service communication, saga, event-driven |
| `flow-code-realtime` | Internal | WebSocket, SSE, long-polling, connection management, scaling |
| `flow-code-state-management` | Internal | Frontend/full-stack state architecture, tool selection, patterns |
| `flow-code-error-handling` | Internal | Error classification, retry logic, circuit breakers, graceful degradation |
| `flow-code-i18n` | Internal | Multi-language support, locale formatting, RTL layouts |
| `flow-code-monitoring` | Internal | Dashboards, SLOs/SLIs, alerting rules, on-call runbooks |
| `flow-code-observability` | Internal | Logging, tracing, metrics, health endpoints |
| `flow-code-documentation` | Internal | ADRs, API docs, READMEs, changelogs, doc-as-code workflow |

### Workflow Extensions

| Skill | Command | Purpose |
|-------|---------|---------|
| `flow-code-interview` | `/flow-code:interview` | Refine specs through structured Q&A (40+ questions) |
| `flow-code-sync` | `/flow-code:sync` | Sync downstream task specs after implementation drift |
| `flow-code-retro` | `/flow-code:retro` | Post-epic retrospective and lessons learned |
| `flow-code-prime` | `/flow-code:prime` | Assess codebase readiness for agent work (8 pillars, 48 criteria) |
| `flow-code-autoplan` | `/flow-code:autoplan` | Multi-perspective auto-review pipeline (CEO, eng, design, DX) |
| `flow-code-qa` | `/flow-code:qa` | Visual QA testing with browser automation |
| `flow-code-design-review` | `/flow-code:design-review` | Visual design audit with browser automation |
| `flow-code-guide` | Internal | Skill discovery flowchart — helps select the right skill |

### Tooling Extensions

| Skill | Command | Purpose |
|-------|---------|---------|
| `flow-code-ralph-init` | `/flow-code:ralph-init` | Scaffold autonomous Ralph harness |
| `flow-code-loop-status` | `/flow-code:loop-status` | Monitor running Ralph/auto-improve loops |
| `flow-code-worktree-kit` | `/flow-code:worktree-kit` | Git worktree management for parallel work |
| `flow-code-export-context` | `/flow-code:export-context` | Export context for external model review |
| `flow-code-rp-explorer` | `/flow-code:rp-explorer` | RepoPrompt-powered codebase exploration |
| `flow-code-skill-create` | `/flow-code:skill-create` | Create new flow-code skills |
| `flow-code-prompt-eng` | Internal | Prompt engineering guidance for review agents |
| `flow-code-cicd` | `/flow-code:cicd` | CI/CD pipeline setup, quality gates, and deployment automation |
| `flow-code-context-eng` | `/flow-code:context-eng` | Context window management and optimization |
| `flow-code-deprecation` | `/flow-code:deprecation` | Feature, API, and module deprecation workflows |
| `browser` | `/browser` | Browser automation via agent-browser CLI |

## Recommended Usage Order

For a new project:

1. `/flow-code:setup` — install and configure
2. `/flow-code:prime` — assess codebase readiness
3. `/flow-code:go "idea"` — full autopilot: brainstorm → plan → work → review → close → PR

Or for more control:

1. `/flow-code:brainstorm --auto "idea"` — AI self-interview, produces requirements doc
2. `/flow-code:plan .flow/specs/<slug>-requirements.md` — research + create tasks
3. `/flow-code:work fn-N` — execute tasks

`/flow-code:go fn-N` resumes an existing epic (brainstorm auto-skipped).

## Tier Classification

Skills are classified into four tiers based on complexity and decision-making authority, inspired by the gstack tiering system. The `tier` field in each skill's YAML frontmatter enables tooling to filter, prioritize, and display skills by capability level.

### Tier 1 — Basic Tools

Simple utilities and wrappers that perform a single focused action.

| Skill | Purpose |
|-------|---------|
| `flow-code-setup` | Install flowctl CLI and configure project |
| `flow-code-worktree-kit` | Git worktree management |
| `flow-code-export-context` | Export context for external model review |
| `flow-code-deps` | Dependency graph visualization |
| `flow-code-loop-status` | Monitor running loops |
| `flow-code-guide` | Skill discovery flowchart |
| `browser` | Browser automation via agent-browser CLI |
| `flow-code-rp-explorer` | RepoPrompt-powered codebase exploration |

### Tier 2 — Observation & Monitoring

Skills that gather information, analyze codebases, or provide monitoring and guidance.

| Skill | Purpose |
|-------|---------|
| `flow-code-map` | Generate codebase architecture maps |
| `flow-code-prime` | Assess codebase readiness for agent work |
| `flow-code-context-eng` | Context window management and optimization |
| `flow-code-performance` | Performance investigation and benchmarks |
| `flow-code-prompt-eng` | Prompt engineering guidance for review agents |
| `flow-code-debug` | Systematic debugging with root cause investigation |
| `flow-code-monitoring` | Dashboards, SLOs/SLIs, alerting rules |
| `flow-code-observability` | Logging, tracing, metrics, health endpoints |
| `flow-code-documentation` | ADRs, API docs, changelogs, doc-as-code |

### Tier 3 — Interactive Decision-Making

Skills that involve planning, design, or interactive multi-step workflows.

| Skill | Purpose |
|-------|---------|
| `flow-code-go` | Full autopilot (brainstorm → plan → work → review → close) |
| `flow-code-run` | Unified pipeline entry point (plan, review, work, close) |
| `flow-code-plan` | Research codebase and create epic with tasks |
| `flow-code-work` | Execute tasks with re-anchoring and wave checkpoints |
| `flow-code-brainstorm` | Explore and pressure-test ideas (interactive or --auto) |
| `flow-code-interview` | Refine specs through structured Q&A |
| `flow-code-api-design` | API design and module boundary review |
| `flow-code-cicd` | CI/CD pipeline setup and deployment automation |
| `flow-code-django` | Django-specific patterns, security, and testing |
| `flow-code-skill-create` | Create new flow-code skills |
| `flow-code-auto-improve` | Autonomous code quality improvement loops |
| `flow-code-sync` | Sync downstream task specs after implementation drift |
| `flow-code-ralph-init` | Scaffold autonomous Ralph harness |
| `flow-code-deprecation` | Feature and API deprecation workflows |
| `flow-code-plan-review` | Cross-model plan validation |
| `flow-code-impl-review` | Per-task implementation review |
| `flow-code-epic-review` | Epic completion adversarial gate |
| `flow-code-code-review` | Five-axis scoring with severity labels |
| `flow-code-qa` | Visual QA testing with browser |
| `flow-code-design-review` | Visual design audit with browser |
| `flow-code-autoplan` | Multi-perspective auto-review pipeline |
| `flow-code-tdd` | Test-first methodology, Prove-It Pattern |
| `flow-code-incremental` | Vertical slicing, incremental commits |
| `flow-code-simplify` | Reduce complexity while preserving behavior |
| `flow-code-frontend-ui` | Production-quality UI components and layouts |
| `flow-code-security` | OWASP Top 10, security boundaries |
| `flow-code-auth` | Authentication, authorization, OAuth, JWT, RBAC |
| `flow-code-database` | Schema design, migrations, query optimization |
| `flow-code-caching` | HTTP cache, CDN, Redis, cache invalidation |
| `flow-code-containerization` | Docker, Kubernetes, image optimization |
| `flow-code-microservices` | Service boundaries, saga, event-driven patterns |
| `flow-code-realtime` | WebSocket, SSE, connection management |
| `flow-code-state-management` | Frontend/full-stack state architecture |
| `flow-code-error-handling` | Error classification, retry, circuit breakers |
| `flow-code-i18n` | Multi-language support, locale formatting, RTL |

### Tier 4 — Final Decisions & Execution

Skills that make final judgments, complete workflows, or serve as primary management entry points.

| Skill | Purpose |
|-------|---------|
| `flow-code` | Task and epic management entry point |
| `flow-code-retro` | Post-epic retrospective and lessons learned |
| `flow-code-autoplan` | Multi-perspective auto-review pipeline |

## Template Generation

Skills can be generated from `.tmpl` template files with `{{PLACEHOLDER}}` markers that resolve to shared content. This keeps common patterns (preamble, flowctl path, review protocols) in sync across all skills.

### Creating a Template

1. Copy the existing `SKILL.md` to `SKILL.md.tmpl` in the same directory
2. Replace shared content with placeholder markers (see table below)
3. Add `{{GENERATED_NOTICE}}` after the frontmatter to mark the file as auto-generated
4. Run the generation script to produce the `SKILL.md`

### Available Placeholders

| Placeholder | Resolves To |
|-------------|-------------|
| `{{GENERATED_NOTICE}}` | `<!-- AUTO-GENERATED from SKILL.md.tmpl — DO NOT EDIT DIRECTLY -->` |
| `{{FLOWCTL_PATH}}` | `FLOWCTL="$HOME/.flow/bin/flowctl"` |
| `{{SKILL_NAME}}` | Extracted from the template's frontmatter `name:` field |
| `{{PREAMBLE}}` | Contents of `skills/_shared/preamble.md` |
| `{{RP_REVIEW_PROTOCOL}}` | Contents of `skills/_shared/rp-review-protocol.md` |

### Running Generation

```bash
# Generate all SKILL.md files from their .tmpl sources
bash scripts/gen-skill-docs.sh

# Preview what would change without writing
bash scripts/gen-skill-docs.sh --dry-run

# Check if generated files are up to date (for CI)
bash scripts/gen-skill-docs.sh --check
```

### CI Freshness Check

Add to your CI pipeline to ensure generated files stay in sync:

```bash
bash scripts/check-skill-freshness.sh
```

This exits with code 1 if any `SKILL.md` does not match its `.tmpl` source, preventing stale generated files from being committed.
