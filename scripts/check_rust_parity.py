#!/usr/bin/env python3
"""Compare Rust validation results with the Python reference cases."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "conformance" / "cases.json"


def _command_result(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=False)


def _json_output(result: subprocess.CompletedProcess[str], label: str) -> Any:
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        detail = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"{label} returned invalid JSON: {detail}") from exc


def _rust_binary(value: str | None) -> Path:
    if value:
        return Path(value).expanduser()
    for candidate in (ROOT / "target" / "release" / "ttyinv", ROOT / "target" / "debug" / "ttyinv"):
        if candidate.is_file():
            return candidate
    raise RuntimeError("Rust ttyinv binary not found; run `make rust-release` first")


def _case_list() -> list[dict[str, Any]]:
    try:
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise RuntimeError(f"cannot read {MANIFEST}: {exc}") from exc
    cases = manifest.get("cases") if isinstance(manifest, dict) else None
    if not isinstance(cases, list):
        raise RuntimeError(f"{MANIFEST} must contain a cases array")
    return cases


def _check_case(case: dict[str, Any], python_bin: str, rust_bin: Path) -> None:
    name = case.get("name")
    relative_path = case.get("path")
    expected_python_valid = case.get("expected_python_valid", case.get("valid"))
    expected_rust_valid = case.get("expected_rust_valid", case.get("valid"))
    required_codes = case.get("codes", [])
    reason = case.get("reason")
    if (
        not isinstance(name, str)
        or not isinstance(relative_path, str)
        or not isinstance(expected_python_valid, bool)
        or not isinstance(expected_rust_valid, bool)
    ):
        raise RuntimeError(f"invalid case metadata: {case!r}")
    if expected_python_valid != expected_rust_valid and (not isinstance(reason, str) or not reason.strip()):
        raise RuntimeError(f"case {name!r} needs a reason when Python and Rust validity differ")
    if not isinstance(required_codes, list) or not all(isinstance(code, str) for code in required_codes):
        raise RuntimeError(f"case {name!r} has invalid codes")

    source_path = (ROOT / relative_path).resolve()
    try:
        source_path.relative_to(ROOT)
    except ValueError as exc:
        raise RuntimeError(f"case {name!r} path escapes public root: {relative_path!r}") from exc

    python_result = _command_result([python_bin, "-m", "ttyinv", "lint", "--json", str(source_path)])
    if python_result.returncode == 70:
        detail = python_result.stderr.strip() or python_result.stdout.strip()
        raise RuntimeError(f"case {name!r}: Python lint returned unexpected exit 70: {detail}")
    python_valid = python_result.returncode == 0
    if python_valid != expected_python_valid:
        raise RuntimeError(
            f"case {name!r}: Python validity mismatch; expected {expected_python_valid}, got {python_valid} "
            f"(exit {python_result.returncode})"
        )
    _json_output(python_result, f"Python lint for case {name!r}")

    rust_result = _command_result([str(rust_bin), "validate", "--json", str(source_path)])
    if rust_result.returncode == 70:
        detail = rust_result.stderr.strip() or rust_result.stdout.strip()
        raise RuntimeError(f"case {name!r}: Rust validation returned unexpected exit 70: {detail}")
    rust_json = _json_output(rust_result, f"Rust validation for case {name!r}")
    if not isinstance(rust_json, dict):
        raise RuntimeError(f"case {name!r}: Rust JSON is not an object")
    rust_valid = rust_json.get("valid")
    if not isinstance(rust_valid, bool):
        raise RuntimeError(f"case {name!r}: Rust JSON has no boolean valid field")
    if rust_valid != expected_rust_valid:
        raise RuntimeError(
            f"case {name!r}: Rust validity mismatch; expected {expected_rust_valid}, got {rust_valid} "
            f"(exit {rust_result.returncode})"
        )
    diagnostics = rust_json.get("diagnostics")
    if not isinstance(diagnostics, list):
        raise RuntimeError(f"case {name!r}: Rust JSON has no diagnostics array")
    actual_codes = {item.get("code") for item in diagnostics if isinstance(item, dict)}
    missing_codes = [code for code in required_codes if code not in actual_codes]
    if missing_codes:
        raise RuntimeError(f"case {name!r}: Rust diagnostics miss required codes {missing_codes!r}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python", default=sys.executable, help="Python executable for the reference CLI")
    parser.add_argument("--rust", help="Rust ttyinv binary; defaults to release, then debug")
    args = parser.parse_args()
    try:
        rust_bin = _rust_binary(args.rust)
        cases = _case_list()
    except RuntimeError as exc:
        print(f"parity: {exc}", file=sys.stderr)
        return 1

    failures: list[str] = []
    for case in cases:
        if not isinstance(case, dict):
            failures.append(f"invalid case entry: {case!r}")
            continue
        try:
            _check_case(case, args.python, rust_bin)
        except RuntimeError as exc:
            failures.append(str(exc))
            print(f"FAIL {case.get('name', '<unnamed>')}: {exc}", file=sys.stderr)
        else:
            print(f"PASS {case['name']}")
    if failures:
        print(f"Rust parity failed: {len(failures)} of {len(cases)} cases", file=sys.stderr)
        return 1
    print(f"Rust parity passed: {len(cases)} cases")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
