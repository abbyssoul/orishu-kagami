#!/usr/bin/env python3
"""Check the public-documentation skeleton and relative Markdown links."""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parent.parent
REQUIRED = (
    "README.md",
    "CONTRIBUTING.md",
    "CODE_OF_CONDUCT.md",
    "SECURITY.md",
    "LICENSE",
    "CONTEXT.md",
    "docs/README.md",
    "docs/architecture.md",
    "docs/development.md",
)
LINK = re.compile(r"!?(?:\[[^]]*\])\(([^)]+)\)")


def markdown_files() -> list[Path]:
    files: list[Path] = []
    for path in ROOT.rglob("*.md"):
        if any(part in {".git", "target"} for part in path.parts):
            continue
        files.append(path)
    return sorted(files)


def local_target(raw: str) -> str | None:
    target = raw.strip()
    if target.startswith("<") and ">" in target:
        target = target[1 : target.index(">")]
    else:
        target = target.split(maxsplit=1)[0]

    if not target or target.startswith("#"):
        return None
    if "://" in target or target.startswith(("mailto:", "data:")):
        return None

    target = target.split("#", 1)[0].split("?", 1)[0]
    return unquote(target) or None


def main() -> int:
    errors: list[str] = []

    for relative in REQUIRED:
        if not (ROOT / relative).exists():
            errors.append(f"missing required public file: {relative}")

    for source in markdown_files():
        text = source.read_text(encoding="utf-8")
        for match in LINK.finditer(text):
            target = local_target(match.group(1))
            if target is None:
                continue
            destination = ROOT / target.lstrip("/") if target.startswith("/") else source.parent / target
            if not destination.resolve().exists():
                errors.append(
                    f"{source.relative_to(ROOT)}: broken local link {match.group(1)!r}"
                )

    if errors:
        print("documentation check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(f"documentation check passed ({len(markdown_files())} Markdown files)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
