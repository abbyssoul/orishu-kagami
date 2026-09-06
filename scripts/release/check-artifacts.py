#!/usr/bin/env python3
"""Check a release candidate set and write its deterministic SHA256SUMS."""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path


TARGET_ARCHIVES = (
    "orishu-kagami-{version}-x86_64-unknown-linux-gnu.tar.gz",
    "orishu-kagami-{version}-aarch64-unknown-linux-gnu.tar.gz",
    "orishu-kagami-{version}-aarch64-apple-darwin.tar.gz",
    "orishu-kagami-{version}-x86_64-apple-darwin.tar.gz",
    "kagami-{version}-x86_64-pc-windows-msvc.zip",
)
DEB_PACKAGES = ("orishuctl", "orishu-worker", "orishu-monitor")
DEB_ARCHES = ("amd64", "arm64")


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("version")
    args = parser.parse_args()

    expected = [name.format(version=args.version) for name in TARGET_ARCHIVES]
    expected.extend(
        f"{package}_{args.version}-1_{architecture}.deb"
        for package in DEB_PACKAGES
        for architecture in DEB_ARCHES
    )
    permitted = set(expected) | {"SHA256SUMS"}
    actual = {path.name for path in args.directory.iterdir() if path.is_file()}
    missing = sorted(set(expected) - actual)
    unexpected = sorted(actual - permitted)
    if missing or unexpected:
        raise SystemExit(f"release: invalid artifact set; missing={missing}, unexpected={unexpected}")

    content = "".join(f"{digest(args.directory / name)}  {name}\n" for name in expected)
    checksum_file = args.directory / "SHA256SUMS"
    if checksum_file.exists() and checksum_file.read_text(encoding="utf-8") != content:
        raise SystemExit("release: staged artifacts do not match the existing SHA256SUMS")
    checksum_file.write_text(content, encoding="utf-8")
    print(f"release: staged {len(expected)} artifacts for {args.version}")


if __name__ == "__main__":
    main()
