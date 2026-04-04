#!/usr/bin/env python3
"""
flowctl - CLI for managing .flow/ task tracking system.

Thin shim that delegates to the flowctl package (Python) or the Rust
binary when FLOWCTL_RUST=1 is set.
"""

import os
import shutil
import sys


def _dispatch_rust():
    """Find and exec the Rust flowctl binary, replacing this process."""
    # 1. Check PATH
    rust_bin = shutil.which("flowctl")

    # 2. Check plugin dir bin/flowctl
    if not rust_bin:
        plugin_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        candidate = os.path.join(plugin_dir, "bin", "flowctl")
        if os.path.isfile(candidate) and os.access(candidate, os.X_OK):
            rust_bin = candidate

    if not rust_bin:
        print("Error: Rust flowctl binary not found.", file=sys.stderr)
        print("FLOWCTL_RUST=1 is set but no binary is available.", file=sys.stderr)
        print("", file=sys.stderr)
        print("Install options:", file=sys.stderr)
        print("  1. Build from source:  cd flowctl && cargo build --release", file=sys.stderr)
        print("     Then copy target/release/flowctl to a directory in your PATH", file=sys.stderr)
        print("  2. Place the binary at: %s" % os.path.join(
            os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "bin", "flowctl"
        ), file=sys.stderr)
        sys.exit(1)

    os.execvp(rust_bin, [rust_bin] + sys.argv[1:])


if os.environ.get("FLOWCTL_RUST"):
    _dispatch_rust()

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

try:
    from flowctl.cli import main  # noqa: E402
except ImportError:
    print("Error: flowctl package not found alongside flowctl.py", file=sys.stderr)
    print("Expected at: %s" % os.path.join(os.path.dirname(os.path.abspath(__file__)), "flowctl"), file=sys.stderr)
    print("Ensure both flowctl.py and flowctl/ were copied together.", file=sys.stderr)
    sys.exit(1)

if __name__ == "__main__":
    main()
