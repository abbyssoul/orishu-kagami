#!/usr/bin/env python3
"""Write the binary Homebrew formula for a published Orishu Kagami release."""

from __future__ import annotations

import argparse
from pathlib import Path
import re


HEX = re.compile(r"^[0-9a-f]{64}$")
REPOSITORY = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("formula", type=Path)
    parser.add_argument("version")
    parser.add_argument("repository")
    parser.add_argument("arm64_sha256")
    parser.add_argument("intel_sha256")
    args = parser.parse_args()

    if not REPOSITORY.fullmatch(args.repository):
        raise SystemExit("release: repository must be OWNER/REPOSITORY")
    for value in (args.arm64_sha256, args.intel_sha256):
        if not HEX.fullmatch(value):
            raise SystemExit(f"release: invalid SHA-256 {value!r}")

    base = f"https://github.com/{args.repository}/releases/download/v{args.version}"
    content = f'''class OrishuKagami < Formula
  desc "Distributed physical simulation tools and Kagami authoring client"
  homepage "https://github.com/{args.repository}"
  version "{args.version}"
  license "Apache-2.0"

  on_arm do
    url "{base}/orishu-kagami-{args.version}-aarch64-apple-darwin.tar.gz"
    sha256 "{args.arm64_sha256}"
  end

  on_intel do
    url "{base}/orishu-kagami-{args.version}-x86_64-apple-darwin.tar.gz"
    sha256 "{args.intel_sha256}"
  end

  depends_on :macos

  def install
    bin.install "orishu-worker", "orishuctl", "orishu-monitor", "kagami"
  end

  test do
    assert_match "Usage", shell_output("#{{bin}}/orishuctl --help")
    assert_match version.to_s, shell_output("#{{bin}}/kagami --version")
  end
end
'''
    args.formula.parent.mkdir(parents=True, exist_ok=True)
    args.formula.write_text(content, encoding="utf-8")


if __name__ == "__main__":
    main()
