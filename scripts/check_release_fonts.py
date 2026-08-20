#!/usr/bin/env python3
"""Fail a release build when the canonical Geist Mono assets are missing."""

from __future__ import annotations

from pathlib import Path

REQUIRED = (
    "GeistMono-Regular.woff2",
    "GeistMono-SemiBold.woff2",
    "OFL.txt",
)


def main() -> int:
    project_root = Path(__file__).resolve().parents[1]
    font_directory = project_root / "src" / "ttyinv" / "fonts"
    missing = [name for name in REQUIRED if not (font_directory / name).is_file()]
    if missing:
        print("release font check failed; missing:")
        for name in missing:
            print(f"- src/ttyinv/fonts/{name}")
        print("run `python scripts/vendor_geist_mono.py` first")
        return 1
    print("release font check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
