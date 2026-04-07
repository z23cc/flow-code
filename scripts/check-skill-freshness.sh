#!/usr/bin/env bash
# CI check: ensures generated SKILL.md files match their .tmpl sources
set -euo pipefail
bash "$(dirname "$0")/gen-skill-docs.sh" --check
