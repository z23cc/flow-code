#!/usr/bin/env python3
"""
flowctl - CLI for managing .flow/ task tracking system.

Thin shim that delegates to the flowctl package.
"""

import os
import sys

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
