"""Stack detection, invariants, and guard commands."""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

from _flowctl.core.config import get_config, set_config
from _flowctl.core.io import error_exit, json_output
from _flowctl.core.paths import ensure_flow_exists, get_flow_dir, get_repo_root


INVARIANTS_FILE = "invariants.md"


def get_invariants_path() -> Path:
    """Get path to .flow/invariants.md."""
    return get_flow_dir() / INVARIANTS_FILE


def detect_stack() -> dict:
    """Auto-detect project tech stack from files in the repo."""
    repo = get_repo_root()
    stack: dict = {}

    # --- Backend detection ---
    backend: dict = {}

    # Python detection
    has_python = False
    pyproject = repo / "pyproject.toml"
    requirements = repo / "requirements.txt"
    setup_py = repo / "setup.py"
    manage_py = repo / "manage.py"

    py_content = ""
    if pyproject.exists():
        has_python = True
        py_content = pyproject.read_text(encoding="utf-8", errors="ignore")
    if requirements.exists():
        has_python = True
        py_content += "\n" + requirements.read_text(encoding="utf-8", errors="ignore")
    if setup_py.exists():
        has_python = True

    if has_python:
        backend["language"] = "python"

        # Framework detection
        if manage_py.exists() or "django" in py_content.lower():
            backend["framework"] = "django"
            conventions = []
            if "rest_framework" in py_content or "djangorestframework" in py_content:
                conventions.append("DRF")
            if "celery" in py_content.lower():
                conventions.append("Celery")
            if conventions:
                backend["conventions"] = ", ".join(conventions)
        elif "flask" in py_content.lower():
            backend["framework"] = "flask"
        elif "fastapi" in py_content.lower():
            backend["framework"] = "fastapi"

        # Test command
        if "pytest" in py_content:
            backend["test"] = "pytest"
        elif manage_py.exists():
            backend["test"] = "python manage.py test"

        # Lint
        if "ruff" in py_content:
            backend["lint"] = "ruff check"
        elif "flake8" in py_content:
            backend["lint"] = "flake8"

        # Type check
        if "mypy" in py_content:
            backend["typecheck"] = "mypy"
        elif "pyright" in py_content:
            backend["typecheck"] = "pyright"

    # Go detection
    go_mod = repo / "go.mod"
    if go_mod.exists() and not has_python:
        backend["language"] = "go"
        backend["test"] = "go test ./..."
        backend["lint"] = "golangci-lint run"
        go_content = go_mod.read_text(encoding="utf-8", errors="ignore")
        if "gin-gonic" in go_content:
            backend["framework"] = "gin"
        elif "labstack/echo" in go_content:
            backend["framework"] = "echo"
        elif "gofiber" in go_content:
            backend["framework"] = "fiber"

    if backend:
        stack["backend"] = backend

    # --- Frontend detection ---
    frontend: dict = {}

    # Find package.json (root or common frontend dirs)
    pkg_paths = [repo / "package.json"]
    for d in ["frontend", "client", "web", "app"]:
        pkg_paths.append(repo / d / "package.json")

    pkg_json: dict = {}
    pkg_path = None
    # Pick the package.json with the most dependencies (richest frontend config)
    best_dep_count = -1
    for p in pkg_paths:
        if p.exists():
            try:
                candidate = json.loads(p.read_text(encoding="utf-8"))
                dep_count = len(candidate.get("dependencies", {})) + len(
                    candidate.get("devDependencies", {})
                )
                if dep_count > best_dep_count:
                    best_dep_count = dep_count
                    pkg_json = candidate
                    pkg_path = p
            except (json.JSONDecodeError, Exception):
                pass

    if pkg_json:
        all_deps = {}
        all_deps.update(pkg_json.get("dependencies", {}))
        all_deps.update(pkg_json.get("devDependencies", {}))
        scripts = pkg_json.get("scripts", {})

        # Language
        if (repo / "tsconfig.json").exists() or any(
            (pkg_path.parent / f).exists()
            for f in ["tsconfig.json", "tsconfig.app.json"]
        ):
            frontend["language"] = "typescript"
        else:
            frontend["language"] = "javascript"

        # Framework
        if "react" in all_deps:
            frontend["framework"] = "react"
        elif "vue" in all_deps:
            frontend["framework"] = "vue"
        elif "svelte" in all_deps:
            frontend["framework"] = "svelte"
        elif "@angular/core" in all_deps:
            frontend["framework"] = "angular"

        # Meta-framework
        if "next" in all_deps:
            frontend["meta_framework"] = "nextjs"
        elif "nuxt" in all_deps:
            frontend["meta_framework"] = "nuxt"
        elif "@remix-run/react" in all_deps:
            frontend["meta_framework"] = "remix"

        # Package manager detection
        pkg_mgr = "npm"
        if (pkg_path.parent / "pnpm-lock.yaml").exists():
            pkg_mgr = "pnpm"
        elif (pkg_path.parent / "yarn.lock").exists():
            pkg_mgr = "yarn"
        elif (pkg_path.parent / "bun.lockb").exists() or (
            pkg_path.parent / "bun.lock"
        ).exists():
            pkg_mgr = "bun"

        # Prefix for subdirectory projects
        prefix = ""
        if pkg_path.parent != repo:
            rel = pkg_path.parent.relative_to(repo)
            prefix = f"cd {rel} && "

        # Commands from scripts
        if "test" in scripts:
            frontend["test"] = f"{prefix}{pkg_mgr} test"
        if "lint" in scripts:
            frontend["lint"] = f"{prefix}{pkg_mgr} run lint"
        if "typecheck" in scripts or "type-check" in scripts:
            tc_key = "typecheck" if "typecheck" in scripts else "type-check"
            frontend["typecheck"] = f"{prefix}{pkg_mgr} run {tc_key}"
        elif frontend.get("language") == "typescript":
            frontend["typecheck"] = (
                f"{prefix}{pkg_mgr} run tsc --noEmit"
                if pkg_mgr != "npx"
                else f"{prefix}npx tsc --noEmit"
            )

        # CSS framework (check deps + config files)
        has_tailwind = (
            "tailwindcss" in all_deps
            or (repo / "tailwind.config.js").exists()
            or (repo / "tailwind.config.ts").exists()
        )
        if has_tailwind:
            frontend.setdefault("conventions", "")
            frontend["conventions"] = "Tailwind" + (
                ", " + frontend["conventions"] if frontend["conventions"] else ""
            )

    if frontend:
        stack["frontend"] = frontend

    # --- Infra detection ---
    infra: dict = {}

    if (
        (repo / "Dockerfile").exists()
        or any(repo.glob("Dockerfile.*"))
        or any(repo.glob("**/Dockerfile"))
    ):
        infra["runtime"] = "docker"
    if (
        (repo / "docker-compose.yml").exists()
        or (repo / "docker-compose.yaml").exists()
        or (repo / "compose.yml").exists()
        or (repo / "compose.yaml").exists()
    ):
        infra["compose"] = True
    if (repo / "terraform").is_dir() or any(repo.glob("*.tf")):
        infra["iac"] = "terraform"
    elif (repo / "pulumi").is_dir():
        infra["iac"] = "pulumi"

    if infra:
        stack["infra"] = infra

    return stack


# --- Invariant commands ---


def cmd_invariants_show(args: argparse.Namespace) -> None:
    """Show architecture invariants."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    inv_path = get_invariants_path()
    if not inv_path.exists():
        if args.json:
            json_output(
                {
                    "invariants": None,
                    "message": "no invariants.md — create with 'flowctl invariants init'",
                }
            )
        else:
            print("No invariants.md. Create with: flowctl invariants init")
        return

    content = inv_path.read_text(encoding="utf-8")
    if args.json:
        json_output({"invariants": content, "path": str(inv_path)})
    else:
        print(content)


def cmd_invariants_init(args: argparse.Namespace) -> None:
    """Create .flow/invariants.md with template."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    inv_path = get_invariants_path()
    if inv_path.exists() and not getattr(args, "force", False):
        if args.json:
            json_output(
                {
                    "success": False,
                    "message": "invariants.md already exists. Use --force to overwrite.",
                }
            )
        else:
            print("invariants.md already exists. Use --force to overwrite.")
        return

    template = """# Architecture Invariants

Rules that must NEVER be violated, regardless of task or feature.
Workers check these during Phase 1. Planners check during Step 1.

<!-- Add your project's invariants below. Format:

## [Concept Name]
- **Rule:** [what must always hold]
- **Verify:** `shell command that exits 0 if invariant holds`
- **Fix:** [how to fix if violated]

-->
"""
    inv_path.write_text(template, encoding="utf-8")
    if args.json:
        json_output(
            {"success": True, "path": str(inv_path), "message": "invariants.md created"}
        )
    else:
        print(f"Created: {inv_path}")


def cmd_invariants_check(args: argparse.Namespace) -> None:
    """Run all verify commands from invariants.md."""
    if not ensure_flow_exists():
        error_exit(".flow/ does not exist.", use_json=args.json)

    inv_path = get_invariants_path()
    if not inv_path.exists():
        if args.json:
            json_output(
                {"success": True, "results": [], "message": "no invariants.md"}
            )
        else:
            print("No invariants.md — nothing to check.")
        return

    content = inv_path.read_text(encoding="utf-8")

    results = []
    all_passed = True

    # Strip HTML comments (template examples) before parsing
    content_clean = re.sub(r"<!--.*?-->", "", content, flags=re.DOTALL)

    current_name = None
    for line in content_clean.splitlines():
        if line.startswith("## "):
            current_name = line[3:].strip()
        elif "**Verify:**" in line and current_name:
            match = re.search(r"`([^`]+)`", line)
            if match:
                cmd = match.group(1)
                if not args.json:
                    print(f"\u25b8 [{current_name}] {cmd}")
                rc = subprocess.run(
                    cmd,
                    shell=True,
                    cwd=str(get_repo_root()),
                    capture_output=args.json,
                    text=True,
                ).returncode
                passed = rc == 0
                if not passed:
                    all_passed = False
                results.append(
                    {
                        "name": current_name,
                        "command": cmd,
                        "passed": passed,
                        "exit_code": rc,
                    }
                )
                if not args.json:
                    print(f"  {'\u2713' if passed else '\u2717'} exit {rc}")
                current_name = None

    if args.json:
        json_output({"success": all_passed, "results": results})
    else:
        total = len(results)
        passed_count = sum(1 for r in results if r["passed"])
        if total == 0:
            print("No verify commands found in invariants.md.")
        else:
            print(
                f"\n{passed_count}/{total} invariants hold"
                + ("" if all_passed else " — VIOLATED")
            )

    if not all_passed:
        sys.exit(1)


# --- Guard command ---


def cmd_guard(args: argparse.Namespace) -> None:
    """Run all guard commands (test/lint/typecheck) from stack config."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    stack = get_config("stack", {})
    if not stack:
        # Auto-detect on the fly
        stack = detect_stack()
        if stack:
            set_config("stack", stack)

    if not stack:
        if args.json:
            json_output(
                {
                    "success": True,
                    "results": [],
                    "message": "no stack detected, nothing to run",
                }
            )
        else:
            print("No stack detected. Nothing to run.")
        return

    layer = getattr(args, "layer", "all")
    cmd_types = ["test", "lint", "typecheck"]

    # Collect commands to run
    commands: list[tuple[str, str, str]] = []  # (layer, type, cmd)
    for layer_name, layer_conf in stack.items():
        if layer != "all" and layer_name != layer:
            continue
        if not isinstance(layer_conf, dict):
            continue
        for ct in cmd_types:
            if ct in layer_conf and layer_conf[ct]:
                commands.append((layer_name, ct, layer_conf[ct]))

    if not commands:
        if args.json:
            json_output(
                {
                    "success": True,
                    "results": [],
                    "message": "no guard commands configured",
                }
            )
        else:
            print("No guard commands found in stack config.")
        return

    results = []
    all_passed = True

    for layer_name, cmd_type, cmd in commands:
        if not args.json:
            print(f"\u25b8 [{layer_name}] {cmd_type}: {cmd}")

        rc = subprocess.run(
            cmd,
            shell=True,
            cwd=str(get_repo_root()),
            capture_output=args.json,
            text=True,
        ).returncode

        passed = rc == 0
        if not passed:
            all_passed = False

        results.append(
            {
                "layer": layer_name,
                "type": cmd_type,
                "command": cmd,
                "passed": passed,
                "exit_code": rc,
            }
        )

        if not args.json:
            status = "\u2713" if passed else "\u2717"
            print(f"  {status} exit {rc}")

    if args.json:
        json_output({"success": all_passed, "results": results})
    else:
        total = len(results)
        passed = sum(1 for r in results if r["passed"])
        print(
            f"\n{passed}/{total} guards passed"
            + ("" if all_passed else " — FAILED")
        )

    if not all_passed:
        sys.exit(1)


# --- Stack commands ---


def cmd_stack_detect(args: argparse.Namespace) -> None:
    """Auto-detect project stack and write to config."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    stack = detect_stack()

    if not stack:
        if args.json:
            json_output({"stack": {}, "message": "no stack detected"})
        else:
            print("No stack detected.")
        return

    if not args.dry_run:
        set_config("stack", stack)

    if args.json:
        json_output(
            {
                "stack": stack,
                "message": "stack auto-detected"
                + (" (dry-run)" if args.dry_run else ""),
            }
        )
    else:
        if args.dry_run:
            print("Detected stack (dry-run, not saved):")
        else:
            print("Stack detected and saved:")
        print(json.dumps(stack, indent=2, ensure_ascii=False))


def cmd_stack_set(args: argparse.Namespace) -> None:
    """Set stack config from JSON file or stdin."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    try:
        if args.file == "-":
            raw = sys.stdin.read()
        else:
            raw = Path(args.file).read_text(encoding="utf-8")
        stack_data = json.loads(raw)
    except (json.JSONDecodeError, Exception) as e:
        error_exit(f"Invalid JSON: {e}", use_json=args.json)

    if not isinstance(stack_data, dict):
        error_exit("Stack config must be a JSON object", use_json=args.json)

    set_config("stack", stack_data)
    result = get_config("stack")

    if args.json:
        json_output({"stack": result, "message": "stack config updated"})
    else:
        print(json.dumps(result, indent=2, ensure_ascii=False))


def cmd_stack_show(args: argparse.Namespace) -> None:
    """Show current stack config."""
    if not ensure_flow_exists():
        error_exit(
            ".flow/ does not exist. Run 'flowctl init' first.", use_json=args.json
        )

    stack = get_config("stack", {})

    if args.json:
        json_output({"stack": stack})
    else:
        if not stack:
            print(
                "No stack configured. Use 'flowctl stack set --file <path>' to set."
            )
        else:
            print(json.dumps(stack, indent=2, ensure_ascii=False))
