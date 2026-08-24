from pathlib import Path

import pytest
from bs4 import BeautifulSoup

from ttyinv.errors import TtyinvError
from ttyinv.models import AmountPolicy, RenderOptions
from ttyinv.money import calculate_invoice
from ttyinv.parser import parse_invoice_file
from ttyinv.renderer import _safe_font_family, render_html


def test_renders_self_contained_html(tmp_path: Path) -> None:
    parsed = parse_invoice_file(Path("examples/simple.md"))
    calculated = calculate_invoice(parsed, AmountPolicy())
    result = render_html(
        calculated,
        RenderOptions(theme="light", output_path=tmp_path / "invoice.html"),
    )
    assert "<style>" in result.html
    assert "data:font/" in result.html
    assert "Total due" in result.html
    assert "€\u00a05,200.00" in result.html
    assert '<link rel="stylesheet"' not in result.html
    assert 'src="http' not in result.html


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


def test_numeric_headers_follow_numeric_cells_even_with_stale_left_alignment(tmp_path: Path) -> None:
    source = Path("examples/simple.md").read_text(encoding="utf-8").replace(
        "| --- | ---: | ---: | ---: |",
        "| --- | --- | --- | ---: |",
    )
    invoice_path = tmp_path / "invoice.md"
    invoice_path.write_text(source, encoding="utf-8")
    calculated = calculate_invoice(parse_invoice_file(invoice_path), AmountPolicy())
    html = render_html(
        calculated,
        RenderOptions(theme="light", output_path=tmp_path / "invoice.html"),
    ).html
    headers = {header.get_text(strip=True): header.get("class", []) for header in BeautifulSoup(html, "html.parser").select("th")}
    assert "numeric" in headers["Days"]
    assert "numeric" in headers["Rate"]


@pytest.mark.parametrize(
    "family",
    [
        '</style><script>alert(1)</script>',
        'Mono";background:url(https://example.invalid)',
        "Mono\0evil",
    ],
)
def test_rejects_font_family_html_and_css_breakout(family: str) -> None:
    with pytest.raises(TtyinvError, match="unsafe|control"):
        _safe_font_family(family)


def test_pdf_local_links_do_not_disclose_the_rendering_path(tmp_path: Path) -> None:
    source = Path("examples/simple.md").read_text(encoding="utf-8")
    invoice_path = tmp_path / "invoice.md"
    invoice_path.write_text(
        source.replace(
            "Payment is due within fourteen days.",
            "Payment is due within fourteen days. [Terms](./terms.md)",
        ),
        encoding="utf-8",
    )
    (tmp_path / "terms.md").write_text("Fabricated terms", encoding="utf-8")
    parsed = parse_invoice_file(invoice_path)
    calculated = calculate_invoice(parsed, AmountPolicy())
    result = render_html(
        calculated,
        RenderOptions(theme="light", output_path=tmp_path / "invoice.pdf", for_pdf=True),
    )

    assert 'href="./terms.md"' in result.html
    assert str(tmp_path) not in result.html
