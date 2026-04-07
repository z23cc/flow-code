#!/usr/bin/env bash
# setup-hooks.sh — Install git hooks by symlinking from scripts/ to .git/hooks/
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOOKS_DIR="$ROOT/.git/hooks"

if [[ ! -d "$ROOT/.git" ]]; then
  echo "ERROR: not a git repository (no .git/ at $ROOT)" >&2
  exit 1
fi

mkdir -p "$HOOKS_DIR"

# Symlink pre-commit hook
SOURCE="$ROOT/scripts/pre-commit.sh"
TARGET="$HOOKS_DIR/pre-commit"

if [[ ! -f "$SOURCE" ]]; then
  echo "ERROR: $SOURCE not found" >&2
  exit 1
fi

if [[ -L "$TARGET" ]]; then
  echo "pre-commit hook already symlinked, updating..."
  rm "$TARGET"
elif [[ -f "$TARGET" ]]; then
  echo "WARNING: existing pre-commit hook found, backing up to pre-commit.bak"
  mv "$TARGET" "$TARGET.bak"
fi

ln -s "$SOURCE" "$TARGET"
chmod +x "$TARGET"
echo "Installed pre-commit hook: $TARGET -> $SOURCE"
