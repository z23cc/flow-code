#!/usr/bin/env python3
"""
flowctl - CLI for managing .flow/ task tracking system.

Thin shim that delegates to the _flowctl package.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from _flowctl.cli import main  # noqa: E402
# Backward-compat re-exports for test scripts that do `from flowctl import ...`
from _flowctl.core.git import gather_context_hints  # noqa: E402,F401
from _flowctl.core.git import extract_symbols_from_file  # noqa: E402,F401
from _flowctl.commands.review import build_review_prompt  # noqa: E402,F401

if __name__ == "__main__":
    main()
