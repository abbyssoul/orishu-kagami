#!/usr/bin/env python3
"""Validate a stable release version against the workspace version."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import tomllib


STABLE = re.compile(r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$")
APPLICATION_MANIFESTS = (
    Path("apps/orishu-worker/Cargo.toml"),
    Path("apps/orishu-ctl/Cargo.toml"),
    Path("apps/orishu-monitor/Cargo.toml"),
    Path("apps/kagami/Cargo.toml"),
)


def normalize(value: str) -> str:
    return value[1:] if value.startswith("v") else value


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("version")
    parser.add_argument("--manifest", type=Path, default=Path("Cargo.toml"))
    args = parser.parse_args()

    requested = normalize(args.version)
    if not STABLE.fullmatch(requested):
        raise SystemExit(f"release: {args.version!r} is not stable MAJOR.MINOR.PATCH SemVer")
    manifest = tomllib.loads(args.manifest.read_text(encoding="utf-8"))
    actual = manifest["workspace"]["package"]["version"]
    if requested != actual:
        raise SystemExit(
            f"release: requested version {requested} does not match workspace version {actual}"
        )
    for path in APPLICATION_MANIFESTS:
        package = tomllib.loads(path.read_text(encoding="utf-8"))["package"]
        declared = package["version"]
        application_version = actual if isinstance(declared, dict) and declared.get("workspace") else declared
        if application_version != actual:
            raise SystemExit(
                f"release: {package['name']} version {application_version} does not match "
                f"workspace version {actual}"
            )
    print(requested)


if __name__ == "__main__":
    main()
