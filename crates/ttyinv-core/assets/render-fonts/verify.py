#!/usr/bin/env python3
"""Verify the checked-in renderer font registry and its static SFNT faces.

This intentionally uses only the Python standard library so it can run before a
Rust toolchain is available. It never follows registry source URLs or paths.
"""
from __future__ import annotations

import hashlib
import json
import struct
import sys
from pathlib import Path
from typing import Any

EXPECTED_IDS = (
    "geist-mono",
    "cousine",
    "fira",
    "ibm-plex",
    "inconsolata",
    "jetbrains",
    "roboto",
    "source-code",
    "space",
    "ubuntu",
)
SLOT_KEYS = {"slot", "file", "sha256", "source", "weight", "family", "postscript", "name"}
FONT_KEYS = {"id", "label", "license", "regular", "semibold"}
PROVENANCE_KEYS = {
    "id", "slot", "file", "source", "upstream_commit", "upstream_release",
    "upstream_sha256", "distributed_sha256", "license", "family", "postscript",
    "name", "weight",
}
POLICY_KEYS = {"network", "source_hash_equality", "description"}
KNOWN_LICENSES = {"OFL-1.1": "OFL", "Ubuntu Font Licence 1.0": "UFL"}
LICENSE_MARKERS = {
    "OFL-1.1": "SIL OPEN FONT LICENSE Version 1.1",
    "Ubuntu Font Licence 1.0": "UBUNTU FONT LICENCE Version 1.0",
}


def fail(message: str) -> None:
    raise ValueError(message)


def u16(data: bytes, offset: int) -> int:
    if offset < 0 or offset + 2 > len(data):
        fail("truncated SFNT field")
    return struct.unpack_from(">H", data, offset)[0]


def sfnt_tables(data: bytes) -> dict[bytes, tuple[int, int]]:
    if len(data) < 12 or data[:4] not in (b"\x00\x01\x00\x00", b"true", b"OTTO"):
        fail("not an SFNT font")
    count = u16(data, 4)
    directory_end = 12 + count * 16
    if directory_end > len(data):
        fail("truncated SFNT directory")
    tables: dict[bytes, tuple[int, int]] = {}
    for index in range(count):
        row = 12 + index * 16
        tag = data[row : row + 4]
        offset = struct.unpack_from(">I", data, row + 8)[0]
        length = struct.unpack_from(">I", data, row + 12)[0]
        if offset > len(data) or length > len(data) - offset:
            fail(f"table {tag!r} escapes file")
        if tag in tables:
            fail(f"duplicate table {tag!r}")
        tables[tag] = (offset, length)
    return tables


def name_values(data: bytes, tables: dict[bytes, tuple[int, int]]) -> dict[int, set[str]]:
    if b"name" not in tables:
        fail("missing name table")
    offset, length = tables[b"name"]
    table = data[offset : offset + length]
    if len(table) < 6:
        fail("truncated name table")
    count = u16(table, 2)
    string_offset = u16(table, 4)
    if 6 + count * 12 > len(table) or string_offset > len(table):
        fail("invalid name table offsets")
    values: dict[int, set[str]] = {}
    for index in range(count):
        row = 6 + index * 12
        platform, encoding = u16(table, row), u16(table, row + 2)
        name_id, size, string_at = u16(table, row + 6), u16(table, row + 8), u16(table, row + 10)
        begin = string_offset + string_at
        end = begin + size
        if end > len(table):
            fail("name record escapes table")
        raw = table[begin:end]
        try:
            # Unicode and Windows name records are UTF-16BE. Macintosh Roman
            # is the only legacy encoding expected in these approved faces.
            text = raw.decode("utf-16-be" if platform in (0, 3) else "mac_roman")
        except UnicodeDecodeError as exc:
            fail(f"invalid name record encoding: {exc}")
        values.setdefault(name_id, set()).add(text)
    return values


def verify_face(root: Path, family: dict[str, Any], slot: str, record: dict[str, Any]) -> str:
    if set(record) != SLOT_KEYS:
        fail(f"{family['id']}/{slot}: slot schema mismatch")
    if record["slot"] != slot:
        fail(f"{family['id']}/{slot}: slot identifier mismatch")
    file_name = record["file"]
    if not isinstance(file_name, str) or not file_name or Path(file_name).name != file_name:
        fail(f"{family['id']}/{slot}: file must be a relative basename")
    if file_name in {".", ".."} or "/" in file_name or "\\" in file_name:
        fail(f"{family['id']}/{slot}: path traversal")
    if not file_name.lower().endswith(".ttf"):
        fail(f"{family['id']}/{slot}: renderer faces must be TTF")
    if not isinstance(record["weight"], int) or record["weight"] not in (400, 472, 600, 700):
        fail(f"{family['id']}/{slot}: unsupported declared weight")
    if not isinstance(record["sha256"], str) or len(record["sha256"]) != 64:
        fail(f"{family['id']}/{slot}: invalid SHA-256")
    if not isinstance(record["source"], str) or not record["source"].startswith("https://"):
        fail(f"{family['id']}/{slot}: source must be an HTTPS URL")
    for key in ("family", "postscript", "name"):
        if not isinstance(record[key], str) or not record[key]:
            fail(f"{family['id']}/{slot}: missing {key} metadata")

    path = root / file_name
    if not path.is_file() or path.is_symlink():
        fail(f"{family['id']}/{slot}: missing or symlinked face {file_name}")
    data = path.read_bytes()
    actual = hashlib.sha256(data).hexdigest()
    if actual != record["sha256"].lower():
        fail(f"{family['id']}/{slot}: SHA-256 mismatch")
    tables = sfnt_tables(data)
    if b"fvar" in tables:
        fail(f"{family['id']}/{slot}: variable font axes are forbidden")
    if b"OS/2" not in tables:
        fail(f"{family['id']}/{slot}: missing OS/2 table")
    os2_offset, os2_length = tables[b"OS/2"]
    if os2_length < 8:
        fail(f"{family['id']}/{slot}: truncated OS/2 table")
    weight_class = u16(data, os2_offset + 4)
    if weight_class != record["weight"]:
        fail(f"{family['id']}/{slot}: declared weight does not match OS/2 usWeightClass")
    names = name_values(data, tables)
    if record["family"] not in names.get(1, set()):
        fail(f"{family['id']}/{slot}: family name mismatch")
    if record["name"] not in names.get(4, set()):
        fail(f"{family['id']}/{slot}: full name mismatch")
    if record["postscript"] not in names.get(6, set()):
        fail(f"{family['id']}/{slot}: PostScript name mismatch")
    return actual


def verify_provenance(
    root: Path,
    registry: dict[str, Any],
    actual_hashes: dict[tuple[str, str], str],
) -> None:
    provenance_path = root / "provenance.json"
    if not provenance_path.is_file() or provenance_path.is_symlink():
        fail("missing or symlinked provenance manifest")
    provenance = json.loads(provenance_path.read_text(encoding="utf-8"))
    if not isinstance(provenance, dict) or set(provenance) != {"version", "policy", "faces"}:
        fail("provenance schema mismatch")
    if provenance["version"] != 1 or not isinstance(provenance["policy"], dict):
        fail("unsupported provenance version")
    policy = provenance["policy"]
    if set(policy) != POLICY_KEYS or policy["network"] != "never" or policy["source_hash_equality"] != "required":
        fail("provenance policy must require offline source-hash equality")
    if not isinstance(policy["description"], str) or not policy["description"].strip():
        fail("provenance policy description is required")
    if not isinstance(provenance["faces"], list) or len(provenance["faces"]) != 20:
        fail("provenance must contain exactly twenty faces")

    registry_faces = {
        (family["id"], slot): family[slot]
        for family in registry["fonts"]
        for slot in ("regular", "semibold")
    }
    seen: set[tuple[str, str]] = set()
    for face in provenance["faces"]:
        if not isinstance(face, dict) or set(face) != PROVENANCE_KEYS:
            fail("provenance face schema mismatch")
        key = (face["id"], face["slot"])
        if key in seen or key not in registry_faces:
            fail(f"duplicate or unknown provenance face {key!r}")
        seen.add(key)
        record = registry_faces[key]
        for field in ("file", "source", "license", "family", "postscript", "name", "weight"):
            if face[field] != (record[field] if field != "license" else next(
                family["license"] for family in registry["fonts"] if family["id"] == face["id"]
            )):
                fail(f"{face['id']}/{face['slot']}: provenance {field} mismatch")
        for field in ("upstream_sha256", "distributed_sha256"):
            value = face[field]
            if (
                not isinstance(value, str)
                or len(value) != 64
                or value != value.lower()
                or any(char not in "0123456789abcdef" for char in value)
            ):
                fail(f"{face['id']}/{face['slot']}: invalid provenance {field}")
        if face["upstream_sha256"] != face["distributed_sha256"]:
            fail(f"{face['id']}/{face['slot']}: upstream/distributed hash mismatch")
        if face["distributed_sha256"] != record["sha256"].lower():
            fail(f"{face['id']}/{face['slot']}: registry/distributed hash mismatch")
        if actual_hashes.get(key) != face["distributed_sha256"]:
            fail(f"{face['id']}/{face['slot']}: distributed hash does not match bytes")

        commit, release = face["upstream_commit"], face["upstream_release"]
        if commit is not None and (
            not isinstance(commit, str)
            or len(commit) != 40
            or any(char not in "0123456789abcdef" for char in commit)
        ):
            fail(f"{face['id']}/{face['slot']}: invalid upstream commit")
        if release is not None and (not isinstance(release, str) or not release.strip()):
            fail(f"{face['id']}/{face['slot']}: invalid upstream release")
        if (commit is None) == (release is None):
            fail(f"{face['id']}/{face['slot']}: exactly one upstream pin is required")
        if commit is not None and f"/{commit}/" not in face["source"]:
            fail(f"{face['id']}/{face['slot']}: source is not pinned to upstream commit")
        if release is not None and release not in face["source"]:
            fail(f"{face['id']}/{face['slot']}: source is not pinned to upstream release")
    if seen != set(registry_faces):
        fail("provenance is incomplete")


def main() -> int:
    root = Path(__file__).resolve().parent
    registry_path = root / "registry.json"
    try:
        registry = json.loads(registry_path.read_text(encoding="utf-8"))
        if not isinstance(registry, dict) or set(registry) != {"version", "fonts"}:
            fail("registry schema mismatch")
        if registry["version"] != 1 or not isinstance(registry["fonts"], list):
            fail("unsupported registry version")
        if len(registry["fonts"]) != len(EXPECTED_IDS):
            fail("registry must contain exactly twenty faces in ten families")
        ids = []
        actual_hashes: dict[tuple[str, str], str] = {}
        for family in registry["fonts"]:
            if not isinstance(family, dict) or set(family) != FONT_KEYS:
                fail("font family schema mismatch")
            family_id = family["id"]
            ids.append(family_id)
            if family_id not in EXPECTED_IDS or not isinstance(family["label"], str):
                fail(f"unknown family id {family_id!r}")
            license_name = family["license"]
            if license_name not in KNOWN_LICENSES:
                fail(f"{family_id}: unknown license")
            license_file_name = {"ubuntu": "ubuntu-UFL.txt", "geist-mono": "geist-OFL.txt"}.get(family_id, f"{family_id}-OFL.txt")
            license_file = root / "licenses" / license_file_name
            if not license_file.is_file() or license_file.is_symlink():
                fail(f"{family_id}: missing license text")
            license_text = license_file.read_text(encoding="utf-8")
            if not license_text.strip() or LICENSE_MARKERS[license_name] not in license_text:
                fail(f"{family_id}: license text does not match declared license")
            for slot in ("regular", "semibold"):
                actual_hashes[(family_id, slot)] = verify_face(root, family, slot, family[slot])
        if tuple(ids) != EXPECTED_IDS or len(set(ids)) != len(ids):
            fail("family IDs are unstable, incomplete, or duplicated")
        verify_provenance(root, registry, actual_hashes)
    except (OSError, UnicodeError, KeyError, json.JSONDecodeError, TypeError, ValueError) as exc:
        print(f"render-font verification failed: {exc}", file=sys.stderr)
        return 1
    print(f"verified {len(EXPECTED_IDS)} families / {len(EXPECTED_IDS) * 2} static faces")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
