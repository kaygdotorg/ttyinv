from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXCLUDED_DIRECTORIES = {".git", ".pytest_cache", ".venv", "__pycache__", "build", "dist", "htmlcov"}
FORBIDDEN_DIRECTORIES = {"private", "reference", "real-invoices"}
FORBIDDEN_EXTENSIONS = {".pdf", ".png", ".jpg", ".jpeg", ".webp", ".tif", ".tiff"}
ALLOWED_EMAIL_DOMAINS = {"example.com", "example.org", "example.net"}
TEXT_SKIP = {"LICENSE"}

failures: list[str] = []

for path in ROOT.rglob("*"):
    relative = path.relative_to(ROOT)
    if any(part in EXCLUDED_DIRECTORIES for part in relative.parts):
        continue
    if path.is_dir():
        if path.name in FORBIDDEN_DIRECTORIES:
            failures.append(f"{relative}: private/reference directory must not be included")
        continue
    if path.suffix.lower() in FORBIDDEN_EXTENSIONS:
        failures.append(f"{relative}: raster images and PDFs are blocked by the privacy gate")
        continue
    if path.name in TEXT_SKIP or path.stat().st_size > 1_000_000:
        continue
    try:
        contents = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        continue

    if re.search(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----", contents):
        failures.append(f"{relative}: contains a private key")
    if re.search(r"\b[A-Z]{5}[0-9]{4}[A-Z]\b", contents):
        failures.append(f"{relative}: contains a PAN-like identifier")
    if re.search(r"\b[0-9]{2}[A-Z]{5}[0-9]{4}[A-Z][0-9A-Z]Z[0-9A-Z]\b", contents):
        failures.append(f"{relative}: contains a GSTIN-like identifier")
    for match in re.finditer(r"\b[A-Z0-9._%+-]+@([A-Z0-9.-]+\.[A-Z]{2,})\b", contents, re.I):
        domain = match.group(1).lower()
        if domain not in ALLOWED_EMAIL_DOMAINS:
            failures.append(f"{relative}: contains a non-example email domain ({domain})")

if failures:
    print("Privacy check failed:", file=sys.stderr)
    for failure in failures:
        print(f"- {failure}", file=sys.stderr)
    raise SystemExit(1)

print("privacy check passed")
