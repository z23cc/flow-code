#!/bin/sh
# ensure-flowctl.sh — idempotent binary provisioner for the flow-code plugin.
#
# Called from SessionStart hook. Ensures $PLUGIN_ROOT/bin/flowctl exists and
# matches the version pinned in .claude-plugin/flowctl-version.
#
# Strategy:
#   1. Fast path: binary present and version matches → exit 0 in <100ms
#   2. Download from GitHub Releases (with checksum verification)
#   3. Fallback: cargo build from source (if toolchain + source available)
#   4. On any failure: warn but exit 0 (never block session start)
#
# Env overrides:
#   FLOWCTL_SKIP_ENSURE=1   — skip entirely (for CI/testing)
#   FLOWCTL_FORCE_BUILD=1   — skip download, build from source directly

set -eu

[ "${FLOWCTL_SKIP_ENSURE:-0}" = "1" ] && exit 0

PLUGIN_ROOT="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT:-}}"
[ -n "$PLUGIN_ROOT" ] || exit 0
[ -d "$PLUGIN_ROOT" ] || exit 0

PINNED_FILE="$PLUGIN_ROOT/.claude-plugin/flowctl-version"
[ -f "$PINNED_FILE" ] || exit 0

PINNED="$(tr -d ' \t\n\r' < "$PINNED_FILE")"
[ -n "$PINNED" ] || exit 0

BIN="$PLUGIN_ROOT/bin/flowctl"

# Fast path: binary exists and version matches
if [ -x "$BIN" ] && [ "${FLOWCTL_FORCE_BUILD:-0}" != "1" ]; then
    CURRENT="$("$BIN" --version 2>/dev/null | awk 'NR==1{print $2}' || echo "")"
    if [ -n "$CURRENT" ]; then
        # PINNED may be "v0.1.26" or "0.1.26"; normalize
        PINNED_NUM="${PINNED#v}"
        if [ "$CURRENT" = "$PINNED_NUM" ]; then
            exit 0
        fi
    fi
fi

# Need to provision
log() { printf '[flow-code] %s\n' "$1" >&2; }

log "provisioning flowctl $PINNED..."

mkdir -p "$PLUGIN_ROOT/bin"

# ── Try GitHub Release download ─────────────────────────────────────
try_download() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"
    case "$OS" in
        Linux)  OS_T="unknown-linux-gnu" ;;
        Darwin) OS_T="apple-darwin" ;;
        MINGW*|MSYS*|CYGWIN*)
            log "Windows is not supported natively — run under WSL"
            return 1 ;;
        *) log "unsupported OS: $OS"; return 1 ;;
    esac
    case "$ARCH" in
        x86_64|amd64)  ARCH_T="x86_64" ;;
        aarch64|arm64) ARCH_T="aarch64" ;;
        *) log "unsupported arch: $ARCH"; return 1 ;;
    esac
    # Intel Macs are unsupported (ort-sys has no prebuilt binary)
    if [ "$OS_T" = "apple-darwin" ] && [ "$ARCH_T" = "x86_64" ]; then
        log "Intel Mac unsupported by prebuilt binaries — building from source"
        return 1
    fi
    PLATFORM="${ARCH_T}-${OS_T}"
    EXT="tar.gz"
    BIN_NAME="flowctl"

    REPO="z23cc/flow-code"
    ARCHIVE="flowctl-${PINNED}-${PLATFORM}.${EXT}"
    URL="https://github.com/${REPO}/releases/download/${PINNED}/${ARCHIVE}"

    command -v curl >/dev/null 2>&1 || { log "curl not found"; return 1; }

    TMP="$(mktemp -d)"
    # shellcheck disable=SC2064
    trap "rm -rf '$TMP'" EXIT INT TERM

    log "downloading $ARCHIVE"
    if ! curl -fsSL --connect-timeout 10 --max-time 120 "$URL" -o "$TMP/$ARCHIVE"; then
        log "download failed: $URL"
        return 1
    fi

    # Verify checksum if available (best-effort)
    if curl -fsSL --connect-timeout 5 --max-time 15 "${URL}.sha256" -o "$TMP/$ARCHIVE.sha256" 2>/dev/null; then
        (cd "$TMP" && {
            if command -v sha256sum >/dev/null 2>&1; then
                sha256sum -c "$ARCHIVE.sha256" >/dev/null 2>&1
            elif command -v shasum >/dev/null 2>&1; then
                shasum -a 256 -c "$ARCHIVE.sha256" >/dev/null 2>&1
            else
                true
            fi
        }) || { log "checksum verification failed"; return 1; }
    fi

    # Extract
    case "$EXT" in
        tar.gz)
            tar xzf "$TMP/$ARCHIVE" -C "$TMP" || { log "extract failed"; return 1; }
            ;;
        zip)
            (cd "$TMP" && unzip -q "$ARCHIVE") || { log "extract failed"; return 1; }
            ;;
    esac

    [ -f "$TMP/$BIN_NAME" ] || { log "archive missing $BIN_NAME"; return 1; }

    mv "$TMP/$BIN_NAME" "$BIN"
    chmod +x "$BIN"
    log "flowctl $PINNED installed from GitHub Release ✓"
    return 0
}

# ── Fallback: build from source ─────────────────────────────────────
try_build() {
    SRC="$PLUGIN_ROOT/flowctl"
    [ -d "$SRC/crates" ] || { log "source not available at $SRC"; return 1; }
    command -v cargo >/dev/null 2>&1 || { log "cargo not found"; return 1; }

    log "building from source (may take ~1-2min, downloads ~130MB on first run)..."
    (cd "$SRC" && cargo build --release -p flowctl-cli >&2) || { log "cargo build failed"; return 1; }

    BUILT="$SRC/target/release/flowctl"
    [ -x "$BUILT" ] || { log "built binary not found at $BUILT"; return 1; }

    cp "$BUILT" "$BIN"
    chmod +x "$BIN"
    log "flowctl built from source ✓"
    return 0
}

# ── Execute strategy ────────────────────────────────────────────────
if [ "${FLOWCTL_FORCE_BUILD:-0}" = "1" ]; then
    try_build || log "build failed — flow-code features disabled"
else
    try_download || try_build || {
        log "flowctl unavailable — flow-code features will be disabled"
        log "retry manually: $PLUGIN_ROOT/scripts/hooks/ensure-flowctl.sh"
    }
fi

exit 0
