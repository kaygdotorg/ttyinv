from pathlib import Path

from ttyinv.models import AmountPolicy, RenderOptions
from ttyinv.money import calculate_invoice
from ttyinv.parser import parse_invoice_file
from ttyinv.renderer import render_html


def _render(tmp_path: Path) -> str:
    parsed = parse_invoice_file(Path("examples/reference.md"))
    calculated = calculate_invoice(parsed, AmountPolicy())
    return render_html(
        calculated,
        RenderOptions(
            theme="dark",
            output_path=tmp_path / "invoice.html",
        ),
    ).html


def test_page_frame_uses_one_border_and_typographic_corners(tmp_path: Path) -> None:
    html = _render(tmp_path)
    assert 'class="page-frame"' in html
    for corner in ("tl", "tr", "bl", "br"):
        assert f'class="frame-corner {corner}"' in html
    assert html.count(">+</span>") == 4
    assert "frame-edge" not in html
    assert "frame-junction" not in html
    assert "border: var(--stroke) dashed var(--rule)" in html


def test_table_and_total_share_the_same_five_column_grid(tmp_path: Path) -> None:
    html = _render(tmp_path)
    assert 'data-column-widths="45,10,17,10,18"' in html
    assert '<col style="width:45%"/>' in html
    assert "grid-template-columns:45fr 10fr 17fr 10fr 18fr" in html
    assert 'class="summary-rule" style="grid-column:4 / 6"' in html
    assert 'class="summary-label" style="grid-column:4 / 5"' in html
    assert 'class="summary-amount" style="grid-column:5 / 6"' in html
    assert "table-end-rule" in html
    assert "text-align: right" in html
    assert "white-space: nowrap" in html


def test_invoice_tables_have_no_outer_border(tmp_path: Path) -> None:
    html = _render(tmp_path)
    assert ".invoice-table {" in html
    assert "border: 0;" in html
    assert "border-collapse: collapse" in html


def test_single_financial_section_has_only_grand_total(tmp_path: Path) -> None:
    html = _render(tmp_path)
    assert 'class="aligned-summary grand-total"' in html
    assert 'class="aligned-summary section-total"' not in html
    assert "Total due" in html


def test_section_titles_share_payment_alignment(tmp_path: Path) -> None:
    html = _render(tmp_path)
    assert "[ Contract fees ]" in html
    assert "[ Notes ]" in html
    assert "[ Payment Methods ]" in html
    assert "left: 4mm" in html
    assert "top: 0" in html
    assert "translateY(-50%)" in html
