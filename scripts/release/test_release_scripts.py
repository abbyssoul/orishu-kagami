from __future__ import annotations

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("package.py")
SPEC = importlib.util.spec_from_file_location("release_package", SCRIPT)
assert SPEC and SPEC.loader
package = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(package)


class PackageTests(unittest.TestCase):
    def test_tar_and_zip_are_reproducible_and_have_exact_members(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary_dir = root / "bin"
            binary_dir.mkdir()
            (binary_dir / "tool").write_bytes(b"unix executable")
            (binary_dir / "tool.exe").write_bytes(b"windows executable")

            original_documents = package.DOCUMENTS
            package.DOCUMENTS = (Path("README.md"),)
            try:
                for target, archive_format in (
                    ("x86_64-unknown-linux-gnu", "tar.gz"),
                    ("x86_64-pc-windows-msvc", "zip"),
                ):
                    items = package.entries("suite", "1.2.3", target, binary_dir, ["tool"])
                    first = root / f"first.{archive_format}"
                    second = root / f"second.{archive_format}"
                    writer = package.package_tar if archive_format == "tar.gz" else package.package_zip
                    writer(first, items)
                    writer(second, items)
                    self.assertEqual(package.sha256(first), package.sha256(second))
                    self.assertEqual(
                        package.member_names(first),
                        package.expected_names("suite", "1.2.3", target, ["tool"]),
                    )
            finally:
                package.DOCUMENTS = original_documents

    def test_homebrew_formula_writer_validates_and_renders_archives(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            formula = Path(directory) / "Formula" / "orishu-kagami.rb"
            subprocess.run(
                [
                    "python3",
                    "scripts/release/write-homebrew-formula.py",
                    str(formula),
                    "1.2.3",
                    "owner/repository",
                    "a" * 64,
                    "b" * 64,
                ],
                check=True,
            )
            content = formula.read_text(encoding="utf-8")
            self.assertIn("orishu-kagami-1.2.3-aarch64-apple-darwin.tar.gz", content)
            self.assertIn("orishu-kagami-1.2.3-x86_64-apple-darwin.tar.gz", content)
            self.assertIn('bin.install "orishu-worker", "orishuctl", "orishu-monitor", "kagami"', content)


if __name__ == "__main__":
    unittest.main()
