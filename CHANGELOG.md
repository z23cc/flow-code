# Changelog

All notable changes to Flow-Code are documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/). Versions use [Semantic Versioning](https://semver.org/).

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
