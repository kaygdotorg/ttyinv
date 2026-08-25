from __future__ import annotations

import json
from pathlib import Path
import subprocess

import pytest

from scripts import check_rust_parity


CASE = {
    "name": "parity-test",
    "path": "conformance/cases/simple-valid.md",
    "expected_python_valid": False,
    "expected_rust_valid": False,
    "expected_exit": 1,
    "expected_rust_diagnostics": [{"code": "DATE001", "severity": "error"}],
    "reason": None,
}


@pytest.mark.parametrize(
    ("diagnostic", "message"),
    [
        ({"code": "SCHEMA001", "severity": "error"}, "unexpected codes"),
        ({"code": "DATE001", "severity": "warning"}, "severity mismatch"),
    ],
)
def test_rust_diagnostic_code_and_severity_drift_fails_gate(
    monkeypatch: pytest.MonkeyPatch, diagnostic: dict[str, str], message: str
) -> None:
    def fake_command(command: list[str]) -> subprocess.CompletedProcess[str]:
        if "validate" in command:
            return subprocess.CompletedProcess(
                command,
                1,
                json.dumps({"valid": False, "diagnostics": [diagnostic]}),
                "",
            )
        return subprocess.CompletedProcess(command, 1, '{"valid": false}', "")

    monkeypatch.setattr(check_rust_parity, "_command_result", fake_command)

    outcome = check_rust_parity._check_case(CASE, "python", Path("ttyinv"))

    assert not outcome.passed
    assert any(message in failure for failure in outcome.failures or [])
