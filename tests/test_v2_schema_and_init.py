from __future__ import annotations

import json
from pathlib import Path

from ttyinv.cli_v2 import ExitCode, main
from ttyinv.schema_v1 import schema, schema_json


def test_schema_is_versioned_and_requires_core_fields() -> None:
    document = schema()
    assert document["properties"]["schema"]["const"] == "ttyinv/v1"
    assert set(document["required"]) == {"schema", "invoice", "from", "to"}


def test_schema_command_writes_json(tmp_path: Path) -> None:
    output = tmp_path / "ttyinv-v1.schema.json"
    assert main(["schema", "--output", str(output)]) == 0
    parsed = json.loads(output.read_text(encoding="utf-8"))
    assert parsed["title"].startswith("ttyinv/v1")
    assert "×" in schema_json()
    assert r"\u00d7" not in schema_json()
    assert schema_json() == schema_json()


def test_init_writes_only_fabricated_starter_data(tmp_path: Path) -> None:
    invoice = tmp_path / "invoice.md"
    assert main(["init", str(invoice)]) == 0
    content = invoice.read_text(encoding="utf-8")
    assert "Example Studio" in content
    assert "Example Client" in content
    assert "Example Bank" in content
    assert "ttyinv/v1" in content


def test_init_refuses_to_overwrite_without_force(tmp_path: Path) -> None:
    invoice = tmp_path / "invoice.md"
    invoice.write_text("keep me", encoding="utf-8")
    assert main(["init", str(invoice)]) == ExitCode.USAGE
    assert invoice.read_text(encoding="utf-8") == "keep me"


def test_init_can_write_only_fabricated_svg_assets(tmp_path: Path) -> None:
    invoice = tmp_path / "invoice.md"
    assert main(["init", str(invoice), "--with-assets"]) == ExitCode.OK
    content = invoice.read_text(encoding="utf-8")
    assert "./assets/logo.svg" in content
    assert "./assets/signature.svg" in content
    logo = (tmp_path / "assets" / "logo.svg").read_text(encoding="utf-8")
    signature = (tmp_path / "assets" / "signature.svg").read_text(encoding="utf-8")
    assert "EXAMPLE STUDIO" in logo
    assert "Fabricated example signature" in signature
