from __future__ import annotations

from decimal import Decimal
from pathlib import Path

from bs4 import BeautifulSoup

from ttyinv.models import AmountPolicy, RenderOptions
from ttyinv.money import calculate_invoice, display_money
from ttyinv.parser import parse_invoice_file
from ttyinv.renderer import render_html


def _fabricated_multi_currency_recap() -> str:
    return """---
schema: ttyinv/v1
invoice:
  number: INV-FABRICATED-RECAP
  title: Synthetic multi-currency recap
  issued: 2026-08-24
  currency: USD
  locale: en-GB
from:
  name: Example Studio
to:
  name: Example Client
---

## Services

| Description | Amount (USD) |
| --- | ---: |
| Fabricated implementation | 100.00 |

<!-- ttyinv:summary-only -->
## Currency recap

| Description | Amount (USD) | Amount (EUR) |
| --- | ---: | ---: |
| Subtotal | 100.00 | 92.00 |
| Grand Total | 100.00 | 92.00 |

## Expenses

| Description | Amount (USD) |
| --- | ---: |
| Fabricated verification | 50.00 |
"""


def test_summary_only_recap_is_positional_displayed_and_not_double_counted(tmp_path: Path) -> None:
    source_path = tmp_path / "fabricated-recap.md"
    source_path.write_text(_fabricated_multi_currency_recap(), encoding="utf-8")
    parsed = parse_invoice_file(source_path)
    calculated = calculate_invoice(parsed, AmountPolicy())

    assert [section.summary_only for section in parsed.sections] == [False, True, False]
    assert [section.summary_only for section in calculated.sections] == [False, True, False]
    assert calculated.sections[1].total == Decimal("0")
    assert calculated.grand_total == Decimal("150")

    html = render_html(
        calculated,
        RenderOptions(theme="light", output_path=tmp_path / "invoice.html", deterministic=True),
    ).html
    soup = BeautifulSoup(html, "html.parser")
    recap = next(section for section in soup.select("section.document-section") if "Currency recap" in section.get_text())
    assert recap.select_one(".section-total") is None
    assert display_money(Decimal("100"), "USD", "en-GB") in recap.get_text()
    assert display_money(Decimal("92"), "EUR", "en-GB") in recap.get_text()
    assert html.count("Total due") == 1
