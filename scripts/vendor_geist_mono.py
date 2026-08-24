#!/usr/bin/env python3
"""Vendor the OFL-licensed Geist Mono webfonts used by ttyinv.

The repository intentionally does not require network access at render time. Run this
script before building a release wheel so generated HTML and PDF outputs can embed the
canonical font instead of relying on a locally installed fallback.
"""

from __future__ import annotations

import argparse
import hashlib
import sys
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

DEFAULT_VERSION = "v1.7.2"
BASE_URL = "https://raw.githubusercontent.com/vercel/geist-font/refs/tags/{version}"
FILES = {
    "fonts/GeistMono/webfonts/GeistMono-Regular.woff2": (
        "GeistMono-Regular.woff2",
        "67b27e8a75395c074cc23656acf208ccd69e674da4cad2c7d6dfa254272ad7e8",
    ),
    "fonts/GeistMono/webfonts/GeistMono-SemiBold.woff2": (
        "GeistMono-SemiBold.woff2",
        "3609788228e2cb2c3ec156df82c1c1d9258b22cc9ed006fdaf1df33018a6ae9b",
    ),
    "OFL.txt": (
        "OFL.txt",
        "c683bfbcc7e087f5d37a54ef628f10387c451a83ddc459b151403a164ac46c90",
    ),
}


def _download(url: str) -> bytes:
    request = Request(url, headers={"User-Agent": "ttyinv font vendor script"})
    try:
        with urlopen(request, timeout=30) as response:
            return response.read()
    except (HTTPError, URLError, TimeoutError) as exc:
        raise RuntimeError(f"could not download {url}: {exc}") from exc


def _verify(contents: bytes, expected: str, filename: str) -> None:
    actual = hashlib.sha256(contents).hexdigest()
    if actual != expected:
        raise RuntimeError(
            f"checksum mismatch for {filename}: expected {expected}, got {actual}"
        )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--version",
        choices=(DEFAULT_VERSION,),
        default=DEFAULT_VERSION,
        help="pinned Geist release tag",
    )
    parser.add_argument("--force", action="store_true", help="overwrite existing files")
    args = parser.parse_args()

    project_root = Path(__file__).resolve().parents[1]
    destination = project_root / "src" / "ttyinv" / "fonts"
    destination.mkdir(parents=True, exist_ok=True)

    for source, (filename, expected_hash) in FILES.items():
        target = destination / filename
        if target.exists() and not args.force:
            try:
                _verify(target.read_bytes(), expected_hash, filename)
            except (OSError, RuntimeError) as exc:
                print(f"error: {exc}; pass --force to replace the file", file=sys.stderr)
                return 1
            print(f"keep {target.relative_to(project_root)}")
            continue
        url = f"{BASE_URL.format(version=args.version)}/{source}"
        try:
            contents = _download(url)
        except RuntimeError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 1
        try:
            _verify(contents, expected_hash, filename)
        except RuntimeError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 1
        target.write_bytes(contents)
        print(f"wrote {target.relative_to(project_root)}")

    print("Geist Mono vendored. Include OFL.txt alongside the font assets in distributions.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
