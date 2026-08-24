#!/usr/bin/env python3
"""Fail a release build when the canonical Geist Mono assets are missing."""

from __future__ import annotations

import argparse
import tarfile
import zipfile
from pathlib import Path

REQUIRED = (
    "GeistMono-Regular.woff2",
    "GeistMono-SemiBold.woff2",
    "OFL.txt",
)


def _archive_names(path: Path) -> set[str]:
    if path.suffix == ".whl":
        with zipfile.ZipFile(path) as archive:
            return set(archive.namelist())
    with tarfile.open(path) as archive:
        return set(archive.getnames())


def _missing_from_archive(path: Path) -> list[str]:
    names = _archive_names(path)
    return [
        name
        for name in REQUIRED
        if not any(
            member == f"ttyinv/fonts/{name}"
            or member.endswith(f"/ttyinv/fonts/{name}")
            for member in names
        )
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dist",
        type=Path,
        help="also inspect wheel and sdist archives in this directory",
    )
    args = parser.parse_args()
    project_root = Path(__file__).resolve().parents[1]
    font_directory = project_root / "src" / "ttyinv" / "fonts"
    missing = [name for name in REQUIRED if not (font_directory / name).is_file()]
    if missing:
        print("release font check failed; missing:")
        for name in missing:
            print(f"- src/ttyinv/fonts/{name}")
        print("run `python scripts/vendor_geist_mono.py` first")
        return 1
    if args.dist:
        archives = sorted((*args.dist.glob("*.whl"), *args.dist.glob("*.tar.gz")))
        if not archives:
            print(f"release font check failed; no wheel or sdist found in {args.dist}")
            return 1
        failures = [
            (archive, archive_missing)
            for archive in archives
            if (archive_missing := _missing_from_archive(archive))
        ]
        if failures:
            print("release font check failed; distribution assets missing:")
            for archive, archive_missing in failures:
                print(f"- {archive}: {', '.join(archive_missing)}")
            return 1
    print("release font check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
