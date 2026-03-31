"""Allow running as: python3 -m _flowctl"""

if __package__ is None:
    import sys
    import os
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from _flowctl.cli import main

main()
