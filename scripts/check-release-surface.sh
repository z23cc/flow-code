#!/usr/bin/env bash
# CI/release preflight: ensure install/release metadata is internally consistent.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

bash scripts/bump-version.sh --check

python3 - <<'PY'
import json
import pathlib
import re
import sys

root = pathlib.Path('.')
errors: list[str] = []

expected_version = json.loads((root / '.claude-plugin/plugin.json').read_text(encoding='utf-8'))['version']

for readme in ('README.md', 'README_CN.md'):
    text = (root / readme).read_text(encoding='utf-8')
    match = re.search(r'badge/Version-([0-9]+\.[0-9]+\.[0-9]+)-green', text)
    if not match:
        errors.append(f'{readme}: missing Version badge')
    elif match.group(1) != expected_version:
        errors.append(
            f'{readme}: badge version {match.group(1)} != plugin version {expected_version}'
        )

marketplace_path = root / '.agents/plugins/marketplace.json'
marketplace = json.loads(marketplace_path.read_text(encoding='utf-8'))
marketplace_dir = marketplace_path.parent
for plugin in marketplace.get('plugins', []):
    source = plugin.get('source', {})
    if source.get('source') != 'local':
        continue
    local_path = source.get('path')
    if not local_path:
        errors.append(f".agents marketplace: plugin {plugin.get('name', '<unknown>')} missing local path")
        continue
    target = (marketplace_dir / local_path).resolve()
    if not target.exists():
        errors.append(
            f".agents marketplace: plugin {plugin.get('name', '<unknown>')} path does not exist: {local_path}"
        )

flowctl_readme = (root / 'flowctl/README.md').read_text(encoding='utf-8')
if 'github.com/anthropics/flow-code' in flowctl_readme:
    errors.append('flowctl/README.md still references anthropics/flow-code')
if 'https://raw.githubusercontent.com/z23cc/flow-code/main/flowctl/install.sh' not in flowctl_readme:
    errors.append('flowctl/README.md missing canonical install.sh URL')

if errors:
    print('✗ release surface drift detected:')
    for err in errors:
        print(f'  - {err}')
    sys.exit(1)

print(f'✓ release surface checks passed for v{expected_version}')
PY
