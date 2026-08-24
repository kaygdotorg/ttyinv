from pathlib import Path
from decimal import Decimal

from ttyinv.models import AmountPolicy
from ttyinv.money import RATE_ROUNDING_WARNING_CODE, calculate_invoice, display_money
from ttyinv.parser import parse_invoice_file


def test_display_money_uses_nonbreaking_currency_spacing_and_locale_grouping() -> None:
    assert display_money(Decimal("5200"), "EUR", "en-GB") == "€\u00a05,200.00"
    assert display_money(Decimal("560499.97"), "INR", "en-IN") == "₹\u00a05,60,499.97"
    assert display_money(Decimal("5200"), "EUR", "de-DE") == "5.200,00\u00a0€"


def test_calculates_auto_amount_and_grand_total() -> None:
    parsed = parse_invoice_file(Path("examples/simple.md"))
    calculated = calculate_invoice(parsed, AmountPolicy())
    assert calculated.grand_total == 5200
    financial = next(section for section in calculated.sections if section.kind == "financial")
    assert financial.total == 5200
    assert financial.rows[0].amount_source == "calculated"


def test_accepts_fractional_quantity_with_authored_rate_rounding(tmp_path: Path) -> None:
    source = """---
schema: ttyinv/v1
invoice:
  number: INV-FABRICATED-ROUNDING
  issued: 2026-08-01
  currency: EUR
from:
  name: Example Seller
to:
  name: Example Buyer
---

## Services

| Description | Quantity | Rate | Amount (EUR) |
| --- | ---: | ---: | ---: |
| Fabricated work | 21 | 238.10 | 5000.00 |
"""
    path = tmp_path / "fabricated-rounding.md"
    path.write_text(source, encoding="utf-8")
    parsed = parse_invoice_file(path)
    calculated = calculate_invoice(parsed, AmountPolicy())
    financial = next(section for section in calculated.sections if section.kind == "financial")
    assert calculated.grand_total == 5000
    assert financial.rows[0].amount_source == "explicit-rounded-rate"
    assert RATE_ROUNDING_WARNING_CODE in calculated.warnings[0]


def test_rejects_amount_outside_authored_rate_rounding_tolerance(tmp_path: Path) -> None:
    source = """---
schema: ttyinv/v1
invoice:
  number: INV-FABRICATED-OUTSIDE
  issued: 2026-08-01
  currency: EUR
from:
  name: Example Seller
to:
  name: Example Buyer
---

## Services

| Description | Quantity | Rate | Amount (EUR) |
| --- | ---: | ---: | ---: |
| Fabricated work | 21 | 238.10 | 4999.00 |
"""
    path = tmp_path / "fabricated-outside.md"
    path.write_text(source, encoding="utf-8")
    parsed = parse_invoice_file(path)
    try:
        calculate_invoice(parsed, AmountPolicy())
    except Exception as error:
        assert getattr(error, "code", None) == "MONEY004"
    else:
        raise AssertionError("out-of-tolerance amount should be rejected")


def test_fractional_quantity_uses_rate_precision_for_tolerance(tmp_path: Path) -> None:
    source = """---
schema: ttyinv/v1
invoice:
  number: INV-FABRICATED-FRACTION
  issued: 2026-08-01
  currency: EUR
from:
  name: Example Seller
to:
  name: Example Buyer
---

## Services

| Description | Quantity | Rate | Amount (EUR) |
| --- | ---: | ---: | ---: |
| Fractional fabricated work | 2.5 | 6.67 | 16.66 |
"""
    path = tmp_path / "fabricated-fraction.md"
    path.write_text(source, encoding="utf-8")
    calculated = calculate_invoice(parse_invoice_file(path), AmountPolicy())
    assert calculated.grand_total == Decimal("16.66")
    assert calculated.warnings and RATE_ROUNDING_WARNING_CODE in calculated.warnings[0]
