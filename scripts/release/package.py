#!/usr/bin/env python3
"""Build and inspect deterministic Orishu Kagami binary archives."""

from __future__ import annotations

import argparse
import gzip
import hashlib
from pathlib import Path, PurePosixPath
import shutil
import tarfile
import zipfile


DOCUMENTS = (Path("README.md"), Path("LICENSE"), Path("docs/install.md"))


def archive_name(product: str, version: str, target: str, archive_format: str) -> str:
    suffix = ".tar.gz" if archive_format == "tar.gz" else ".zip"
    return f"{product}-{version}-{target}{suffix}"


def entries(
    product: str,
    version: str,
    target: str,
    binary_dir: Path,
    binaries: list[str],
) -> list[tuple[Path, PurePosixPath, int]]:
    root = PurePosixPath(f"{product}-{version}-{target}")
    executable_suffix = ".exe" if "windows" in target else ""
    result: list[tuple[Path, PurePosixPath, int]] = []
    for binary in binaries:
        source = binary_dir / f"{binary}{executable_suffix}"
        result.append((source, root / source.name, 0o755))
    for document in DOCUMENTS:
        result.append((document, root / document, 0o644))
    return result


def require_files(items: list[tuple[Path, PurePosixPath, int]]) -> None:
    missing = [str(source) for source, _, _ in items if not source.is_file()]
    if missing:
        raise SystemExit("release: missing input files: " + ", ".join(missing))


def package_tar(output: Path, items: list[tuple[Path, PurePosixPath, int]]) -> None:
    with output.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as archive:
                roots: set[PurePosixPath] = set()
                for _, destination, _ in items:
                    roots.update(destination.parents)
                for directory in sorted((path for path in roots if str(path) != "."), key=str):
                    info = tarfile.TarInfo(f"{directory}/")
                    info.type = tarfile.DIRTYPE
                    info.mode = 0o755
                    info.mtime = 0
                    archive.addfile(info)
                for source, destination, mode in sorted(items, key=lambda item: str(item[1])):
                    info = tarfile.TarInfo(str(destination))
                    info.size = source.stat().st_size
                    info.mode = mode
                    info.mtime = 0
                    with source.open("rb") as data:
                        archive.addfile(info, data)


def package_zip(output: Path, items: list[tuple[Path, PurePosixPath, int]]) -> None:
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for source, destination, mode in sorted(items, key=lambda item: str(item[1])):
            info = zipfile.ZipInfo(str(destination), date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = (mode & 0xFFFF) << 16
            with source.open("rb") as data, archive.open(info, "w") as destination_file:
                shutil.copyfileobj(data, destination_file, length=1024 * 1024)


def member_names(path: Path) -> list[str]:
    if path.name.endswith(".tar.gz"):
        with tarfile.open(path, "r:gz") as archive:
            return sorted(member.name.rstrip("/") for member in archive.getmembers() if member.isfile())
    with zipfile.ZipFile(path) as archive:
        return sorted(name.rstrip("/") for name in archive.namelist() if not name.endswith("/"))


def expected_names(product: str, version: str, target: str, binaries: list[str]) -> list[str]:
    root = PurePosixPath(f"{product}-{version}-{target}")
    suffix = ".exe" if "windows" in target else ""
    names = [str(root / f"{binary}{suffix}") for binary in binaries]
    names.extend(str(root / document) for document in DOCUMENTS)
    return sorted(names)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--product", default="orishu-kagami")
    parser.add_argument("--version", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--format", choices=("tar.gz", "zip"), required=True)
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, default=Path("dist"))
    parser.add_argument("--binary", action="append", dest="binaries", required=True)
    args = parser.parse_args()

    items = entries(args.product, args.version, args.target, args.binary_dir, args.binaries)
    require_files(items)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    output = args.output_dir / archive_name(args.product, args.version, args.target, args.format)
    if args.format == "tar.gz":
        package_tar(output, items)
    else:
        package_zip(output, items)

    actual = member_names(output)
    expected = expected_names(args.product, args.version, args.target, args.binaries)
    if actual != expected:
        raise SystemExit(f"release: archive members differ\nexpected={expected}\nactual={actual}")
    print(f"{output}  sha256:{sha256(output)}")


if __name__ == "__main__":
    main()
