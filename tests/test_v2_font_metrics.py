from __future__ import annotations

import shutil
import subprocess
from pathlib import Path

import pytest

from ttyinv.font_metrics import inspect_font


def font_path(family: str) -> Path:
    if not shutil.which("fc-match"):
        pytest.skip("fontconfig is not available")
    value = subprocess.check_output(["fc-match", "-f", "%{file}", family], text=True).strip()
    path = Path(value)
    if not path.exists():
        pytest.skip(f"fontconfig did not resolve {family}")
    return path


def test_known_monospace_font_has_one_ascii_advance() -> None:
    metrics = inspect_font(font_path("DejaVu Sans Mono"))
    assert metrics.ascii_monospace
    assert metrics.advance_width is not None
    assert not metrics.missing_ascii


def test_proportional_font_is_not_accepted_as_monospace() -> None:
    metrics = inspect_font(font_path("DejaVu Sans"))
    assert not metrics.ascii_monospace
