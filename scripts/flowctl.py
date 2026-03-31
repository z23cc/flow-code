#!/usr/bin/env python3
"""
flowctl - CLI for managing .flow/ task tracking system.

Thin shim that delegates to the _flowctl package.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

try:
    from _flowctl.cli import main  # noqa: E402
except ImportError:
    print("Error: _flowctl package not found alongside flowctl.py", file=sys.stderr)
    print("Expected at: %s" % os.path.join(os.path.dirname(os.path.abspath(__file__)), "_flowctl"), file=sys.stderr)
    print("Ensure both flowctl.py and _flowctl/ were copied together.", file=sys.stderr)
    sys.exit(1)
# Backward-compat re-exports for test scripts that do `from flowctl import ...`
from _flowctl.core.git import gather_context_hints  # noqa: E402,F401
from _flowctl.core.git import extract_symbols_from_file  # noqa: E402,F401
from _flowctl.commands.review import build_review_prompt  # noqa: E402,F401

if __name__ == "__main__":
    main()
