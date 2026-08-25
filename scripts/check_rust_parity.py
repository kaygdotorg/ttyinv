#!/usr/bin/env python3
"""Compare Rust validation results with the Python reference cases."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "conformance" / "cases.json"
VALID_SEVERITIES = frozenset({"error", "warning"})


@dataclass
class CaseOutcome:
    name: str
    python_valid: bool | None = None
    rust_valid: bool | None = None
    rust_exit: int | None = None
    rust_diagnostics: tuple[tuple[str, str], ...] = ()
    failures: list[str] | None = None

    def __post_init__(self) -> None:
        if self.failures is None:
            self.failures = []

    @property
    def passed(self) -> bool:
        return not self.failures


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


def _expected_diagnostics(case: dict[str, Any], name: str) -> set[tuple[str, str]]:
    expected = case.get("expected_rust_diagnostics")
    if not isinstance(expected, list):
        raise RuntimeError(f"case {name!r} must define expected_rust_diagnostics")

    result: set[tuple[str, str]] = set()
    for item in expected:
        if not isinstance(item, dict):
            raise RuntimeError(f"case {name!r} has an invalid Rust diagnostic expectation")
        code = item.get("code")
        severity = item.get("severity")
        if (
            not isinstance(code, str)
            or not code
            or not isinstance(severity, str)
            or severity not in VALID_SEVERITIES
        ):
            raise RuntimeError(f"case {name!r} has an invalid Rust diagnostic expectation: {item!r}")
        pair = (code, severity)
        if pair in result:
            raise RuntimeError(f"case {name!r} repeats Rust diagnostic expectation {item!r}")
        if any(existing_code == code for existing_code, _ in result):
            raise RuntimeError(f"case {name!r} assigns multiple severities to {code!r}")
        result.add(pair)
    return result


def _actual_diagnostics(value: Any, name: str) -> tuple[set[tuple[str, str]], list[str]]:
    if not isinstance(value, list):
        return set(), [f"case {name!r}: Rust JSON has no diagnostics array"]

    diagnostics: set[tuple[str, str]] = set()
    failures: list[str] = []
    for item in value:
        if not isinstance(item, dict):
            failures.append(f"case {name!r}: Rust diagnostics contains a non-object")
            continue
        code = item.get("code")
        severity = item.get("severity")
        if not isinstance(code, str) or not isinstance(severity, str):
            failures.append(f"case {name!r}: Rust diagnostic has invalid code or severity: {item!r}")
            continue
        diagnostics.add((code, severity))
    return diagnostics, failures


def _format_diagnostics(diagnostics: set[tuple[str, str]] | tuple[tuple[str, str], ...]) -> str:
    if not diagnostics:
        return "-"
    return ", ".join(f"{code}:{severity}" for code, severity in sorted(diagnostics))


def _check_diagnostics(
    expected: set[tuple[str, str]], actual: set[tuple[str, str]], name: str
) -> list[str]:
    failures: list[str] = []
    expected_codes = {code for code, _ in expected}
    actual_codes = {code for code, _ in actual}
    severity_mismatches = sorted(
        (code, expected_severity, actual_severity)
        for code, expected_severity in expected
        for actual_code, actual_severity in actual
        if code == actual_code and expected_severity != actual_severity
    )
    if severity_mismatches:
        details = ", ".join(
            f"{code} expected {expected_severity}, got {actual_severity}"
            for code, expected_severity, actual_severity in severity_mismatches
        )
        failures.append(f"case {name!r}: Rust diagnostic severity mismatch: {details}")

    missing_codes = sorted(expected_codes - actual_codes)
    if missing_codes:
        failures.append(f"case {name!r}: Rust diagnostics miss required codes {missing_codes!r}")
    extra_codes = sorted(actual_codes - expected_codes)
    if extra_codes:
        failures.append(f"case {name!r}: Rust diagnostics contain unexpected codes {extra_codes!r}")
    if not severity_mismatches and expected_codes == actual_codes and expected != actual:
        failures.append(
            f"case {name!r}: Rust diagnostics mismatch; expected "
            f"{_format_diagnostics(expected)}, got {_format_diagnostics(actual)}"
        )
    return failures


def _check_case(case: dict[str, Any], python_bin: str, rust_bin: Path) -> CaseOutcome:
    name = case.get("name")
    relative_path = case.get("path")
    expected_python_valid = case.get("expected_python_valid")
    expected_rust_valid = case.get("expected_rust_valid")
    expected_exit = case.get("expected_exit")
    reason = case.get("reason")
    if (
        not isinstance(name, str)
        or not isinstance(relative_path, str)
        or not isinstance(expected_python_valid, bool)
        or not isinstance(expected_rust_valid, bool)
        or not isinstance(expected_exit, int)
        or isinstance(expected_exit, bool)
    ):
        raise RuntimeError(f"invalid case metadata: {case!r}")
    expected_diagnostics = _expected_diagnostics(case, name)
    if expected_python_valid != expected_rust_valid and (not isinstance(reason, str) or not reason.strip()):
        raise RuntimeError(f"case {name!r} needs a reason when Python and Rust validity differ")

    source_path = (ROOT / relative_path).resolve()
    try:
        source_path.relative_to(ROOT)
    except ValueError as exc:
        raise RuntimeError(f"case {name!r} path escapes public root: {relative_path!r}") from exc

    outcome = CaseOutcome(name=name)
    failures = outcome.failures
    assert failures is not None

    python_result = _command_result([python_bin, "-m", "ttyinv", "lint", "--json", str(source_path)])
    outcome.python_valid = python_result.returncode == 0
    if python_result.returncode == 70:
        detail = python_result.stderr.strip() or python_result.stdout.strip()
        failures.append(f"case {name!r}: Python lint returned unexpected exit 70: {detail}")
    if outcome.python_valid != expected_python_valid:
        failures.append(
            f"case {name!r}: Python validity mismatch; expected {expected_python_valid}, "
            f"got {outcome.python_valid} (exit {python_result.returncode})"
        )
    try:
        _json_output(python_result, f"Python lint for case {name!r}")
    except RuntimeError as exc:
        failures.append(str(exc))

    rust_result = _command_result([str(rust_bin), "validate", "--json", str(source_path)])
    outcome.rust_exit = rust_result.returncode
    if rust_result.returncode != expected_exit:
        failures.append(
            f"case {name!r}: Rust exit mismatch; expected {expected_exit}, got {rust_result.returncode}"
        )
    try:
        rust_json = _json_output(rust_result, f"Rust validation for case {name!r}")
    except RuntimeError as exc:
        failures.append(str(exc))
        return outcome
    if not isinstance(rust_json, dict):
        failures.append(f"case {name!r}: Rust JSON is not an object")
        return outcome

    rust_valid = rust_json.get("valid")
    if not isinstance(rust_valid, bool):
        failures.append(f"case {name!r}: Rust JSON has no boolean valid field")
    else:
        outcome.rust_valid = rust_valid
        if rust_valid != expected_rust_valid:
            failures.append(
                f"case {name!r}: Rust validity mismatch; expected {expected_rust_valid}, "
                f"got {rust_valid} (exit {rust_result.returncode})"
            )

    actual_diagnostics, diagnostic_failures = _actual_diagnostics(rust_json.get("diagnostics"), name)
    outcome.rust_diagnostics = tuple(sorted(actual_diagnostics))
    failures.extend(diagnostic_failures)
    failures.extend(_check_diagnostics(expected_diagnostics, actual_diagnostics, name))
    return outcome


def _print_table(outcomes: list[CaseOutcome]) -> None:
    headers = ("CASE", "PYTHON", "RUST", "EXIT", "DIAGNOSTICS", "RESULT")
    rows = [
        (
            outcome.name,
            "-" if outcome.python_valid is None else str(outcome.python_valid).lower(),
            "-" if outcome.rust_valid is None else str(outcome.rust_valid).lower(),
            "-" if outcome.rust_exit is None else str(outcome.rust_exit),
            _format_diagnostics(outcome.rust_diagnostics),
            "PASS" if outcome.passed else "FAIL",
        )
        for outcome in outcomes
    ]
    widths = [len(header) for header in headers]
    for row in rows:
        for index, value in enumerate(row):
            widths[index] = max(widths[index], len(value))
    print("  ".join(header.ljust(widths[index]) for index, header in enumerate(headers)))
    print("  ".join("-" * width for width in widths))
    for row in rows:
        print("  ".join(value.ljust(widths[index]) for index, value in enumerate(row)))


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

    outcomes: list[CaseOutcome] = []
    for case in cases:
        if not isinstance(case, dict):
            outcomes.append(CaseOutcome(name="<unnamed>", failures=[f"invalid case entry: {case!r}"]))
            continue
        try:
            outcome = _check_case(case, args.python, rust_bin)
        except RuntimeError as exc:
            outcome = CaseOutcome(name=str(case.get("name", "<unnamed>")), failures=[str(exc)])
        outcomes.append(outcome)

    _print_table(outcomes)
    failures = [failure for outcome in outcomes for failure in (outcome.failures or [])]
    if failures:
        print("", file=sys.stderr)
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        print(f"Rust parity failed: {len(failures)} failure(s) in {len(cases)} cases", file=sys.stderr)
        return 1
    print(f"Rust parity passed: {len(cases)} cases")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
