# Changelog

All notable changes to Flow-Code are documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/). Versions use [Semantic Versioning](https://semver.org/).

## [0.1.49] - 2026-04-10

### Changed
- RP agent_run pipeline integration, ADR/spec commands, shared RP orchestration guide, context engineering, skill expansions, and numerous flowctl improvements across core+CLI crates.

## [0.1.48] - 2026-04-10

### Fixed
- Aligned release/install version metadata across Cargo, Claude plugin manifests, Codex plugin manifest, and `flowctl-version` pin.
- Corrected stale local marketplace source path to point at this repository root.
- Updated `flowctl/README.md` repository/install links and architecture description to match the current workspace.

### Changed
- Added CI/release preflight checks for version parity and install-surface drift (`scripts/check-release-surface.sh`).

## [0.1.46] - 2026-04-09

### Added
- **47 forcing questions** across entire pipeline (brainstorm 20Q + plan_review 10Q + impl_review 10Q + close 7Q)
- Every question has reject/accept criteria + mandatory pushback
- Quantitative scoring gates: brainstorm /25, plan_review /30, impl_review /30, close /21
- Brainstorm 5 dimensions: Problem Reality, Solution Space, Risk & Failure, Implementation, Long-term
- Plan review: Premise Challenge (4Q) + Architecture Interrogation (6Q)
- Impl review: Correctness (5Q) + Quality (5Q) + 3-layer parallel review
- Close: Ship-readiness interrogation with security grep, impact analysis, rollback plan
- Adaptive tier sizing: Trivial 6Q, Medium 17Q, Large 20Q (brainstorm)

### Fixed
- **P0: State directory resolution** — `get_flow_dir()` now walks up directory tree (fixes state loss in subdirectories)
- **P0: State recovery** — `flowctl recover --epic <id>` rebuilds task status from git history
- **P1: Guard fallback** — missing tools → "skipped" not "failed" (doesn't block pipeline)
- **P1: Review-backend verify** — rp-cli/codex not in PATH → auto-fallback to "none"
- **P2: Slug length** — max 40 → 20 characters (shorter task IDs)
- **P2: Brainstorm auto-skip** — trivial tasks (≤10 words, "fix"/"typo") skip brainstorm
- **P2: --interactive flag** — pause at key decisions for user confirmation

## [0.1.45] - 2026-04-09

### Added
- **Code graph persistent index** (`flowctl graph build/update/refs/impact/map`) — 1407 symbols, 107K edges, bincode persistence to `.flow/graph.bin`, incremental update via git diff
- **`flowctl find`** — intent-level search that auto-routes: regex → index regex, symbol → graph refs, literal → trigram, fallback → nucleo fuzzy
- **`flowctl edit`** — intent-level edit: exact str::replacen first, fuzzy fudiff fallback
- **`flowctl graph refs`** — find all references to a symbol (<16ms from cache)
- **`flowctl graph impact`** — transitive impact analysis: what files break if you change this file (BFS depth 3)
- **N-gram index optimized**: bincode serialization (6.2MB → 502KB, 12x smaller), memchr verification (2-5x faster), regex→trigram extraction for indexed regex search
- **`flowctl index regex`** — indexed regex search via trigram pre-filtering (12ms)
- **10 ADRs** with YAML frontmatter (verify + scope) and 5 invariant checks
- **project-context.md maximized** — now read by all pipeline stages (brainstorm/plan/review/worker/close)
- **Memory type classification** — `--type pitfall|convention|decision|general`, auto-capture pitfalls on NEEDS_WORK
- **Quick Commands enforced** — Worker Phase 6 and close phase run epic smoke tests
- **repo-map default unlimited** — outputs all ranked symbols by default
- Deep comparison docs: flow-code vs flow-next, fff.nvim lessons, ADR strategy, Rust crate optimization research

### Changed
- Skills/agents updated to use intent-level API (`find`/`graph refs`/`graph impact`/`edit`) instead of raw tool commands
- Tool priority: native Claude Code tools (Grep/Glob/Read/Edit) first, flowctl for unique capabilities only
- Pipeline alignment fixes: checklist wired into Worker Phase 10 + close, memory docs corrected, frecency docs corrected

### Dependencies
- Added: `bincode` 2, `memchr` 2, `regex-syntax` 0.8

## [0.1.44] - 2026-04-09

### Added
- **`flowctl search`** — Fuzzy file search with nucleo-matcher + frecency scoring + git status boost + ignore (.gitignore-aware). `--git modified|staged|untracked` filter, `--limit N`
- **`flowctl index`** — N-gram trigram inverted index for fast text search. `build` (56ms for 145 files), `status`, `search` (<1ms per query). Persistent `.flow/index/ngram.bin`
- **`flowctl code-structure`** — Regex-based symbol extraction (functions, structs, traits, classes) across 9 languages (Rust, Python, JS, TS, Go, Java, C, C++, Ruby)
- **`flowctl repo-map`** — PageRank-ranked symbol overview within token budget. Builds file-level reference graph, outputs top signatures grouped by file
- **`flowctl patch`** — Fuzzy diff/patch via fudiff. `diff` (generate), `apply` (context-based, tolerates drift), `replace` (3-tier fallback: exact → whitespace-normalized → context-based)
- **`flowctl doctor` enhanced** — 9 check categories: binary, flow-dir, review backends, git status, state integrity (orphaned tasks, stale locks), project-context, search tools, external tools
- **Frecency scoring** — Exponential decay (14-day half-life), auto-tracked on task completion. Files modified/accessed recently rank higher in search
- Agent skills updated: repo-scout, context-scout, worker, plan step-02, brainstorm step-02, code-review now use `flowctl search/index/code-structure/repo-map/patch` as primary tools

### Dependencies
- Added: `nucleo-matcher` 0.3, `ignore` 0.4, `fudiff` 0.2, `memmap2` 0.9 (workspace-level)
- Zero new deps for code-structure/repo-map (uses existing regex + petgraph + ignore)

## [0.1.43] - 2026-04-08

### Added
- **7 BMAD-METHOD patterns** adopted from deep analysis of BMAD-METHOD v6.2.2
- `flowctl write-file` — Pipeline file I/O command (bypasses Claude Code permission prompts for zero-interaction pipelines)
- `flowctl checklist` — Structured Definition of Done with init/check/uncheck/verify/show subcommands (8 default items across 4 categories)
- `project-context.md` support — Shared technical standards document auto-loaded by Worker Phase 2 re-anchoring (template in `templates/`)
- Zero-findings-halt review rule — Reviewers must find issues; zero findings triggers NEEDS_REANALYSIS
- Three-layer parallel code review — Blind Hunter (diff-only) + Edge Case Hunter (boundary analysis) + Acceptance Auditor (spec compliance)
- Advanced elicitation methods in brainstorm — Pre-mortem, First Principles, Inversion, Red Team, Constraint Removal, Stakeholder Mapping
- `--quick` flag for `/flow-code:go` — Fast path skipping brainstorm/plan for trivial changes
- Step-file architecture — 15 step files across plan (5), work (5), brainstorm (5) for JIT loading
- 3 deep comparison documents: flow-code vs compound-engineering, flow-code vs BMAD-METHOD, BMAD lessons analysis

### Changed
- Skills now use step-file workflow (JIT loading) instead of monolithic steps.md/phases.md
- Removed legacy workflow files (steps.md, phases.md, examples.md) in favor of steps/ directories
- `flowctl parse-findings` now detects zero findings and returns NEEDS_REANALYSIS verdict
- Worker agent Phase 2 now reads `.flow/project-context.md` if present
- `flowctl init` now hints about project-context.md template

## [0.1.42] - 2026-04-08

### Added
- Confidence calibration review framework (CE-compatible multi-persona dispatch + merge pipeline)
- `reviewer` field on ReviewFinding for persona attribution
- `fingerprint()` method for finding deduplication (file + line_bucket +-3 + normalized title)
- `merge_findings()` pipeline: confidence gate, dedup, cross-reviewer boost (+0.10), conservative routing, sort
- `partition_findings()` for autofix routing (fixer_queue / residual_queue / report_only)
- `flowctl review merge --files` CLI command to merge multi-reviewer outputs
- 6 reviewer agent definitions: correctness, security, testing, performance, maintainability, architecture
- Multi-Persona Review Mode section in code-review skill
- Findings JSON schema reference doc (`docs/findings-schema.md`)
- Integration test script with 14 tests / 32 assertions (`scripts/review_merge_test.sh`)

### Fixed
- `--deps` and `dep add` now auto-expand short IDs (e.g., `fn-42.1` → `fn-42-full-slug.1`)
- Improved error messages with Hint showing correct full ID format

## [0.1.41] - 2026-04-08

### Added
- 12 engineering skills across 4 domains: security, performance, database, observability
- 5 engineering skills: TDD, incremental development, error handling, state management, caching
- Frontend UI engineering skill for production-quality interfaces
- References directory with security, performance, and accessibility checklists
- Pre-launch checklist and skill discovery guide
- QA and design-review browser automation skills
- Multi-perspective autoplan review skill (CEO, eng, design, DX)
- Template generation system with placeholder resolution
- Memory auto-inject with levels and tag reference
- Shared preamble startup sequence template
- Tier metadata to all skill frontmatter
- Brainstorm phase and `/flow-code:go` full-autopilot entry point

### Changed
- Unified CLI arg style: positional for primary entity, flags for filters
- Improved review-backend defaults and skill docs clarity
- Wired frontend-ui skill into plan/work pipeline
- Enforced zero-interaction contract across entire go pipeline

### Fixed
- Pipeline gaps: GO_MODE, dual-phase clarity, Phase 3 PhaseDef
- Five quality gaps: domain skills, brainstorm→plan handoff, planSync, investigation, memory
- Comprehensive quality, consistency, and documentation sweep

## [0.1.31] - 2026-04-08

### Added
- Event-sourced pipeline-first architecture (`flowctl phase next/done`)
- Shared preamble startup sequence template

### Changed
- Consolidated plugin: removed deprecated skills, codex dupes, fixed cross-refs
- Removed all deprecated skill references from docs
- Removed libSQL/fastembed — pure file-based state
- Merged 4 Rust crates into 2 (core + cli)

### Fixed
- Stale references updated to show flow-code-run as primary entry point
- Test snapshot wildcards to avoid `.flow/` state dependency

## [0.1.0] - 2026-04-07

### Added
- Initial fork from gmickel/flow-next v0.26.1
- Rust `flowctl` binary: task DAG, state machine, file locking, cycle detection
- PID+TTL hybrid file locking with read/write/directory_add modes
- Schema migration infrastructure
- Atomic state writes with rename and error propagation
- Dual-tier worker timeout and deadlock detection
- Enhanced circuit breaker with regression, oscillation detection
- CI pipeline: shellcheck, cargo-audit, JSON validation, shell integration tests
- RTK hook caching for performance

### Changed
- Renamed flow-next → flow-code across all dirs, files, and content
- Flattened phase/step numbering to sequential integers
- Split monolithic smoke_test.sh into focused test files
