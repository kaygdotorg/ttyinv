from __future__ import annotations

from pathlib import Path

from ttyinv.linting import lint_source


VALID = """---
schema: ttyinv/v1
invoice:
  number: INV-1
  issued: 2026-01-01
  due: 2026-01-15
  currency: EUR
from:
  name: Example Seller
to:
  name: Example Buyer
---

## Contract fees

| Description | Days | Rate | Amount (EUR) |
| --- | ---: | ---: | ---: |
| Consulting | 2 | 100.00 | auto |
"""


def codes(path: Path, source: str, **kwargs: object) -> set[str]:
    return {item.code for item in lint_source(path, source=source, **kwargs)}


def test_valid_invoice_has_no_errors(tmp_path: Path) -> None:
    path = tmp_path / "invoice.md"
    diagnostics = lint_source(path, source=VALID)
    assert not [item for item in diagnostics if item.severity == "error"]


def test_amount_mismatch_has_source_line(tmp_path: Path) -> None:
    source = VALID.replace("| Consulting | 2 | 100.00 | auto |", "| Consulting | 2 | 100.00 | 199.00 |")
    path = tmp_path / "invoice.md"
    diagnostics = lint_source(path, source=source)
    mismatch = next(item for item in diagnostics if item.code == "MONEY004")
    assert mismatch.line is not None
    assert mismatch.path == str(path)


def test_trust_explicit_downgrades_mismatch(tmp_path: Path) -> None:
    source = VALID.replace("| Consulting | 2 | 100.00 | auto |", "| Consulting | 2 | 100.00 | 199.00 |")
    assert "MONEY004" not in codes(tmp_path / "invoice.md", source, amount_policy="trust-explicit")
    assert "MONEY005" in codes(tmp_path / "invoice.md", source, amount_policy="trust-explicit")


def test_duplicate_yaml_keys_are_rejected(tmp_path: Path) -> None:
    source = VALID.replace("  number: INV-1", "  number: INV-1\n  number: INV-2")
    assert "YAML002" in codes(tmp_path / "invoice.md", source)


def test_authored_total_row_is_warned(tmp_path: Path) -> None:
    source = VALID.replace("| Consulting | 2 | 100.00 | auto |", "| Consulting | 2 | 100.00 | auto |\n| TOTAL | 2 | 100.00 | 200.00 |")
    assert "MONEY001" in codes(tmp_path / "invoice.md", source)
