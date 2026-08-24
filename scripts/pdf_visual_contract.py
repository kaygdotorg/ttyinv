#!/usr/bin/env python3
"""Assert deterministic A4 pagination and theme-independent PDF geometry."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from pathlib import Path
from typing import Any

from pypdf import PdfReader
from pypdf.generic import ContentStream

A4_RATIO = 210 / 297
COLOR_OPERATORS = {b"G", b"K", b"RG", b"SC", b"SCN", b"g", b"k", b"rg", b"sc", b"scn"}


def _operand(value: Any) -> Any:
    if isinstance(value, bytes):
        return {"bytes": value.hex()}
    if isinstance(value, (list, tuple)):
        return [_operand(item) for item in value]
    if isinstance(value, dict):
        return {str(key): _operand(item) for key, item in sorted(value.items(), key=lambda pair: str(pair[0]))}
    return str(value)


def geometry_signatures(reader: PdfReader) -> list[str]:
    """Hash every paint/text operation except theme-specific colour values."""

    signatures: list[str] = []
    for page in reader.pages:
        stream = ContentStream(page.get_contents(), reader)
        operations = [
            [operator.decode("ascii"), _operand(operands)]
            for operands, operator in stream.operations
            if operator not in COLOR_OPERATORS
        ]
        encoded = json.dumps(operations, ensure_ascii=True, separators=(",", ":")).encode()
        signatures.append(hashlib.sha256(encoded).hexdigest())
    return signatures


def inspect_pdf(path: Path) -> dict[str, Any]:
    reader = PdfReader(str(path))
    pages: list[dict[str, Any]] = []
    for number, page in enumerate(reader.pages, start=1):
        width, height = float(page.mediabox.width), float(page.mediabox.height)
        text = page.extract_text() or ""
        pages.append(
            {
                "number": number,
                "width": width,
                "height": height,
                "ratio": width / height,
                "text": text,
                "corner_glyphs": text.count("+"),
            }
        )
    return {"path": str(path), "pages": pages, "geometry_signatures": geometry_signatures(reader)}


def _page_for(pages: list[dict[str, Any]], marker: str) -> int | None:
    normalized_marker = " ".join(marker.split())
    matches = [
        page["number"]
        for page in pages
        if normalized_marker in " ".join(page["text"].split())
    ]
    return matches[0] if len(matches) == 1 else None


def assert_stress_contract(pages: list[dict[str, Any]], failures: list[str]) -> None:
    row_markers = [
        *(f"Architecture session {number:02d}" for number in range(1, 11)),
        *(f"Implementation batch {number:02d}" for number in range(1, 11)),
        *(f"Verification pass {number:02d}" for number in range(1, 6)),
        *(f"Documentation chapter {number:02d}" for number in range(1, 6)),
        *(f"Handover workshop {number:02d}" for number in range(1, 3)),
    ]
    for marker in row_markers:
        if _page_for(pages, marker) is None:
            failures.append(f"stress row {marker!r} is missing or split across pages")

    for page in pages:
        if any(marker in page["text"] for marker in row_markers):
            header = "Description Hours Rate Amount (EUR)"
            if header not in page["text"]:
                failures.append(f"table header is not repeated on page {page['number']}")

    together = {
        "section label and first row": ["[ Implementation work ]", "Architecture session 01"],
        "last row and totals": ["Handover workshop 02", "Total due"],
        "trailing prose label and body": ["[ Notes ]", "This deliberately long fixture"],
        "payment and signature": ["[ Payment Methods ]", "Example Approver", "Fabricated authorization mark"],
    }
    for label, markers in together.items():
        locations = [_page_for(pages, marker) for marker in markers]
        if None in locations or len(set(locations)) != 1:
            failures.append(f"{label} are not kept together: {dict(zip(markers, locations, strict=True))}")


def assert_long_row_contract(pages: list[dict[str, Any]], failures: list[str]) -> None:
    markers = [
        "Extended compatibility verification",
        "A4 page boundary",
        "€\u00a0900.00",
    ]
    locations = [_page_for(pages, marker) for marker in markers]
    if None in locations or len(set(locations)) != 1:
        failures.append(f"long row is missing, clipped, or split: {dict(zip(markers, locations, strict=True))}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("light", type=Path)
    parser.add_argument("dark", type=Path)
    parser.add_argument("--case", required=True)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--expected-pages", type=int, required=True)
    parser.add_argument("--stress", action="store_true")
    parser.add_argument("--report", type=Path)
    parser.add_argument("--write-baseline", action="store_true")
    args = parser.parse_args()

    light, dark = inspect_pdf(args.light), inspect_pdf(args.dark)
    failures: list[str] = []
    light_pages, dark_pages = light["pages"], dark["pages"]

    for theme, pages in (("light", light_pages), ("dark", dark_pages)):
        if len(pages) != args.expected_pages:
            failures.append(f"{theme} page count is {len(pages)}; expected {args.expected_pages}")
        for page in pages:
            if not math.isclose(page["ratio"], A4_RATIO, abs_tol=0.001):
                failures.append(f"{theme} page {page['number']} is not A4: {page['width']:.2f} x {page['height']:.2f} pt")
            if page["corner_glyphs"] != 4:
                failures.append(f"{theme} page {page['number']} has {page['corner_glyphs']} frame corners; expected 4")

    if light["geometry_signatures"] != dark["geometry_signatures"]:
        failures.append("light and dark PDF geometry differ")

    baseline = json.loads(args.baseline.read_text(encoding="utf-8")) if args.baseline.exists() else {}
    current = light["geometry_signatures"]
    if args.write_baseline:
        expected = baseline.setdefault(args.case, {})
        expected["expected_pages"] = args.expected_pages
        signatures_by_platform = expected.setdefault("geometry_signatures_by_platform", {})
        signatures_by_platform[sys.platform] = current
        expected.pop("geometry_signatures", None)
        args.baseline.parent.mkdir(parents=True, exist_ok=True)
        args.baseline.write_text(json.dumps(baseline, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    else:
        expected = baseline.get(args.case)
        if not expected:
            failures.append(f"no committed geometry baseline for {args.case!r}")
        else:
            if expected.get("expected_pages") != args.expected_pages:
                failures.append(f"baseline page count for {args.case!r} is stale")
            signatures_by_platform = expected.get("geometry_signatures_by_platform", {})
            expected_signatures = signatures_by_platform.get(sys.platform, expected.get("geometry_signatures"))
            if expected_signatures is None:
                failures.append(f"no geometry baseline for platform {sys.platform!r} in {args.case!r}")
            elif expected_signatures != current:
                failures.append(
                    f"geometry baseline changed for {args.case!r}: "
                    f"expected={json.dumps(expected_signatures, separators=(',', ':'))} "
                    f"current={json.dumps(current, separators=(',', ':'))}"
                )

    if args.stress:
        assert_stress_contract(light_pages, failures)
        assert_stress_contract(dark_pages, failures)
    if args.case == "long-row":
        assert_long_row_contract(light_pages, failures)
        assert_long_row_contract(dark_pages, failures)

    report = {
        "case": args.case,
        "expected_pages": args.expected_pages,
        "light": {key: value for key, value in light.items() if key != "pages"},
        "dark": {key: value for key, value in dark.items() if key != "pages"},
        "light_pages": [{key: value for key, value in page.items() if key != "text"} for page in light_pages],
        "dark_pages": [{key: value for key, value in page.items() if key != "text"} for page in dark_pages],
        "failures": failures,
    }
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    if failures:
        for failure in failures:
            print(f"pdf-visual-contract: {failure}")
        return 1
    print(f"pdf-visual-contract: {args.case}: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
