#!/usr/bin/env python3
"""Vendor the OFL-licensed Geist Mono webfonts used by ttyinv.

The repository intentionally does not require network access at render time. Run this
script before building a release wheel so generated HTML and PDF outputs can embed the
canonical font instead of relying on a locally installed fallback.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

DEFAULT_VERSION = "1.7.2"
BASE_URL = "https://raw.githubusercontent.com/vercel/geist-font/refs/tags/{version}"
FILES = {
    "fonts/GeistMono/webfonts/GeistMono-Regular.woff2": "GeistMono-Regular.woff2",
    "fonts/GeistMono/webfonts/GeistMono-SemiBold.woff2": "GeistMono-SemiBold.woff2",
    "OFL.txt": "OFL.txt",
}


def _download(url: str) -> bytes:
    request = Request(url, headers={"User-Agent": "ttyinv font vendor script"})
    try:
        with urlopen(request, timeout=30) as response:
            return response.read()
    except (HTTPError, URLError, TimeoutError) as exc:
        raise RuntimeError(f"could not download {url}: {exc}") from exc


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", default=DEFAULT_VERSION, help="Geist release tag")
    parser.add_argument("--force", action="store_true", help="overwrite existing files")
    args = parser.parse_args()

    project_root = Path(__file__).resolve().parents[1]
    destination = project_root / "src" / "ttyinv" / "fonts"
    destination.mkdir(parents=True, exist_ok=True)

    for source, filename in FILES.items():
        target = destination / filename
        if target.exists() and not args.force:
            print(f"keep {target.relative_to(project_root)}")
            continue
        url = f"{BASE_URL.format(version=args.version)}/{source}"
        try:
            contents = _download(url)
        except RuntimeError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 1
        target.write_bytes(contents)
        print(f"wrote {target.relative_to(project_root)}")

    print("Geist Mono vendored. Include OFL.txt alongside the font assets in distributions.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
