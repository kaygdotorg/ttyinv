#!/usr/bin/env python3
"""Verify the embedded Geist Mono TTF files without third-party packages."""

from __future__ import annotations

import hashlib
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
MANIFEST = ROOT / "manifest.json"
PRINTABLE = range(0x20, 0x7F)


def u16(data: bytes, offset: int) -> int:
    return struct.unpack_from(">H", data, offset)[0]


def i16(data: bytes, offset: int) -> int:
    return struct.unpack_from(">h", data, offset)[0]


def u32(data: bytes, offset: int) -> int:
    return struct.unpack_from(">I", data, offset)[0]


def table_directory(data: bytes) -> dict[str, tuple[int, int]]:
    if len(data) < 12:
        raise ValueError("font is shorter than an sfnt header")
    count = u16(data, 4)
    directory_end = 12 + count * 16
    if directory_end > len(data):
        raise ValueError("sfnt table directory exceeds the file")
    tables: dict[str, tuple[int, int]] = {}
    for index in range(count):
        base = 12 + index * 16
        tag = data[base : base + 4].decode("ascii")
        offset = u32(data, base + 8)
        length = u32(data, base + 12)
        if offset + length > len(data):
            raise ValueError(f"{tag} table exceeds the file")
        tables[tag] = (offset, length)
    return tables


def glyph_for_format_4(data: bytes, offset: int, codepoint: int) -> int:
    segments = u16(data, offset + 6) // 2
    end_code = offset + 14
    start_code = end_code + 2 * segments + 2
    id_delta = start_code + 2 * segments
    id_range_offset = id_delta + 2 * segments
    for index in range(segments):
        end = u16(data, end_code + 2 * index)
        if codepoint > end:
            continue
        start = u16(data, start_code + 2 * index)
        if codepoint < start:
            return 0
        delta = i16(data, id_delta + 2 * index)
        range_offset = u16(data, id_range_offset + 2 * index)
        if range_offset == 0:
            return (codepoint + delta) & 0xFFFF
        glyph_address = (
            id_range_offset
            + 2 * index
            + range_offset
            + 2 * (codepoint - start)
        )
        glyph = u16(data, glyph_address)
        return (glyph + delta) & 0xFFFF if glyph else 0
    return 0


def glyph_for_format_12(data: bytes, offset: int, codepoint: int) -> int:
    groups = u32(data, offset + 12)
    for index in range(groups):
        base = offset + 16 + 12 * index
        start = u32(data, base)
        end = u32(data, base + 4)
        if codepoint < start:
            return 0
        if codepoint <= end:
            return u32(data, base + 8) + codepoint - start
    return 0


def unicode_subtable(data: bytes, tables: dict[str, tuple[int, int]]) -> tuple[int, int]:
    cmap_offset, cmap_length = tables["cmap"]
    if cmap_length < 4:
        raise ValueError("cmap table is truncated")
    count = u16(data, cmap_offset + 2)
    preferred: list[tuple[int, int, int]] = []
    for index in range(count):
        base = cmap_offset + 4 + 8 * index
        platform = u16(data, base)
        encoding = u16(data, base + 2)
        sub_offset = u32(data, base + 4)
        absolute = cmap_offset + sub_offset
        if absolute + 2 > cmap_offset + cmap_length:
            raise ValueError("cmap subtable exceeds cmap")
        format_number = u16(data, absolute)
        if platform == 3 and encoding in (1, 10) and format_number in (4, 12):
            preferred.append((format_number, absolute, encoding))
    if not preferred:
        raise ValueError("no Windows Unicode cmap subtable")
    preferred.sort(key=lambda item: (item[0] != 12, item[2] != 10))
    return preferred[0][0], preferred[0][1]


def inspect(path: Path) -> dict[str, object]:
    data = path.read_bytes()
    if data[:4] != b"\x00\x01\x00\x00":
        raise ValueError("missing TrueType sfnt magic")
    tables = table_directory(data)
    required = {"head", "hhea", "maxp", "hmtx", "cmap"}
    missing_tables = sorted(required - tables.keys())
    if missing_tables:
        raise ValueError(f"missing required tables: {', '.join(missing_tables)}")

    head = tables["head"][0]
    hhea = tables["hhea"][0]
    maxp = tables["maxp"][0]
    hmtx = tables["hmtx"][0]
    units_per_em = u16(data, head + 18)
    ascent = i16(data, hhea + 4)
    descent = i16(data, hhea + 6)
    line_gap = i16(data, hhea + 8)
    glyph_count = u16(data, maxp + 4)
    metric_count = u16(data, hhea + 34)
    if not units_per_em or not glyph_count or not metric_count <= glyph_count:
        raise ValueError("invalid sfnt metric counts")

    format_number, cmap_offset = unicode_subtable(data, tables)
    glyphs: dict[int, int] = {}
    for codepoint in PRINTABLE:
        if format_number == 4:
            glyphs[codepoint] = glyph_for_format_4(data, cmap_offset, codepoint)
        else:
            glyphs[codepoint] = glyph_for_format_12(data, cmap_offset, codepoint)
    missing = [codepoint for codepoint, glyph in glyphs.items() if not glyph]
    if missing:
        raise ValueError("missing printable glyphs: " + ", ".join(f"U+{c:04X}" for c in missing))

    advances: set[int] = set()
    for glyph in glyphs.values():
        metric_index = min(glyph, metric_count - 1)
        advances.add(u16(data, hmtx + 4 * metric_index))
    if len(advances) != 1:
        raise ValueError(f"printable glyphs have non-fixed advances: {sorted(advances)}")

    return {
        "sha256": hashlib.sha256(data).hexdigest(),
        "size_bytes": len(data),
        "magic": data[:4].hex(),
        "units_per_em": units_per_em,
        "ascent": ascent,
        "descent": descent,
        "line_gap": line_gap,
        "glyph_count": glyph_count,
        "printable_ascii": {
            "range": "U+0020-U+007E",
            "count": len(glyphs),
            "missing": [],
            "advance_widths": sorted(advances),
        },
    }


def main() -> int:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    failures: list[str] = []
    for entry in manifest["fonts"]:
        path = ROOT / entry["path"].split("/", 2)[-1]
        try:
            measured = inspect(path)
        except (OSError, ValueError, struct.error) as error:
            failures.append(f"{path.name}: {error}")
            continue
        for field in ("sha256", "size_bytes", "magic", "units_per_em", "ascent", "descent", "line_gap", "glyph_count"):
            if measured[field] != entry[field]:
                failures.append(f"{path.name}: {field}={measured[field]!r}, expected {entry[field]!r}")
        if measured["sha256"] != entry["source_sha256"] or measured["sha256"] != entry["generated_sha256"]:
            failures.append(f"{path.name}: source/generated hash does not match bytes")
    if failures:
        print("font verification failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print(f"verified {len(manifest['fonts'])} TrueType fonts; printable ASCII is covered at fixed advance")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
