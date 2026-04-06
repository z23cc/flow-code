#!/bin/bash
# Install flow-code into Codex CLI (~/.codex) using pre-built files.
#
# Usage: ./scripts/install-codex.sh
#
# What gets installed (from pre-built codex/ directory):
#   - Skills:    codex/skills/             → ~/.codex/skills/
#   - Agents:    codex/agents/*.toml       → ~/.codex/agents/
#   - Hooks:     codex/hooks.json          → ~/.codex/hooks.json
#   - Prompts:   commands/flow-code/*.md   → ~/.codex/prompts/
#   - CLI tools: bin/flowctl               → ~/.flow/bin/
#   - Manifest:  .codex-plugin/plugin.json → ~/.codex/plugin.json
#   - Config:    agent entries             → ~/.codex/config.toml (merged)
#
# Environment overrides:
#   CODEX_MODEL_INTELLIGENT  — model for opus/smart scouts (default: gpt-5.4)
#   CODEX_MODEL_FAST         — model for fast scouts (default: gpt-5.4-mini)
#   CODEX_MAX_THREADS        — max concurrent agent threads (default: 12)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
CODEX_DIR="$HOME/.codex"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# ---------------------------------------------------------------------------
# Validate
# ---------------------------------------------------------------------------

CODEX_SRC="$REPO_ROOT/codex"

if [ ! -d "$CODEX_SRC/skills" ] || [ ! -d "$CODEX_SRC/agents" ]; then
    echo -e "${YELLOW}Pre-built codex/ directory not found. Generating...${NC}"
    # Find flowctl
    FLOWCTL=""
    if [ -x "$REPO_ROOT/bin/flowctl" ]; then
        FLOWCTL="$REPO_ROOT/bin/flowctl"
    elif [ -x "$REPO_ROOT/flowctl/target/release/flowctl" ]; then
        FLOWCTL="$REPO_ROOT/flowctl/target/release/flowctl"
    fi

    if [ -n "$FLOWCTL" ]; then
        mkdir -p "$CODEX_SRC/agents" "$CODEX_SRC/skills"
        SYNC_ARGS=(codex sync --agents-dir "$REPO_ROOT/agents" --output-dir "$CODEX_SRC")
        [ -f "$REPO_ROOT/hooks/hooks.json" ] && SYNC_ARGS+=(--hooks "$REPO_ROOT/hooks/hooks.json")
        "$FLOWCTL" "${SYNC_ARGS[@]}" 2>/dev/null || true

        # Rename claude-md-scout → agents-md-scout
        if [ -f "$CODEX_SRC/agents/claude-md-scout.toml" ]; then
            mv "$CODEX_SRC/agents/claude-md-scout.toml" "$CODEX_SRC/agents/agents-md-scout.toml"
            sed -i.bak 's/name = "claude-md-scout"/name = "agents-md-scout"/' "$CODEX_SRC/agents/agents-md-scout.toml"
            rm -f "$CODEX_SRC/agents/agents-md-scout.toml.bak"
        fi

        # Generate skills
        for skill_dir in "$REPO_ROOT/skills/"flow-code*/; do
            [ -d "$skill_dir" ] || continue
            skill_name="$(basename "$skill_dir")"
            [ -f "$skill_dir/SKILL.md" ] || continue
            dst="$CODEX_SRC/skills/$skill_name"
            mkdir -p "$dst"
            for f in "$skill_dir"/*.md; do
                [ -f "$f" ] || continue
                sed -e 's/CLAUDE\.md/AGENTS.md/g' \
                    -e 's/claude-md-scout/agents-md-scout/g' \
                    -e 's|FLOWCTL="\$HOME/\.flow/bin/flowctl"|FLOWCTL="$HOME/.flow/bin/flowctl"\n[ -x "$FLOWCTL" ] || FLOWCTL="$HOME/.codex/scripts/flowctl"|g' \
                    -e 's|PLUGIN_ROOT="\${DROID_PLUGIN_ROOT:-\${CLAUDE_PLUGIN_ROOT}}"|PLUGIN_ROOT="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT:-$HOME/.codex}}"|g' \
                    -e 's|PLUGIN_JSON="\${DROID_PLUGIN_ROOT:-\${CLAUDE_PLUGIN_ROOT}}/\.claude-plugin/plugin\.json"|PLUGIN_JSON="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT:-$HOME/.codex}}/.codex-plugin/plugin.json"|g' \
                    -e 's|\${DROID_PLUGIN_ROOT:-\${CLAUDE_PLUGIN_ROOT}}/skills/|${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT:-$HOME/.codex}}/skills/|g' \
                    "$f" > "$dst/$(basename "$f")"
            done
            [ -d "$skill_dir/agents" ] && cp -r "$skill_dir/agents" "$dst/agents"
        done
        echo -e "${GREEN}✓${NC} Generated codex/ directory"
    else
        echo -e "${RED}Error: No flowctl binary and no pre-built codex/ directory.${NC}"
        echo "Build first: cd flowctl && cargo build --release && cp target/release/flowctl ../bin/"
        exit 1
    fi
fi

if [ ! -d "$CODEX_DIR" ]; then
    echo -e "${RED}Error: ~/.codex not found. Is Codex CLI installed?${NC}"
    exit 1
fi

echo "Installing flow-code to Codex CLI..."
echo

# Create target directories
mkdir -p "$CODEX_DIR/skills" "$CODEX_DIR/agents" "$CODEX_DIR/scripts" "$CODEX_DIR/prompts"

# ====================
# Skills
# ====================
SKILL_COUNT=0
for skill_dir in "$CODEX_SRC/skills/"*/; do
    [ -d "$skill_dir" ] || continue
    rm -rf "$CODEX_DIR/skills/$(basename "$skill_dir")"
    cp -r "${skill_dir%/}" "$CODEX_DIR/skills/"
    SKILL_COUNT=$((SKILL_COUNT + 1))
done
echo -e "${GREEN}✓${NC} $SKILL_COUNT skills"

# ====================
# Agents (.toml)
# ====================
# Clean old auto-generated agents
grep -rl "Auto-generated.*flowctl codex sync\|Auto-generated.*do not edit" "$CODEX_DIR/agents/"*.toml 2>/dev/null | xargs rm -f 2>/dev/null || true

AGENT_COUNT=0
for toml_file in "$CODEX_SRC/agents/"*.toml; do
    [ -f "$toml_file" ] || continue
    cp "$toml_file" "$CODEX_DIR/agents/"
    AGENT_COUNT=$((AGENT_COUNT + 1))
done
echo -e "${GREEN}✓${NC} $AGENT_COUNT agents"

# ====================
# Hooks
# ====================
if [ -f "$CODEX_SRC/hooks.json" ]; then
    cp "$CODEX_SRC/hooks.json" "$CODEX_DIR/hooks.json"
    echo -e "${GREEN}✓${NC} hooks.json"
fi

# ====================
# CLI tools → ~/.flow/bin/
# ====================
FLOW_BIN="$HOME/.flow/bin"
mkdir -p "$FLOW_BIN"
HAS_FLOWCTL=false
if [ -x "$REPO_ROOT/bin/flowctl" ]; then
    cp "$REPO_ROOT/bin/flowctl" "$FLOW_BIN/"
    chmod +x "$FLOW_BIN/flowctl"
    HAS_FLOWCTL=true
fi
if [ -f "$REPO_ROOT/bin/flowctl.py" ]; then
    cp "$REPO_ROOT/bin/flowctl.py" "$FLOW_BIN/"
fi
[ "$HAS_FLOWCTL" = true ] && echo -e "${GREEN}✓${NC} flowctl → ~/.flow/bin/"

# Clean up old locations
rm -f "$CODEX_DIR/scripts/flowctl" "$CODEX_DIR/scripts/flowctl.py" 2>/dev/null
rm -f "$CODEX_DIR/bin/flowctl" "$CODEX_DIR/bin/flowctl.py" 2>/dev/null

# ====================
# Plugin manifest
# ====================
if [ -f "$REPO_ROOT/.codex-plugin/plugin.json" ]; then
    cp "$REPO_ROOT/.codex-plugin/plugin.json" "$CODEX_DIR/plugin.json"
    echo -e "${GREEN}✓${NC} plugin.json"
fi

# ====================
# Prompts (commands → prompts)
# ====================
PROMPT_COUNT=0
for cmd in "$REPO_ROOT/commands/flow-code/"*.md; do
    [ -f "$cmd" ] || continue
    cp "$cmd" "$CODEX_DIR/prompts/"
    PROMPT_COUNT=$((PROMPT_COUNT + 1))
done
echo -e "${GREEN}✓${NC} $PROMPT_COUNT prompts"

# ====================
# Config.toml (merge agent entries + features)
# ====================
echo -e "${BLUE}Merging config.toml...${NC}"
CONFIG="$CODEX_DIR/config.toml"

# Ensure multi_agent = true
if [ -f "$CONFIG" ]; then
    if ! grep -q "^multi_agent" "$CONFIG" 2>/dev/null; then
        tmp="/tmp/codex-config-prepend.toml"
        { echo "# Enable custom multi-agent roles (Codex 0.102.0+)"
          echo "multi_agent = true"
          echo ""
          cat "$CONFIG"
        } > "$tmp"
        mv "$tmp" "$CONFIG"
    fi
else
    { echo "# Enable custom multi-agent roles (Codex 0.102.0+)"
      echo "multi_agent = true"
      echo ""
    } > "$CONFIG"
fi

# Clean old flow-code entries
if grep -q "flow-code multi-agent roles" "$CONFIG" 2>/dev/null; then
    sed -i.bak '/# --- flow-code multi-agent roles/,/# --- end flow-code roles ---/d' "$CONFIG"
    rm -f "${CONFIG}.bak"
fi

if grep -q "# --- flow-code features" "$CONFIG" 2>/dev/null; then
    sed -i.bak '/# --- flow-code features/,/# --- end flow-code features ---/d' "$CONFIG"
    rm -f "${CONFIG}.bak"
fi

# Merge codex_hooks into existing [features] section (avoid duplicate keys)
if grep -q "^\[features\]" "$CONFIG" 2>/dev/null; then
    if ! grep -q "codex_hooks" "$CONFIG" 2>/dev/null; then
        sed -i.bak '/^\[features\]/a\
codex_hooks = true' "$CONFIG"
        rm -f "${CONFIG}.bak"
    fi
else
    echo -e "\n[features]\ncodex_hooks = true" >> "$CONFIG"
fi
echo -e "  ${GREEN}✓${NC} [features] codex_hooks = true"

# Generate agent entries
CODEX_MAX_THREADS="${CODEX_MAX_THREADS:-12}"
{
    echo ""
    echo "# --- flow-code multi-agent roles (auto-generated) ---"
    echo "# Re-run install-codex.sh to regenerate"
    echo ""

    if ! grep -q "^\[agents\]" "$CONFIG" 2>/dev/null; then
        echo "[agents]"
    fi
    echo "max_threads = $CODEX_MAX_THREADS"
    echo ""

    for toml_file in "$CODEX_SRC/agents/"*.toml; do
        [ -f "$toml_file" ] || continue
        name=$(basename "$toml_file" .toml)
        role_key="${name//-/_}"
        desc=$(grep '^description = ' "$toml_file" | head -1 | sed 's/^description = "//;s/"$//')
        echo "[agents.$role_key]"
        echo "description = \"$desc\""
        echo "config_file = \"agents/$name.toml\""
        echo ""
    done

    echo "# --- end flow-code roles ---"
} >> "$CONFIG"

echo -e "  ${GREEN}✓${NC} config.toml ($AGENT_COUNT agent entries, max_threads=$CODEX_MAX_THREADS)"

# ====================
# Summary
# ====================
echo
echo -e "${GREEN}Done!${NC} flow-code installed to ~/.codex"
echo "  $SKILL_COUNT skills, $AGENT_COUNT agents, $PROMPT_COUNT prompts"
[ "$HAS_FLOWCTL" = true ] && echo "  flowctl: ~/.flow/bin/flowctl"
echo "  hooks: ~/.codex/hooks.json"
echo "  config: ~/.codex/config.toml (merged, max_threads=$CODEX_MAX_THREADS)"
echo
echo -e "${YELLOW}Usage in Codex:${NC}"
echo "  \$flow-code-plan \"add user auth\""
echo "  \$flow-code-work fn-1"
echo "  \$flow-code-impl-review"
