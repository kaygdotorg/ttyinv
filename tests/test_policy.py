from pathlib import Path

import pytest

from ttyinv.errors import TtyinvError
from ttyinv.models import AmountPolicy
from ttyinv.money import calculate_invoice
from ttyinv.parser import parse_invoice_file


def test_amount_mismatch_is_an_error(tmp_path: Path) -> None:
    source = Path("examples/simple.md").read_text(encoding="utf-8").replace("| auto |", "| 4999.00 |")
    fixture = tmp_path / "mismatch.md"
    fixture.write_text(source, encoding="utf-8")
    parsed = parse_invoice_file(fixture)
    with pytest.raises(TtyinvError, match="does not match"):
        calculate_invoice(parsed, AmountPolicy())


def test_trust_explicit_keeps_mismatch(tmp_path: Path) -> None:
    source = Path("examples/simple.md").read_text(encoding="utf-8").replace("| auto |", "| 4999.00 |")
    fixture = tmp_path / "mismatch.md"
    fixture.write_text(source, encoding="utf-8")
    parsed = parse_invoice_file(fixture)
    calculated = calculate_invoice(parsed, AmountPolicy(trust_explicit=True))
    assert calculated.grand_total == 4999


def test_output_stem_preserves_version_dots() -> None:
    from ttyinv.cli import _output_paths

    paths = _output_paths(
        Path("invoice.md"),
        Path("dist/ttyinv-0.1.1-preview"),
        "both",
    )
    assert paths["html"] == Path("dist/ttyinv-0.1.1-preview.html")
    assert paths["pdf"] == Path("dist/ttyinv-0.1.1-preview.pdf")
