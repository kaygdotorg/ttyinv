from __future__ import annotations

from pathlib import Path

import pytest
from bs4 import BeautifulSoup

from ttyinv.errors import TtyinvError
from ttyinv.models import AmountPolicy, RenderOptions
from ttyinv.money import calculate_invoice
from ttyinv.parser import parse_invoice_file
from ttyinv.pdf import find_chromium, render_pdf
from ttyinv.renderer import render_html


def _fabricated_three_page_source() -> str:
    return """---
schema: ttyinv/v1
invoice:
  number: INV-FABRICATED-BREAKS
  title: Synthetic page-break regression
  issued: 2026-08-24
  currency: USD
from:
  name: Example Studio
to:
  name: Example Client
payment:
  title: Payment Methods
  pageBreakBefore: true
  methods:
    - title: Bank transfer
      fields:
        Beneficiary: Example Studio
        Reference: INV-FABRICATED-BREAKS
---

## Services

| Description | Amount (USD) |
| --- | ---: |
| Fabricated implementation | 125.00 |
| Fabricated verification | 75.00 |

<!-- ttyinv:page-break-before -->
## Notes

This forced section is synthetic data for the renderer regression test.
"""


def test_forced_section_and_payment_breaks_keep_one_break_owner_and_shared_top_inset(tmp_path: Path) -> None:
    source_path = tmp_path / "fabricated-breaks.md"
    source_path.write_text(_fabricated_three_page_source(), encoding="utf-8")

    parsed = parse_invoice_file(source_path)
    calculated = calculate_invoice(parsed, AmountPolicy())
    html = render_html(
        calculated,
        RenderOptions(theme="light", output_path=tmp_path / "invoice.html", for_pdf=True, deterministic=True),
    ).html

    assert len(calculated.sections) == 2
    assert calculated.sections[1].page_break_before is True
    assert calculated.frontmatter.payment is not None
    assert calculated.frontmatter.payment.page_break_before is True
    # The section break and payment footer break are both carried into the
    # authoritative HTML. The footer owns the payment break so it does not
    # create a child break inside its no-fragmentation wrapper.
    soup = BeautifulSoup(html, "html.parser")
    assert len(soup.select('[data-page-break-before="true"]')) == 2
    assert ".document-section[data-page-break-before=\"true\"]" in html
    assert ".footer-stack[data-page-break-before=\"true\"]" in html
    assert not soup.select('.payment-frame[data-page-break-before="true"]')
    assert ".footer-stack[data-page-break-before=\"true\"] { margin-top: 5mm; }" in html
    assert '.document-section + .document-section:not([data-page-break-before="true"])' in html
    assert "--print-page-top-margin" not in html
    assert "margin-top: var(--print-page-top-margin)" not in html


def test_fabricated_forced_break_pdf_is_three_pages_when_chromium_is_available(tmp_path: Path) -> None:
    try:
        chromium = find_chromium()
    except TtyinvError:
        pytest.skip("a Chromium-based browser is not installed")

    source_path = tmp_path / "fabricated-breaks.md"
    source_path.write_text(_fabricated_three_page_source(), encoding="utf-8")
    calculated = calculate_invoice(parse_invoice_file(source_path), AmountPolicy())
    rendered = render_html(
        calculated,
        RenderOptions(theme="light", output_path=tmp_path / "invoice.pdf", for_pdf=True, deterministic=True),
    )
    pdf_path = tmp_path / "invoice.pdf"
    render_pdf(rendered.html, pdf_path, str(chromium))

    from pypdf import PdfReader

    assert len(PdfReader(str(pdf_path)).pages) == 3
