#!/usr/bin/env bash
# pre-commit.sh — Git pre-commit hook for flowctl Rust code quality.
#
# Runs cargo fmt and clippy checks on the flowctl workspace.
# Exit non-zero on any failure so the commit is aborted.
#
# Usage:
#   bash scripts/pre-commit.sh          # run manually
#   cp scripts/pre-commit.sh .git/hooks/pre-commit  # install as hook
#
# Note: This script is NOT auto-installed. Copy it manually if you want
# it to run on every commit.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/flowctl"

echo "==> Running cargo fmt --all -- --check"
cargo fmt --all -- --check
echo "    fmt: OK"

echo "==> Running cargo clippy --all -- -D warnings"
cargo clippy --all -- -D warnings
echo "    clippy: OK"

echo ""
echo "All pre-commit checks passed."
