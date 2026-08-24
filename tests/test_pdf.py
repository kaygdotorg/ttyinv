from pathlib import Path

import pytest

from ttyinv.errors import TtyinvError
from ttyinv.models import AmountPolicy, RenderOptions
from ttyinv.money import calculate_invoice
from ttyinv.parser import parse_invoice_file
from ttyinv.pdf import find_chromium, render_pdf
from ttyinv.renderer import render_html


def test_pdf_smoke(tmp_path: Path) -> None:
    try:
        chromium = find_chromium()
    except TtyinvError:
        pytest.skip("a Chromium-based browser is not installed")
    parsed = parse_invoice_file(Path("examples/simple.md"))
    calculated = calculate_invoice(parsed, AmountPolicy())
    pdf_path = tmp_path / "invoice.pdf"
    result = render_html(
        calculated,
        RenderOptions(theme="light", output_path=pdf_path, for_pdf=True),
    )
    render_pdf(result.html, pdf_path, str(chromium))
    assert pdf_path.read_bytes().startswith(b"%PDF")
    assert pdf_path.stat().st_size > 10_000
