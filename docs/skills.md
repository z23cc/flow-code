# Flow-Code Skills Reference

Skills are organized into **core** (essential workflow) and **extensions** (optional capabilities).

## Core Skills (4)

These skills form the primary workflow. They ship with the plugin.

| Skill | Command | Purpose |
|-------|---------|---------|
| `flow-code-run` | `/flow-code:run` | **Primary entry point** — unified phase loop (plan → review → work → close) |
| `flow-code` | `/flow-code` | Task/epic management entry point (list, create, status) |
| `flow-code-setup` | `/flow-code:setup` | Install flowctl CLI and configure project |
| `flow-code-map` | `/flow-code:map` | Generate codebase architecture maps |

## Extension Skills (22)

Optional capabilities that extend the core workflow. Install as needed.

### Development Extensions

| Skill | Command | Purpose |
|-------|---------|---------|
| `flow-code-debug` | `/flow-code:debug` | Systematic debugging with root cause investigation |
| `flow-code-auto-improve` | `/flow-code:auto-improve` | Autonomous code quality improvement loops |
| `flow-code-django` | `/flow-code:django` | Django-specific patterns, security, and testing |
| `flow-code-deps` | `/flow-code:deps` | Dependency graph visualization and execution order |
| `flow-code-api-design` | `/flow-code:api-design` | API design and module boundary review |
| `flow-code-brainstorm` | `/flow-code:brainstorm` | Explore and pressure-test ideas before planning |
| `flow-code-performance` | `/flow-code:performance` | Performance investigation, optimization, and benchmarks |

### Workflow Extensions

| Skill | Command | Purpose |
|-------|---------|---------|
| `flow-code-interview` | `/flow-code:interview` | Refine specs through structured Q&A |
| `flow-code-sync` | `/flow-code:sync` | Sync downstream task specs after implementation drift |
| `flow-code-retro` | `/flow-code:retro` | Post-epic retrospective and lessons learned |
| `flow-code-prime` | `/flow-code:prime` | Assess codebase readiness for agent work |

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
3. `/flow-code:run "description"` — plan, review, execute, and close (all-in-one)

That's it — `/flow-code:run` handles the full plan → review → work → review → close pipeline.

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

### Tier 3 — Interactive Decision-Making

Skills that involve planning, design, or interactive multi-step workflows.

| Skill | Purpose |
|-------|---------|
| `flow-code-run` | Unified pipeline entry point (plan, review, work, close) |
| `flow-code-brainstorm` | Explore and pressure-test ideas before planning |
| `flow-code-interview` | Refine specs through structured Q&A |
| `flow-code-api-design` | API design and module boundary review |
| `flow-code-cicd` | CI/CD pipeline setup and deployment automation |
| `flow-code-django` | Django-specific patterns, security, and testing |
| `flow-code-skill-create` | Create new flow-code skills |
| `flow-code-auto-improve` | Autonomous code quality improvement loops |
| `flow-code-sync` | Sync downstream task specs after implementation drift |
| `flow-code-ralph-init` | Scaffold autonomous Ralph harness |
| `flow-code-deprecation` | Feature and API deprecation workflows |

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
