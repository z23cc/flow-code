#!/bin/sh
# flowctl installer — downloads the latest release binary from GitHub.
# Usage: curl -fsSL https://raw.githubusercontent.com/z23cc/flow-code/main/flowctl/install.sh | sh
set -eu

REPO="z23cc/flow-code"
INSTALL_DIR="${FLOWCTL_INSTALL_DIR:-/usr/local/bin}"

# ── Platform detection ────────────────────────────────────────────────

detect_platform() {
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)  os_target="unknown-linux-gnu" ;;
        Darwin) os_target="apple-darwin" ;;
        *)      echo "Error: unsupported OS: $os" >&2; exit 1 ;;
    esac

    case "$arch" in
        x86_64|amd64)  arch_target="x86_64" ;;
        aarch64|arm64) arch_target="aarch64" ;;
        *)             echo "Error: unsupported architecture: $arch" >&2; exit 1 ;;
    esac

    echo "${arch_target}-${os_target}"
}

# ── Version resolution ────────────────────────────────────────────────

resolve_version() {
    if [ -n "${FLOWCTL_VERSION:-}" ]; then
        echo "$FLOWCTL_VERSION"
        return
    fi
    version="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' | head -1 | cut -d'"' -f4)"
    if [ -z "$version" ]; then
        echo "Error: could not determine latest version" >&2
        exit 1
    fi
    echo "$version"
}

# ── Download & verify ─────────────────────────────────────────────────

main() {
    platform="$(detect_platform)"
    version="$(resolve_version)"
    archive="flowctl-${version}-${platform}.tar.gz"
    base_url="https://github.com/${REPO}/releases/download/${version}"

    echo "Installing flowctl ${version} for ${platform}..."

    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    echo "Downloading ${archive}..."
    curl -fsSL "${base_url}/${archive}" -o "${tmpdir}/${archive}"
    curl -fsSL "${base_url}/${archive}.sha256" -o "${tmpdir}/${archive}.sha256"

    echo "Verifying checksum..."
    cd "$tmpdir"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum -c "${archive}.sha256"
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 -c "${archive}.sha256"
    else
        echo "Warning: no sha256sum or shasum found, skipping checksum verification" >&2
    fi

    echo "Extracting..."
    tar xzf "${archive}"

    echo "Installing to ${INSTALL_DIR}..."
    if [ -w "$INSTALL_DIR" ]; then
        mv flowctl "${INSTALL_DIR}/flowctl"
    else
        sudo mv flowctl "${INSTALL_DIR}/flowctl"
    fi
    chmod +x "${INSTALL_DIR}/flowctl"

    echo "Done! flowctl ${version} installed to ${INSTALL_DIR}/flowctl"
    echo "Run 'flowctl --help' to get started."
}

main
