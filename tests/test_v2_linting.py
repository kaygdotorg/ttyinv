from __future__ import annotations

from pathlib import Path

from ttyinv.cli_v2 import ExitCode, main
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
    assert mismatch.column is not None
    assert mismatch.section == "Contract fees"
    assert mismatch.row == 1
    assert mismatch.column_name == "Amount (EUR)"


def test_rounded_authored_rate_is_a_warning(tmp_path: Path) -> None:
    source = VALID.replace(
        "| Consulting | 2 | 100.00 | auto |",
        "| Consulting | 21 | 238.10 | 5000.00 |",
    )
    diagnostics = lint_source(tmp_path / "invoice.md", source=source)
    assert not [item for item in diagnostics if item.severity == "error"]
    rounded = next(item for item in diagnostics if item.code == "MONEY007")
    assert rounded.severity == "warning"
    assert "5000" not in rounded.message


def test_rounded_rate_tolerance_handles_fractional_quantity(tmp_path: Path) -> None:
    source = VALID.replace(
        "| Consulting | 2 | 100.00 | auto |",
        "| Consulting | 2.5 | 6.67 | 16.66 |",
    )
    diagnostics = lint_source(tmp_path / "invoice.md", source=source)
    assert "MONEY004" not in {item.code for item in diagnostics}
    assert "MONEY007" in {item.code for item in diagnostics}


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


def test_unknown_field_has_precise_yaml_key_location(tmp_path: Path) -> None:
    path = tmp_path / "invoice.md"
    source = VALID.replace("  currency: EUR", "  currency: EUR\n  mystery: nope")
    diagnostic = next(item for item in lint_source(path, source=source) if item.code == "SCHEMA006")
    assert (diagnostic.line, diagnostic.column) == (8, 3)
    assert "invoice.mystery" in diagnostic.message


def test_malformed_separator_has_section_and_source_location(tmp_path: Path) -> None:
    path = tmp_path / "invoice.md"
    source = VALID.replace("| --- | ---: | ---: | ---: |", "| -- | ---: | ---: | ---: |")
    diagnostic = next(item for item in lint_source(path, source=source) if item.code == "MD002")
    assert (diagnostic.line, diagnostic.column) == (17, 1)
    assert diagnostic.section == "Contract fees"
    assert diagnostic.hint


def test_missing_asset_has_source_location_and_section(tmp_path: Path) -> None:
    path = tmp_path / "invoice.md"
    source = VALID + "\n![Missing](./absent.svg)\n"
    diagnostic = next(item for item in lint_source(path, source=source) if item.code == "PATH002")
    assert diagnostic.line is not None and diagnostic.column is not None
    assert diagnostic.section == "Contract fees"


def test_cli_uses_stable_failure_categories_and_lint_writes_no_artifacts(tmp_path: Path) -> None:
    path = tmp_path / "invoice.md"
    path.write_text(VALID, encoding="utf-8")
    before = sorted(tmp_path.iterdir())
    assert main(["lint", str(path)]) == ExitCode.OK
    assert sorted(tmp_path.iterdir()) == before

    path.write_text(VALID.replace("schema: ttyinv/v1", "schema: ["), encoding="utf-8")
    assert main(["lint", str(path)]) == ExitCode.PARSE_SCHEMA
    invalid_invoice = VALID.replace(
        "invoice:\n  number: INV-1\n  issued: 2026-01-01\n  due: 2026-01-15\n  currency: EUR\n",
        "invoice: invalid\n",
    )
    path.write_text(invalid_invoice, encoding="utf-8")
    assert main(["lint", str(path)]) == ExitCode.PARSE_SCHEMA
    path.write_text(VALID.replace("auto |", "199.00 |"), encoding="utf-8")
    assert main(["lint", str(path)]) == ExitCode.ARITHMETIC
    path.write_text(VALID + "\n![Missing](./absent.svg)\n", encoding="utf-8")
    assert main(["lint", str(path)]) == ExitCode.ASSET_SECURITY
