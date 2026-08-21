#!/usr/bin/env python3
"""Fail when repository content resembles private invoice or credential data."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
SKIP_DIRS = {".git", ".venv", "venv", "build", "dist", "__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache"}
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
    return sorted(
        path for path in root.rglob("*")
        if path.is_file() and not any(part in SKIP_DIRS for part in path.relative_to(root).parts)
    )


def allow_vendored_font(path: Path) -> bool:
    relative = path.relative_to(ROOT)
    return relative.parts[:3] == ("src", "ttyinv", "fonts") and (ROOT / "src/ttyinv/fonts/OFL.txt").exists()


def main() -> int:
    failures: list[str] = []
    for path in tracked_candidates(ROOT):
        relative = path.relative_to(ROOT)
        name = path.name.casefold()
        if path.name in SKIP_FILES:
            continue
        if name == ".env" or name.startswith(".env.") or name.startswith("id_rsa") or name.startswith("id_ed25519"):
            failures.append(f"forbidden credential-shaped file: {relative}")
            continue
        suffix = path.suffix.casefold()
        if suffix in FORBIDDEN_SUFFIXES:
            failures.append(f"forbidden private/binary file type: {relative}")
            continue
        if suffix in FONT_SUFFIXES and not allow_vendored_font(path):
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
