from pathlib import Path

import pytest

from ttyinv.errors import TtyinvError
from ttyinv.parser import parse_invoice_file


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
