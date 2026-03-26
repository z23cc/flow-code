#!/usr/bin/env python3
"""Detect project type and output the best program.md template name."""
import json
import sys
from pathlib import Path

def detect(root: Path) -> str:
    """Return one of: django, nextjs, react, generic."""
    # Django: manage.py or settings.py
    if (root / "manage.py").exists():
        return "django"
    for p in root.rglob("settings.py"):
        if "django" in p.read_text(errors="ignore").lower():
            return "django"
            break

    # Next.js: next.config.* or package.json with "next"
    for name in ("next.config.js", "next.config.mjs", "next.config.ts"):
        if (root / name).exists():
            return "nextjs"

    pkg = root / "package.json"
    if pkg.exists():
        try:
            data = json.loads(pkg.read_text())
            deps = {**data.get("dependencies", {}), **data.get("devDependencies", {})}
            if "next" in deps:
                return "nextjs"
            if "react" in deps:
                return "react"
        except Exception:
            pass

    return "generic"

if __name__ == "__main__":
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path.cwd()
    result = detect(root)
    if "--json" in sys.argv:
        print(json.dumps({"project_type": result}))
    else:
        print(result)
