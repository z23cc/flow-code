#!/usr/bin/env bash
# setup-hooks.sh — Symlink pre-commit hook into .git/hooks/
#
# Idempotent: safe to run multiple times.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

HOOK_SRC="$ROOT/scripts/pre-commit.sh"
HOOK_DST="$ROOT/.git/hooks/pre-commit"

if [ -L "$HOOK_DST" ] && [ "$(readlink "$HOOK_DST")" = "$HOOK_SRC" ]; then
  echo "pre-commit hook already installed."
  exit 0
fi

mkdir -p "$ROOT/.git/hooks"
ln -sf "$HOOK_SRC" "$HOOK_DST"
echo "pre-commit hook installed: $HOOK_DST -> $HOOK_SRC"
