#!/usr/bin/env python3
"""Fail when repository content resembles private invoice or credential data."""

from __future__ import annotations

import hashlib
import os
import re
import sys
from pathlib import Path
from typing import Iterable

SKIP_DIRS = {
    ".git", ".next", ".venv", "venv", "build", "data", "dist", "node_modules",
    "__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache",
}
SKIP_FILES = {"LICENSE"}
FORBIDDEN_SUFFIXES = {
    ".pdf", ".png", ".jpg", ".jpeg", ".webp", ".gif", ".tif", ".tiff",
    ".p12", ".pfx", ".jks", ".keystore", ".pem", ".key",
}
FONT_SUFFIXES = {".ttf", ".otf", ".woff", ".woff2"}
TEXT_SUFFIXES = {
    ".md", ".txt", ".toml", ".yaml", ".yml", ".json", ".py", ".css", ".html",
    ".js", ".ts", ".tsx", ".jsx", ".svg", ".sh", ".ini", ".cfg", ".xml",
}
ALLOWED_BINARY_FILES = {
    Path("docs/screenshots/editor-desktop.jpg"): "a78c0898356becb229780e3aadd69157c43f542ac97e032b8c566266e5300413",
    Path("docs/screenshots/editor-tablet.jpg"): "5477dc2babbf6ae00d6c752f384cd3a590a3bbf0f0c64c9a9b7b091f85ca76c8",
    Path("docs/screenshots/editor-mobile.jpg"): "9b7002df01d63ef9b979fdd0e12c37719f49911e3157e594fc9415517b764490",
}


# These are reviewed, deterministic outputs.  A path-only allowlist would let
# a private screenshot or an arbitrary replacement binary into the repository.

PATTERNS = [
    ("private-key", re.compile(r"-----BEGIN (?:RSA |OPENSSH |EC |DSA )?PRIVATE KEY-----")),
    ("aws-access-key", re.compile(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b")),
    ("github-token", re.compile(r"\b(?:gh[pousr]_[A-Za-z0-9_]{30,}|github_pat_[A-Za-z0-9_]{40,})\b")),
    ("slack-token", re.compile(r"\bxox[baprs]-[A-Za-z0-9-]{20,}\b")),
    ("india-pan", re.compile(r"\b[A-Z]{5}[0-9]{4}[A-Z]\b")),
    ("india-gstin", re.compile(r"\b[0-9]{2}[A-Z]{5}[0-9]{4}[A-Z][A-Z0-9]Z[A-Z0-9]\b")),
    ("iban", re.compile(r"\b[A-Z]{2}[0-9]{2}[A-Z0-9]{11,30}\b")),
    ("non-example-email", re.compile(r"\b[A-Z0-9._%+-]+@(?!example\.(?:com|org|net)\b)[A-Z0-9.-]+\.[A-Z]{2,}\b", re.I)),
]


def is_probable_card(value: str) -> bool:
    digits = [int(character) for character in re.sub(r"[^0-9]", "", value)]
    if not 13 <= len(digits) <= 19 or len(set(digits)) == 1:
        return False
    total = 0
    parity = len(digits) % 2
    for index, digit in enumerate(digits):
        if index % 2 == parity:
            digit *= 2
            if digit > 9:
                digit -= 9
        total += digit
    return total % 10 == 0


def tracked_candidates(root: Path) -> list[Path]:
    candidates: list[Path] = []
    for directory, child_dirs, files in os.walk(root):
        current = Path(directory)
        child_dirs[:] = sorted(child for child in child_dirs if child not in SKIP_DIRS)
        candidates.extend(current / name for name in files)
    return sorted(candidates)


def allow_vendored_font(path: Path, root: Path) -> bool:
    relative = path.relative_to(root)
    return relative.parts[:3] == ("src", "ttyinv", "fonts") and (root / "src/ttyinv/fonts/OFL.txt").exists()


def main(argv: Iterable[str] | None = None) -> int:
    arguments = list(sys.argv[1:] if argv is None else argv)
    root = Path(arguments[0] if arguments else ".").resolve()
    failures: list[str] = []
    for path in tracked_candidates(root):
        relative = path.relative_to(root)
        name = path.name.casefold()
        if path.name in SKIP_FILES:
            continue
        if (name == ".env" or (name.startswith(".env.") and name != ".env.example")) or name.startswith("id_rsa") or name.startswith("id_ed25519"):
            failures.append(f"forbidden credential-shaped file: {relative}")
            continue
        suffix = path.suffix.casefold()
        if suffix in FORBIDDEN_SUFFIXES:
            expected_digest = ALLOWED_BINARY_FILES.get(relative)
            if expected_digest is None:
                failures.append(f"forbidden private/binary file type: {relative}")
                continue
            try:
                actual_digest = hashlib.sha256(path.read_bytes()).hexdigest()
            except OSError as exc:
                failures.append(f"unreviewed binary {relative}: cannot read file: {exc}")
            else:
                if actual_digest != expected_digest:
                    failures.append(f"unreviewed binary {relative}: SHA-256 does not match the reviewed asset")
            continue
        if suffix in FONT_SUFFIXES and not allow_vendored_font(path, root):
            failures.append(f"unreviewed font binary: {relative}")
            continue
        if suffix not in TEXT_SUFFIXES and path.name not in {"Makefile", ".gitignore", ".gitleaks.toml"}:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        for label, pattern in PATTERNS:
            if match := pattern.search(text):
                excerpt = match.group(0)
                if label == "non-example-email" and relative.parts[:2] == (".github", "workflows"):
                    continue
                failures.append(f"{label} pattern in {relative}: {excerpt[:8]}…")
        for match in re.finditer(r"(?<![A-Za-z0-9])(?:[0-9][ -]?){13,19}(?![A-Za-z0-9])", text):
            if is_probable_card(match.group(0)):
                failures.append(f"payment-card pattern in {relative}")
                break

    if failures:
        print("privacy check failed:", file=sys.stderr)
        for failure in sorted(set(failures)):
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print("privacy check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
