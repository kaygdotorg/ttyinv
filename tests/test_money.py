from pathlib import Path

from ttyinv.models import AmountPolicy
from ttyinv.money import calculate_invoice
from ttyinv.parser import parse_invoice_file


def test_calculates_auto_amount_and_grand_total() -> None:
    parsed = parse_invoice_file(Path("examples/simple.md"))
    calculated = calculate_invoice(parsed, AmountPolicy())
    assert calculated.grand_total == 5000
    financial = next(section for section in calculated.sections if section.kind == "financial")
    assert financial.total == 5000
    assert financial.rows[0].amount_source == "calculated"
