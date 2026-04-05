#!/usr/bin/env bash
# bump-version.sh — single-command version bump across all 4 canonical files.
#
# Usage:
#   scripts/bump-version.sh 0.1.27    # bump to 0.1.27
#   scripts/bump-version.sh --check   # verify all files agree
#   scripts/bump-version.sh           # print current version
#
# Touches:
#   1. flowctl/crates/flowctl-cli/Cargo.toml       (bare semver)
#   2. .claude-plugin/flowctl-version              (v-prefixed)
#   3. .claude-plugin/plugin.json                  (bare semver)
#   4. .claude-plugin/marketplace.json             (bare semver × 3)
#
# After running: commit + tag v<version> + push to trigger GitHub Release.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

CARGO="flowctl/crates/flowctl-cli/Cargo.toml"
PIN=".claude-plugin/flowctl-version"
PLUGIN=".claude-plugin/plugin.json"
MARKET=".claude-plugin/marketplace.json"

# ── Read current versions ───────────────────────────────────────────
current_cargo()   { awk -F'"' '/^version = /{print $2; exit}' "$CARGO"; }
current_pin()     { tr -d ' \t\n\r' < "$PIN" | sed 's/^v//'; }
current_plugin()  { python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["version"])' "$PLUGIN"; }
current_market()  { python3 -c 'import json,sys;d=json.load(open(sys.argv[1]));print(d["version"])' "$MARKET"; }

print_current() {
    printf "%-50s %s\n" "$CARGO"   "$(current_cargo)"
    printf "%-50s %s\n" "$PIN"     "v$(current_pin)"
    printf "%-50s %s\n" "$PLUGIN"  "$(current_plugin)"
    printf "%-50s %s\n" "$MARKET"  "$(current_market)"
}

# ── Check mode ──────────────────────────────────────────────────────
if [ "${1:-}" = "--check" ]; then
    c="$(current_cargo)"; p="$(current_pin)"; pl="$(current_plugin)"; m="$(current_market)"
    if [ "$c" = "$p" ] && [ "$p" = "$pl" ] && [ "$pl" = "$m" ]; then
        echo "✓ all files agree on v$c"
        exit 0
    fi
    echo "✗ version drift detected:" >&2
    print_current >&2
    exit 1
fi

# ── No args: just show ──────────────────────────────────────────────
if [ $# -eq 0 ]; then
    print_current
    exit 0
fi

# ── Bump mode ───────────────────────────────────────────────────────
NEW="$1"
NEW="${NEW#v}"  # strip leading v if present

# Validate semver-ish (X.Y.Z, optionally with prerelease)
if ! printf '%s' "$NEW" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$'; then
    echo "error: invalid version '$NEW' (expected X.Y.Z)" >&2
    exit 1
fi

echo "bumping → v$NEW"

# 1. Cargo.toml — target the flowctl-cli package line only
#    (line starts with `version = "` near the top)
python3 - "$CARGO" "$NEW" <<'PY'
import re, sys
path, new = sys.argv[1], sys.argv[2]
src = open(path).read()
# Match first `version = "..."` line (package version, not deps)
src = re.sub(r'(?m)^version\s*=\s*"[^"]+"', f'version = "{new}"', src, count=1)
open(path, 'w').write(src)
PY

# 2. flowctl-version — v-prefixed
printf 'v%s\n' "$NEW" > "$PIN"

# 3. plugin.json — update "version" field at top level
python3 - "$PLUGIN" "$NEW" <<'PY'
import json, sys
path, new = sys.argv[1], sys.argv[2]
with open(path) as f:
    data = json.load(f)
data["version"] = new
with open(path, 'w') as f:
    json.dump(data, f, indent=2, ensure_ascii=False)
    f.write('\n')
PY

# 4. marketplace.json — update all 3 version fields
python3 - "$MARKET" "$NEW" <<'PY'
import json, sys
path, new = sys.argv[1], sys.argv[2]
with open(path) as f:
    data = json.load(f)
data["version"] = new
data["metadata"]["version"] = new
for plugin in data.get("plugins", []):
    plugin["version"] = new
with open(path, 'w') as f:
    json.dump(data, f, indent=2, ensure_ascii=False)
    f.write('\n')
PY

echo ""
echo "updated files:"
print_current
echo ""
echo "next steps:"
echo "  cd flowctl && cargo build --release -p flowctl-cli  # updates Cargo.lock"
echo "  git add -u && git commit -m \"chore: bump to v$NEW\""
echo "  git tag v$NEW && git push && git push --tags"
