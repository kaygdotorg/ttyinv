import re
from decimal import Decimal
from pathlib import Path

import pytest
from bs4 import BeautifulSoup

from ttyinv.dates import display_date
from ttyinv.errors import TtyinvError
from ttyinv.models import AmountPolicy, InvoiceMeta, MoneyValue, RenderOptions, Settlement
from ttyinv.money import calculate_invoice, display_money
from ttyinv.linting import lint_source
from ttyinv.parser import parse_invoice_file
from ttyinv.renderer import render_html
from ttyinv.schema_v1 import schema

def test_due_date_cannot_precede_issue_date(tmp_path: Path) -> None:
    source = Path("examples/simple.md").read_text(encoding="utf-8")
    invoice = tmp_path / "invalid-date-order.md"
    invoice.write_text(source.replace("due: 2026-01-29", "due: 2026-01-14"), encoding="utf-8")

    with pytest.raises(TtyinvError, match="due date must be on or after issue date"):
        parse_invoice_file(invoice)


def test_issue_and_due_date_may_match(tmp_path: Path) -> None:
    source = Path("examples/simple.md").read_text(encoding="utf-8")
    invoice = tmp_path / "same-day.md"
    invoice.write_text(source.replace("due: 2026-01-29", "due: 2026-01-15"), encoding="utf-8")

    parsed = parse_invoice_file(invoice)

    assert parsed.frontmatter.invoice.due == "2026-01-15"


def test_structured_dates_reject_invalid_calendar_days_and_accept_leap_days() -> None:
    with pytest.raises(ValueError, match="real calendar date"):
        InvoiceMeta(number="INV-1", issued="2026-02-30", currency="EUR")
    with pytest.raises(ValueError, match="real calendar date"):
        InvoiceMeta(number="INV-0", issued="0000-01-01", currency="EUR")

    leap_day = InvoiceMeta(number="INV-2", issued="2024-02-29", currency="EUR")
    assert display_date(leap_day.issued) == "2024-02-29"
    with pytest.raises(ValueError, match="real calendar date"):
        Settlement(date="2026-02-30", paid=MoneyValue(amount=Decimal("1"), currency="EUR"))


def test_public_schema_rejects_invalid_structured_dates() -> None:
    pattern = schema()["properties"]["invoice"]["properties"]["issued"]["pattern"]
    assert isinstance(pattern, str)
    assert re.fullmatch(pattern, "2024-02-29")
    assert not re.fullmatch(pattern, "2026-02-30")
    assert not re.fullmatch(pattern, "0000-01-01")
    assert not re.fullmatch(pattern, "٢٠٢٤-02-29")


def test_renderer_keeps_dates_iso_when_locale_changes(tmp_path: Path) -> None:
    source = Path("examples/simple.md").read_text(encoding="utf-8")
    first_path = tmp_path / "en.md"
    second_path = tmp_path / "de.md"
    first_path.write_text(source.replace("locale: en-GB", "locale: en-GB"), encoding="utf-8")
    second_path.write_text(source.replace("locale: en-GB", "locale: de-DE"), encoding="utf-8")

    first = render_html(
        calculate_invoice(parse_invoice_file(first_path), AmountPolicy()),
        RenderOptions(theme="light", output_path=tmp_path / "en.html"),
    ).html
    second = render_html(
        calculate_invoice(parse_invoice_file(second_path), AmountPolicy()),
        RenderOptions(theme="light", output_path=tmp_path / "de.html"),
    ).html
    first_meta = BeautifulSoup(first, "html.parser").select_one(".invoice-meta")
    second_meta = BeautifulSoup(second, "html.parser").select_one(".invoice-meta")
    assert first_meta is not None and second_meta is not None
    assert first_meta.get_text(" ", strip=True) == second_meta.get_text(" ", strip=True)
    assert "2026-01-15" in first_meta.get_text(" ", strip=True)
    assert "2026-01-29" in first_meta.get_text(" ", strip=True)
    assert display_money(Decimal("100"), "EUR", "en-GB") != display_money(Decimal("100"), "EUR", "de-DE")


def test_lint_reports_invalid_iso_date_without_yaml_failure(tmp_path: Path) -> None:
    source = Path("examples/simple.md").read_text(encoding="utf-8")
    invoice = tmp_path / "invalid-date.md"
    invoice.write_text(source.replace("issued: 2026-01-15", "issued: 2026-02-30"), encoding="utf-8")

    diagnostics = lint_source(invoice)

    assert any(item.code == "SCHEMA005" and "real calendar date" in item.message for item in diagnostics)


def test_lint_rejects_self_referential_yaml_alias(tmp_path: Path) -> None:
    source = "---\na: &cycle\n  - *cycle\n---\n"

    diagnostics = lint_source(tmp_path / "cycle.md", source=source)

    assert any(item.code == "YAML002" for item in diagnostics)


def test_lint_rejects_yaml_beyond_depth_limit(tmp_path: Path) -> None:
    from ttyinv.yaml_support import MAX_YAML_DEPTH

    source = "---\n" + "".join(f'{"  " * index}a:\n' for index in range(MAX_YAML_DEPTH + 1))
    source += f'{"  " * (MAX_YAML_DEPTH + 1)}value: x\n---\n'

    diagnostics = lint_source(tmp_path / "deep.md", source=source)

    assert any(item.code == "YAML002" for item in diagnostics)


def test_lint_rejects_sequence_yaml_mapping_key(tmp_path: Path) -> None:
    source = "---\n? [a, b]\n: value\n---\n"

    diagnostics = lint_source(tmp_path / "sequence-key.md", source=source)

    assert any(item.code == "YAML002" for item in diagnostics)


def test_parser_read_error_does_not_disclose_path(tmp_path: Path) -> None:
    path = tmp_path / "missing-invoice.md"

    with pytest.raises(TtyinvError) as raised:
        parse_invoice_file(path)

    assert str(path) not in str(raised.value)


def test_fenced_markdown_table_example_validates_cleanly(tmp_path: Path) -> None:
    source = Path("conformance/cases/fenced-table.md").read_text(encoding="utf-8")
    invoice = tmp_path / "fenced-table.md"
    invoice.write_text(source, encoding="utf-8")

    assert not lint_source(invoice)
    parsed = parse_invoice_file(invoice)
    calculated = calculate_invoice(parsed, AmountPolicy())
    render_html(calculated, RenderOptions(theme="light", output_path=tmp_path / "invoice.html"))
