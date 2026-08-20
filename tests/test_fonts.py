from pathlib import Path

import pytest
from click.testing import CliRunner

from ttyinv.cli import main
from ttyinv.errors import TtyinvError
from ttyinv.fonts import list_monospace_families, system_font_assets


def test_lists_verified_system_monospace_fonts() -> None:
    families = list_monospace_families()
    assert "DejaVu Sans Mono" in families
    assert "DejaVu Sans" not in families


def test_embeds_selected_system_monospace_font() -> None:
    assets = system_font_assets("DejaVu Sans Mono")
    assert assets.family == "DejaVu Sans Mono"
    assert assets.regular and assets.regular.startswith("data:font/")
    assert assets.strong and assets.strong.startswith("data:font/")


def test_rejects_proportional_font() -> None:
    with pytest.raises(TtyinvError, match="not found or is not a Latin monospace"):
        system_font_assets("DejaVu Sans")


def test_list_fonts_cli_does_not_require_invoice() -> None:
    result = CliRunner().invoke(main, ["--list-fonts"])
    assert result.exit_code == 0
    assert "DejaVu Sans Mono" in result.output


def test_cli_accepts_accent_and_font(tmp_path: Path) -> None:
    output = tmp_path / "invoice"
    result = CliRunner().invoke(
        main,
        [
            "examples/reference.md",
            "--format",
            "html",
            "--font",
            "DejaVu Sans Mono",
            "--accent",
            "oklch(72% 0.19 35)",
            "--output",
            str(output),
        ],
    )
    assert result.exit_code == 0, result.output
    html = output.with_suffix(".html").read_text(encoding="utf-8")
    assert "oklch(72% 0.19 35)" in html
    assert "ttyinv DejaVu Sans Mono" in html
