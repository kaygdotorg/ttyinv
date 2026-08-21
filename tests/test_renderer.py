from pathlib import Path

from ttyinv.models import AmountPolicy, RenderOptions
from ttyinv.money import calculate_invoice
from ttyinv.parser import parse_invoice_file
from ttyinv.renderer import render_html


def test_renders_self_contained_html(tmp_path: Path) -> None:
    parsed = parse_invoice_file(Path("examples/simple.md"))
    calculated = calculate_invoice(parsed, AmountPolicy())
    result = render_html(
        calculated,
        RenderOptions(theme="light", output_path=tmp_path / "invoice.html"),
    )
    assert "<style>" in result.html
    assert "data:image/svg+xml;base64," in result.html
    assert "Total due" in result.html
    assert "€5,000.00" in result.html
    assert '<link rel="stylesheet"' not in result.html
    assert 'font-family: "ttyinv Geist Mono"' in result.html


def test_section_labels_share_payment_alignment(tmp_path: Path) -> None:
    parsed = parse_invoice_file(Path("examples/simple.md"))
    calculated = calculate_invoice(parsed, AmountPolicy())
    result = render_html(
        calculated,
        RenderOptions(theme="dark", output_path=tmp_path / "invoice.html"),
    )

    assert '[ Contract fees ]' in result.html
    assert '[ Notes ]' in result.html
    assert '[ Payment Methods ]' in result.html
    assert ".section-heading" in result.html
    assert "left: 4mm" in result.html
    assert "translateY(-50%)" in result.html
