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
