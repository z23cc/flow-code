#!/usr/bin/env bash
set -euo pipefail

# Thin wrapper — delegates to split test files in scripts/tests/
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
exec bash "$SCRIPT_DIR/tests/run_all.sh" "$@"
